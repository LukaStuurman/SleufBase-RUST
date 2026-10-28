//! KickTheMap browser-account and jobs workspace for native desktop.
//! Browser auth, filtering, persistent offline snapshots and queued downloads
//! are coordinated without blocking egui's main thread.
use super::*;
use sleufbase_rust::{
    kickthemap_browser::Browser,
    jobs_cache::Job,
};
use std::{collections::{BTreeSet,VecDeque},sync::{Arc,atomic::{AtomicBool,Ordering}}};
use sleufbase_rust::osm_tiles::{self,OsmRaster};

const JOB_COLUMNS:[(&str,f32);7]=[
    ("Jobnaam",205.),("Adres",290.),("Coordinaten",150.),
    ("Aanmaak datum",140.),("Status",150.),("Gemeente",145.),("Job ID",90.),
];
fn job_status_text(status:i32)->&'static str{
    match status{
        1=>"Aangemaakt",2=>"Wacht op verwerking",3|4=>"In wachtrij",
        5=>"Verwerken",6=>"Controle nodig",7=>"Upload bezig",
        8=>"Mislukt",9|10=>"Gereed",11=>"Wacht op bestanden",
        _=>"Onbekend",
    }
}
fn job_column_value(job:&Job,column:usize)->String{
    match column{
        0=>job.title.clone(),1=>job.address.clone(),
        2=>job.coordinates.clone(),3=>job.client_date.clone(),
        4=>job_status_text(job.status).into(),5=>job.municipality.clone(),
        6=>job.job_id.to_string(),_=>String::new(),
    }
}
fn job_matches_filters(job:&Job,filters:&[String;7])->bool{
    filters.iter().enumerate().all(|(column,filter)|{
        filter.trim().is_empty()||job_column_value(job,column)
            .to_lowercase().contains(&filter.trim().to_lowercase())
    })
}
enum RepairSource{File(PathBuf),Browser(sleufbase_rust::kickthemap_browser::BrowserChannel)}
pub(super) struct BrowserUi {
    pub(super) open:bool,
    pub(super) browser:Option<Browser>,
    launching:Option<Receiver<Result<Browser,String>>>,
    #[cfg(test)] pub(super) launch_requests:usize,
    pub(super) capturing:Option<Receiver<(Browser,Result<(KickTheMapLiveClient,Vec<Job>),String>)>>,
    pub(super) selected:BTreeSet<u64>,
    filters:[String;7],
    only_downloadable:bool,
    only_loaded:bool,
    only_selected:bool,
    sort_column:usize,
    sort_descending:bool,
    map_open:bool,
    map_bounds:Option<Bounds>,
    map_texture:Option<(Bounds,egui::TextureHandle)>,
    map_loading:Option<Receiver<Result<OsmRaster,String>>>,
    map_status:String,
    opening_job:Option<Receiver<(Browser,Result<(),String>)>>,
    open_target:Option<u64>,
    pub(super) queue:VecDeque<(u64,RemoteFileKind)>,
    pub(super) batch_mode:bool,
    batch_generation:u64,
    pub(super) downloaded_json:Vec<PathBuf>,
    pub(super) downloaded_tiffs:Vec<PathBuf>,
    pub(super) completed:usize,
    pub(super) failures:Vec<String>,
    pub(super) cancel:Arc<AtomicBool>,
    pub(super) download_notice:Option<Receiver<String>>,
    capture_status:Option<Receiver<String>>,
    pub(super) status:String,
}
impl Default for BrowserUi {
    fn default()->Self {
        Self {open:false,browser:None,launching:None,capturing:None,
            #[cfg(test)] launch_requests:0,
            selected:BTreeSet::new(),filters:Default::default(),
            only_downloadable:false,only_loaded:false,only_selected:false,
            sort_column:3,sort_descending:true,
            map_open:false,map_bounds:None,map_texture:None,map_loading:None,
            map_status:String::new(),opening_job:None,open_target:None,
            queue:VecDeque::new(),batch_mode:false,
            batch_generation:0,
            downloaded_json:Vec::new(),downloaded_tiffs:Vec::new(),
            completed:0,failures:Vec::new(),cancel:Arc::new(AtomicBool::new(false)),
            download_notice:None,capture_status:None,status:String::new()}
    }
}
impl SleufBaseApp {
    fn repair_kickthemap_download(&mut self,id:u64,path:PathBuf) {
        self.start_kickthemap_repair(id,RepairSource::File(path));
    }
    fn capture_kickthemap_download(&mut self,id:u64){
        let Some(browser)=self.browser_ui.browser.as_ref() else{return;};
        self.start_kickthemap_repair(id,RepairSource::Browser(browser.channel()));
    }
    fn start_kickthemap_repair(&mut self,id:u64,source:RepairSource) {
        if self.live_operation_busy() || self.browser_ui.batch_mode { return; }
        let Some(session)=self.live_session.as_ref() else {return;};
        if self.live_jobs_owner.as_ref()!=Some(session.snapshot_owner()) {
            self.message="Vernieuw eerst de jobs van de huidige sessie".into();return;
        }
        let Some(job)=self.live_jobs.iter().find(|j|j.job_id==id).cloned() else {return;};
        let mut session=self.live_session.take().unwrap();
        let (tx,rx)=mpsc::channel();
        let (notice_tx,notice_rx)=mpsc::sync_channel(1);
        let (status_tx,status_rx)=mpsc::sync_channel(1);
        self.browser_ui.capture_status=Some(status_rx);
        self.browser_ui.download_notice=Some(notice_rx);
        self.live_download_generation=self.project_generation;
        self.live_download_rx=Some(rx);
        self.live_busy=true;
        self.browser_ui.cancel.store(false,Ordering::SeqCst);
        let cancel=Arc::clone(&self.browser_ui.cancel);
        self.message=format!("Downloadwijze controleren met job {id}…");
        std::thread::spawn(move || {
            let capture=match source{
                RepairSource::File(path)=>read_download_capture(&path),
                RepairSource::Browser(browser)=>browser.arm_download_capture(id,&cancel).and_then(|handle|{
                    let _=status_tx.send(format!("Klaar voor job {id}: download nu de GeoTIFF in het ingebouwde venster. De vastlegging stopt na twee minuten."));
                    handle.wait(&cancel)
                }),
            };
            let result=capture.and_then(|capture| {
                session.repair_download_from_capture(&job,&capture,&kickthemap_cache_dir().join("Rust"),&cancel)
            }).map(|path|(RemoteFileKind::GeoTiff,path));
            if result.is_ok(){
                if let Err(error)=session.save_download_strategy(&kickthemap_cache_dir()){
                    let _=notice_tx.send(format!("Downloadwijze werkt in deze sessie, maar kon niet voor later worden opgeslagen: {error}"));
                }
            }
            let _=tx.send((session,result));
        });
    }
    pub(super) fn live_operation_busy(&self)->bool{
        self.live_busy||self.live_rx.is_some()||self.live_download_rx.is_some()||self.browser_ui.capturing.is_some()||self.browser_ui.launching.is_some()||self.browser_ui.opening_job.is_some()
    }
    pub(super) fn open_kickthemap_workspace(&mut self) {
        self.browser_ui.open=true;
        if self.jobs_cache.is_some() {return;}
        let dir=kickthemap_cache_dir().join("jobs_cache");
        let mut newest=None::<(f64,JobsCache)>;
        if let Ok(entries)=std::fs::read_dir(dir) {
            for item in entries.flatten().take(250) {
                if item.path().extension().is_none_or(|ext|ext!="json") {continue;}
                if let Ok(cache)=read_cache(&item.path()) {
                    if newest.as_ref().is_none_or(|(ts,_)|cache.saved_at>*ts) {
                        newest=Some((cache.saved_at,cache));
                    }
                }
            }
        }
        if let Some((_,cache))=newest {
            self.browser_ui.status=format!(
                "Offline cache geopend: {} jobs. Log via het ingebouwde venster in voor de huidige gegevens.",
                cache.jobs.len()
            );
            self.jobs_cache=Some(cache);
        }
    }
    pub(super) fn launch_kickthemap_browser(&mut self) {
        if self.browser_ui.launching.is_some() || self.browser_ui.capturing.is_some() ||
            self.browser_ui.browser.is_some() {return;}
        let (tx,rx)=mpsc::channel();
        self.browser_ui.launching=Some(rx);
        self.browser_ui.status="Ingebouwd KickTheMap-inlogvenster openen…".into();
        self.message=self.browser_ui.status.clone();
        #[cfg(not(test))]
        std::thread::spawn(move||{let _=tx.send(Browser::launch());});
        #[cfg(test)] {self.browser_ui.launch_requests+=1;let _=tx.send(Err("Test: ingebouwd inlogvenster niet gestart".into()));}
    }
    pub(super) fn capture_kickthemap_browser(&mut self) {
        if self.live_operation_busy()||self.browser_ui.batch_mode{return;}
        let Some(browser)=self.browser_ui.browser.take() else {return};
        // The browser's account is not yet known. This flag only requests a
        // second empty response; no cached account/job data enters its session.
        let known_nonempty=!self.live_jobs.is_empty()||self.jobs_cache.as_ref().is_some_and(|c|!c.jobs.is_empty());
        let (tx,rx)=mpsc::channel();
        self.browser_ui.capturing=Some(rx);
        self.browser_ui.status="Sessie van het ingebouwde inlogvenster controleren…".into();
        self.message=self.browser_ui.status.clone();
        std::thread::spawn(move|| {
            // Validate through the authenticated tab first. An independent
            // HTTP bootstrap may be rejected even when the embedded browser is signed in.
            let result=browser.csrf_token().and_then(|csrf|browser.cookies()
                .and_then(|cookies|KickTheMapLiveClient::from_authenticated_browser(&cookies,&csrf)))
                .and_then(|mut session|{
                    if known_nonempty{session.require_empty_confirmation();}
                    session.fetch_jobs_from_browser(||browser.browser_job_payload())
                        .map(|jobs|(session,jobs))
                });
            if result.is_ok(){let _=browser.set_visible(false);}
            let _=tx.send((browser,result));
        });
    }
    pub(super) fn poll_kickthemap_browser(&mut self,ctx:&egui::Context) {
        let opening=self.browser_ui.opening_job.as_ref().map(|rx|rx.try_recv());
        match opening {
            Some(Ok((browser,result)))=>{
                self.browser_ui.browser=Some(browser);
                self.browser_ui.opening_job=None;
                self.browser_ui.status=match result {
                    Ok(())=>"Job geopend in het ingebouwde KickTheMap-venster".into(),
                    Err(e)=>format!("Job openen mislukt: {e}"),
                };
                self.message=self.browser_ui.status.clone();
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.browser_ui.opening_job=None;
                self.browser_ui.status="Jobvenster openen onderbroken".into();
            }
            _=>{}
        }
        match self.browser_ui.map_loading.as_ref().map(|rx|rx.try_recv()) {
            Some(Ok(Ok(raster)))=>{
                let bytes=raster.rgba;
                let image=egui::ColorImage::from_rgba_unmultiplied(
                    [raster.width as usize,raster.height as usize],&bytes);
                let texture=ctx.load_texture("kickthemap-jobs-osm",image,egui::TextureOptions::LINEAR);
                self.browser_ui.map_texture=Some((raster.bounds,texture));
                self.browser_ui.map_loading=None;
                self.browser_ui.map_status=if raster.stale_cache{
                    "OpenStreetMap geladen vanuit verouderde tegelcache".into()
                }else{"OpenStreetMap geladen".into()};
            }
            Some(Ok(Err(e)))=>{
                self.browser_ui.map_loading=None;
                self.browser_ui.map_status=format!("Kaartachtergrond niet beschikbaar: {e}");
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.browser_ui.map_loading=None;
                self.browser_ui.map_status="Kaartdownload onderbroken".into();
            }
            _=>{}
        }
        match self.browser_ui.capture_status.as_ref().map(|rx|rx.try_recv()){
            Some(Ok(status))=>{
                if self.live_download_rx.is_some()&&self.live_busy&&self.live_download_generation==self.project_generation{self.browser_ui.status=status.clone();self.message=status;}
                self.browser_ui.capture_status=None;
            }
            Some(Err(TryRecvError::Disconnected))=>self.browser_ui.capture_status=None,
            _=>{},
        }
        if (self.browser_ui.batch_mode ||
             !self.browser_ui.downloaded_json.is_empty() ||
             !self.browser_ui.downloaded_tiffs.is_empty()) &&
            self.browser_ui.batch_generation!=self.project_generation {
            self.browser_ui.cancel.store(true,Ordering::SeqCst);
            self.browser_ui.queue.clear();
            self.browser_ui.downloaded_json.clear();
            self.browser_ui.downloaded_tiffs.clear();
            self.browser_ui.batch_mode=false;
            self.browser_ui.status="Project gewijzigd tijdens downloads; bestanden blijven op schijf, maar zijn niet in het nieuwe project ingeladen".into();
        }
        let event=self.browser_ui.launching.as_ref().map(|rx|rx.try_recv());
        let received=event.as_ref().is_some_and(|r|!matches!(r,Err(TryRecvError::Empty)));
        match event {
            Some(Ok(Ok(browser)))=>{
                self.browser_ui.browser=Some(browser);
                self.browser_ui.launching=None;
                self.browser_ui.status="Het ingebouwde venster staat open. Meld je aan en klik daarna op Sessie overnemen.".into();
            }
            Some(Ok(Err(err)))=>{
                self.browser_ui.launching=None;
                self.browser_ui.status=format!("Ingebouwd inlogvenster starten mislukt: {err}");
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.browser_ui.launching=None;
                self.browser_ui.status="Het ingebouwde inlogvenster is onverwacht gesloten".into();
            }
            _=>{}
        }
        if received{self.message=self.browser_ui.status.clone();}
        let event=self.browser_ui.capturing.as_ref().map(|rx|rx.try_recv());
        match event {
            Some(Ok((browser,result)))=>{
                self.browser_ui.browser=Some(browser);
                self.browser_ui.capturing=None;
                match result {
                    Ok((session,jobs))=>{
                        self.live_jobs_owner=Some(session.snapshot_owner().clone());
                        self.live_session=Some(session);
                        self.live_jobs=jobs;
                        self.browser_ui.open=true;self.shell.login_open=false;
                        self.save_live_cache();
                        self.browser_ui.status=format!(
                            "KickTheMap verbonden: {} live jobs. Selecteer jobs voor downloads.",
                            self.live_jobs.len()
                        );
                    }
                    Err(err)=>self.browser_ui.status=format!(
                        "Browseraanmelding nog niet bruikbaar: {err}. Je kunt opnieuw proberen."
                    ),
                }
                self.message=self.browser_ui.status.clone();
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.browser_ui.capturing=None;
                self.browser_ui.status="Browserovername afgebroken; start desgewenst een nieuw venster".into();
            }
            _=>{}
        }
        // Prepare one kind at a time for FileLoader; otherwise starting TIFF
        // import would cancel an unfinished jobFeatures import.
        if self.file_intent.is_none() && !self.live_busy &&
            !self.browser_ui.batch_mode && self.browser_ui.queue.is_empty() {
            if !self.browser_ui.downloaded_json.is_empty() {
                let paths=std::mem::take(&mut self.browser_ui.downloaded_json);
                self.load_files(paths);
            } else if !self.browser_ui.downloaded_tiffs.is_empty() {
                let paths=std::mem::take(&mut self.browser_ui.downloaded_tiffs);
                self.load_geotiffs(paths,ctx);
            }
        }
        if self.browser_ui.batch_mode && !self.live_busy && !self.browser_ui.queue.is_empty() {
            if self.browser_ui.cancel.load(Ordering::Relaxed) {
                self.browser_ui.queue.clear();
            } else if let Some((job_id,kind))=self.browser_ui.queue.pop_front() {
                self.download_live_job(job_id,kind);
            }
        }
        if self.browser_ui.batch_mode && !self.live_busy && self.browser_ui.queue.is_empty() {
            self.browser_ui.batch_mode=false;
            self.browser_ui.status=format!(
                "Downloadopdracht afgerond: {} bestanden, {} fouten. Bestanden worden nu ingeladen.",
                self.browser_ui.completed,self.browser_ui.failures.len()
            );
        }
    }
    pub(super) fn save_live_cache(&mut self) {
        // Successful refreshes have already confirmed unexpected empty lists.
        // Persist that result too, so old jobs cannot reappear after restarting.
        let Some(session)=self.live_session.as_ref() else{return;};
        let owner=session.snapshot_owner().clone();
        if self.live_jobs_owner.as_ref()!=Some(&owner){self.message="Jobcache niet opgeslagen: de sessie en joblijst horen niet bij elkaar. Vernieuw de jobs.".into();return;}
        let root=kickthemap_cache_dir();
        if let Ok(path)=owner.write_snapshot(&root,&self.live_jobs) {
            if let Ok(cache)=read_cache(&path) {self.jobs_cache=Some(cache);}
        }
    }
    pub(super) fn queue_kickthemap_jobs(&mut self,selected:&[u64],
        download_tiff:bool,download_json:bool)
    {
        if self.live_session.is_none() || self.live_operation_busy() || self.browser_ui.batch_mode {
            self.browser_ui.status="Log eerst in en voltooi de lopende download".into();
            return;
        }
        if !download_tiff && !download_json {return;}
        let mut queued=VecDeque::new();
        for &id in selected.iter().take(500) {
            if self.live_jobs.iter().any(|job|job.job_id==id) {
                if download_json {queued.push_back((id,RemoteFileKind::JobFeatures));}
                if download_tiff {queued.push_back((id,RemoteFileKind::GeoTiff));}
            }
        }
        if queued.is_empty() {return;}
        self.browser_ui.queue=queued;
        self.browser_ui.completed=0;
        self.browser_ui.failures.clear();
        self.browser_ui.cancel.store(false,Ordering::SeqCst);
        self.browser_ui.batch_mode=true;
        self.browser_ui.batch_generation=self.project_generation;
        self.browser_ui.status=format!(
            "{} bestanden ingepland. Elke download wordt na validatie opgeslagen zonder overschrijven.",
            self.browser_ui.queue.len()
        );
    }
    fn open_selected_kickthemap_job(&mut self,id:u64){
        if self.live_operation_busy()||self.browser_ui.batch_mode{return;}
        let Some(browser)=self.browser_ui.browser.take() else {
            self.browser_ui.status="Open eerst het ingebouwde KickTheMap-venster en neem de sessie over.".into();
            return;
        };
        if self.browser_ui.opening_job.is_some(){self.browser_ui.browser=Some(browser);return;}
        let (tx,rx)=mpsc::channel();
        self.browser_ui.opening_job=Some(rx);
        self.browser_ui.status=format!("Job {id} openen in het ingebouwde venster…");
        std::thread::spawn(move||{
            let result=browser.open_job(id);
            let _=tx.send((browser,result));
        });
    }
    pub(super) fn show_kickthemap_workspace(&mut self,ctx:&egui::Context) {
        if !self.browser_ui.open {return;}
        let mut open=true;
        let mut action=None;
        let loaded=self.active_job_ids();
        let jobs=&self.live_jobs;
        let cache=self.jobs_cache.as_ref().map(|c|c.jobs.as_slice()).unwrap_or(&[]);
        let live=self.live_session.is_some()||(self.live_busy&&self.live_jobs_owner.is_some());
        let busy=self.live_operation_busy();
        let source=if live {jobs.as_slice()} else {cache};
        let browser=&mut self.browser_ui;
        let mut indices=(0..source.len()).filter(|&i|{
            let j=&source[i];
            (!browser.only_downloadable||j.downloadable)
                &&(!browser.only_loaded||loaded.contains(&j.job_id))
                &&(!browser.only_selected||browser.selected.contains(&j.job_id))
                &&job_matches_filters(j,&browser.filters)
        }).collect::<Vec<_>>();
        let column=browser.sort_column;
        // Compute text keys once per job, not on every sort comparison.
        let sort_keys=if matches!(column,4|6){Vec::new()}else{source.iter().map(|job|job_column_value(job,column).to_lowercase()).collect::<Vec<_>>()};
        indices.sort_by(|&a,&b|{
            let order=match column {
                4=>source[a].status.cmp(&source[b].status),
                6=>source[a].job_id.cmp(&source[b].job_id),
                _=>sort_keys[a].cmp(&sort_keys[b]),
            };
            (if browser.sort_descending {order.reverse()}else{order})
                .then(source[a].job_id.cmp(&source[b].job_id))
        });
        let filtered_ids=indices.iter().map(|&i|source[i].job_id).collect::<Vec<_>>();
        let selected_single=if browser.selected.len()==1{browser.selected.iter().next().copied()}else{None};
        egui::Window::new("SleufBase Jobs")
            .open(&mut open).default_width(1180.).default_height((ctx.screen_rect().height()-50.).clamp(260.,760.))
            .constrain_to(ctx.screen_rect().shrink(12.))
            .min_width(340.).min_height(240.)
            .resizable(true).show(ctx,|ui|{
                settings_ui::control_style(ui);
                ui.set_max_width(ui.available_width());ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                egui::TopBottomPanel::bottom("kickthemap-download-actions").frame(egui::Frame::NONE).show_inside(ui,|ui|{
                    ui.small(format!("{} van {} jobs zichtbaar; {} geselecteerd{}",
                        indices.len(),source.len(),browser.selected.len(),
                        if live{""}else{" (offline cache)"}));
                    ui.separator();
                    ui.horizontal_wrapped(|ui|{
                        let can=live&&!busy&&!browser.batch_mode&&
                            !browser.selected.is_empty()&&browser.capturing.is_none();
                        if ui.add_enabled(can,egui::Button::new(egui::RichText::new("Laad geselecteerde jobs").color(Color32::WHITE)).fill(ORANGE)).clicked(){action=Some("both");}
                        if ui.add_enabled(can,egui::Button::new("Objecten downloaden + laden")).clicked(){
                            action=Some("objects");
                        }
                        if ui.add_enabled(can,egui::Button::new("GeoTIFF downloaden + laden")).clicked(){
                            action=Some("tiffs");
                        }
                    });
                    ui.menu_button("Meer opties en probleemoplossing",|ui|{
                        if ui.add_enabled(live&&!busy&&browser.browser.is_some()&&browser.opening_job.is_none()&&browser.selected.len()==1,
                            egui::Button::new("Jobdetails op website")).clicked(){action=Some("open-job");}
                    if ui.add_enabled(live&&!busy&&!browser.batch_mode&&browser.selected.len()==1,
                        egui::Button::new("Download herstellen met opgeslagen verzoek…")).clicked(){action=Some("repair");}
                    if ui.add_enabled(live&&!busy&&!browser.batch_mode&&browser.selected.len()==1&&browser.browser.is_some(),
                        egui::Button::new("Handmatige GeoTIFF-download vastleggen in het ingebouwde venster")).clicked(){action=Some("repair-browser");}
                    ui.small("Selecteer één job om een opgeslagen downloadverzoek te controleren. Een werkende downloadwijze wordt voor deze sessie onthouden.");
                    });
                    if busy||browser.batch_mode{ui.horizontal_wrapped(|ui|{
                        ui.spinner();if browser.batch_mode{ui.label(format!("{} klaar, {} in wachtrij",browser.completed,browser.queue.len()));}
                        if ui.button("Download annuleren").clicked(){action=Some("cancel");}
                    });}
                    if !browser.failures.is_empty(){
                        ui.menu_button(format!("Downloadfouten: {}",browser.failures.len()),|ui|{
                            egui::ScrollArea::vertical().max_height(180.).show(ui,|ui|{for message in browser.failures.iter().rev().take(20){ui.label(message);}});
                        });
                    }
                    if ctx.screen_rect().height()>=500.{ui.small("Dubbelklik op een job om de objecten en GeoTIFF in SleufBase te laden.");}
                    if !browser.status.is_empty(){egui::ScrollArea::vertical().id_salt("jobs-status").max_height(64.).show(ui,|ui|{ui.label(&browser.status);});}
                });
                let body_height=ui.available_height().max(40.);
                egui::ScrollArea::vertical().id_salt("jobs-body").max_height(body_height).auto_shrink([false,false]).show(ui,|ui|{
                    let compact=ctx.screen_rect().height()<550.;
                    ui.horizontal_wrapped(|ui|{
                        ui.strong(format!("Jobs: {} / {} {}",indices.len(),source.len(),if live{"live"}else{"offline"}));
                        if ui.add_enabled(!busy,egui::Button::new(if live{"Account / aanmelding"}else{"Aanmelden"})).clicked(){action=Some("login-dialog");}
                        if ui.add_enabled(live&&!busy,egui::Button::new("Vernieuwen")).clicked(){action=Some("refresh");}
                        ui.menu_button("Filter / selectie",|ui|{
                            ui.set_max_width((ctx.screen_rect().width()-50.).min(320.));
                            egui::ScrollArea::vertical().max_height((ctx.screen_rect().height()-100.).max(100.)).show(ui,|ui|{
                            if ui.button(if browser.map_open{"Jobs als lijst"}else{"Jobs op kaart"}).clicked(){browser.map_open=!browser.map_open;if browser.map_open{browser.map_bounds=None;}ui.close_menu();}
                            ui.checkbox(&mut browser.only_loaded,"Geladen jobs");ui.checkbox(&mut browser.only_selected,"Alleen geselecteerd");ui.checkbox(&mut browser.only_downloadable,"Downloadbaar");
                            ui.horizontal_wrapped(|ui|{
                                if ui.button("Selecteer zichtbare").clicked(){browser.selected.extend(filtered_ids.iter().copied());}
                                if ui.button("Deselecteer zichtbare").clicked(){for id in &filtered_ids{browser.selected.remove(id);}}
                                if ui.button("Wis selectie").clicked(){browser.selected.clear();}
                            });
                            ui.separator();
                            for (column,(name,_)) in JOB_COLUMNS.iter().enumerate(){ui.label(*name);ui.add(egui::TextEdit::singleline(&mut browser.filters[column]).desired_width(230.).hint_text("Filter..."));}
                            });
                        });
                    });
                    if browser.map_open {
                        show_jobs_map(ui,source,&indices,browser,&mut action);
                    } else {
                    // The header, seven independent filters and data share one horizontal scroll offset.
                    egui::ScrollArea::horizontal().id_salt("jobs-horizontal").show(ui,|ui|{
                        ui.horizontal(|ui|{
                            ui.add_sized([26.,25.],egui::Label::new(""));
                            for (column,(name,width)) in JOB_COLUMNS.iter().enumerate(){
                                let arrow=if browser.sort_column==column{
                                    if browser.sort_descending{" ▼"}else{" ▲"}
                                }else{""};
                                if ui.add_sized([*width,27.],egui::Button::new(format!("{name}{arrow}"))).clicked(){
                                    if browser.sort_column==column {
                                        browser.sort_descending=!browser.sort_descending;
                                    }else{
                                        browser.sort_column=column;
                                        browser.sort_descending=column==3;
                                    }
                                }
                            }
                        });
                        if !compact{
                        ui.horizontal(|ui|{
                            ui.add_sized([26.,25.],egui::Label::new(""));
                            for (column,(_,width)) in JOB_COLUMNS.iter().enumerate(){
                                ui.add_sized([*width,25.],egui::TextEdit::singleline(&mut browser.filters[column]).hint_text("Filter..."));
                            }
                        });                        }

                        egui::ScrollArea::vertical().id_salt("jobs-vertical").max_height((body_height-if compact{85.}else{120.}).clamp(40.,410.))
                            .show_rows(ui,28.,indices.len(),|ui,rows|{
                                for position in rows{
                                    let job=&source[indices[position]];
                                    ui.horizontal(|ui|{
                                        let mut selected=browser.selected.contains(&job.job_id);
                                        if ui.add_sized([26.,24.],egui::Checkbox::without_text(&mut selected)).changed(){
                                            if selected{browser.selected.insert(job.job_id);}
                                            else{browser.selected.remove(&job.job_id);}
                                        }
                                        for (column,(_,width)) in JOB_COLUMNS.iter().enumerate(){
                                            let value=job_column_value(job,column);
                                            if column==0{
                                                let response=ui.add_sized([*width,24.],egui::Button::new(&value).selected(selected).truncate()).on_hover_text(format!("{value}\nDubbelklik om objecten en GeoTIFF in SleufBase te laden"));
                                                if response.clicked(){let multi=ui.input(|i|i.modifiers.ctrl||i.modifiers.command);if multi&&selected{browser.selected.remove(&job.job_id);}else{if !multi{browser.selected.clear();}browser.selected.insert(job.job_id);}}
                                                if response.double_clicked()&&live&&!busy&&!browser.batch_mode{browser.selected.clear();browser.selected.insert(job.job_id);action=Some("both");}
                                            }else{ui.add_sized([*width,24.],egui::Label::new(value.clone()).truncate()).on_hover_text(value);}
                                        }
                                    });
                                }
                            });
                    });
                    }
                });
            });
        self.browser_ui.open=open;
        match action {
            Some("login-dialog")=>self.open_kickthemap_login(),
            Some("launch")=>self.launch_kickthemap_browser(),
            Some("open-job")=>{
                if let Some(id)=self.browser_ui.open_target.take().or(selected_single){
                    self.open_selected_kickthemap_job(id);
                }
            },
            Some("capture")=>self.capture_kickthemap_browser(),
            Some("close")=>{
                self.browser_ui.browser=None;
                self.browser_ui.status="Ingebouwd inlogvenster gesloten".into();
            }
            Some("refresh")=>self.refresh_live_jobs(),
            Some("repair")=>{
                if let Some(id)=self.browser_ui.selected.first().copied(){
                    if let Some(path)=rfd::FileDialog::new().add_filter("Opgeslagen verzoek",&["json","txt"]).pick_file(){
                        self.repair_kickthemap_download(id,path);
                    }
                }
            }
            Some("repair-browser")=>{
                if let Some(id)=self.browser_ui.selected.first().copied(){self.capture_kickthemap_download(id);}
            }
            Some("cancel")=>{
                self.browser_ui.cancel.store(true,Ordering::SeqCst);
                self.browser_ui.queue.clear();
                self.browser_ui.status="Downloadbatch geannuleerd".into();
            }
            Some("objects")|Some("tiffs")|Some("both")=>{
                let selection=self.browser_ui.selected.iter().copied().collect::<Vec<_>>();
                self.queue_kickthemap_jobs(&selection,action!=Some("objects"),action!=Some("tiffs"));
            }
            _=>{}
        }
    }
}

fn job_rd_coordinates(text:&str)->Option<[f64;2]>{
    // Match the two leading numbers in the original Jobs map. Both RD and
    // longitude/latitude are accepted; malformed values are not plotted.
    let mut words=Vec::new();
    let mut number=String::new();
    for c in text.chars().chain(std::iter::once(' ')){
        if c.is_ascii_digit()||((c=='-'||c=='.'||c==',')&&number.len()<30) {
            if c==',' {number.push('.');} else{number.push(c);}
        }else if !number.is_empty(){
            if let Ok(n)=number.parse::<f64>(){words.push(n);}
            number.clear();
            if words.len()>=2{break;}
        }
    }
    if words.len()<2{return None;}
    let (x,y)=(words[0],words[1]);
    if x.abs()<=20.&&y.abs()<=90.{
        sleufbase_rust::location_search::wgs84_to_rd(y,x).ok()
    }else if x.abs()>1000.&&y.abs()>1000.&&x.is_finite()&&y.is_finite(){
        Some([x,y])
    }else{None}
}
fn jobs_bounds(points:&[(u64,[f64;2])])->Option<Bounds>{
    if points.is_empty(){return None;}
    let min_x=points.iter().map(|(_,p)|p[0]).fold(f64::INFINITY,f64::min);
    let max_x=points.iter().map(|(_,p)|p[0]).fold(f64::NEG_INFINITY,f64::max);
    let min_y=points.iter().map(|(_,p)|p[1]).fold(f64::INFINITY,f64::min);
    let max_y=points.iter().map(|(_,p)|p[1]).fold(f64::NEG_INFINITY,f64::max);
    let padding=500_f64.max((max_x-min_x)*0.18).max((max_y-min_y)*0.18);
    Some(Bounds{min_x:min_x-padding,max_x:max_x+padding,
        min_y:min_y-padding,max_y:max_y+padding})
}
fn map_point(rect:egui::Rect,bounds:Bounds,p:[f64;2])->Pos2{
    Pos2::new(
        rect.left()+((p[0]-bounds.min_x)/(bounds.max_x-bounds.min_x)) as f32*rect.width(),
        rect.bottom()-((p[1]-bounds.min_y)/(bounds.max_y-bounds.min_y)) as f32*rect.height(),
    )
}
fn show_jobs_map(ui:&mut egui::Ui,source:&[Job],indices:&[usize],
    browser:&mut BrowserUi,action:&mut Option<&'static str>){
    let points=indices.iter().filter_map(|&i|{
        job_rd_coordinates(&source[i].coordinates).map(|p|(source[i].job_id,p))
    }).collect::<Vec<_>>();
    let Some(visible_bounds)=jobs_bounds(&points) else {
        ui.label("Geen jobs met geldige coördinaten gevonden.");
        return;
    };
    ui.horizontal(|ui|{
        if ui.button("Passend").clicked(){browser.map_bounds=Some(visible_bounds);browser.map_texture=None;}
        if ui.button("Zoom naar selectie").clicked(){
            let selected=points.iter().filter(|(id,_)|browser.selected.contains(id))
                .copied().collect::<Vec<_>>();
            if let Some(bounds)=jobs_bounds(&selected){browser.map_bounds=Some(bounds);browser.map_texture=None;}
        }
        if ui.button("Vernieuwen").clicked(){browser.map_texture=None;}
        ui.small(format!("{} jobs met coördinaten",points.len()));
    });
    let bounds=*browser.map_bounds.get_or_insert(visible_bounds);
    let (rect,response)=ui.allocate_exact_size(egui::vec2(ui.available_width().max(420.),350.),egui::Sense::click_and_drag());
    let painter=ui.painter_at(rect);
    painter.rect_filled(rect,0.,egui::Color32::from_rgb(238,242,247));
    if browser.map_texture.as_ref().is_some_and(|(b,_)|
        (b.min_x-bounds.min_x).abs()<0.01&&(b.max_x-bounds.max_x).abs()<0.01&&
        (b.min_y-bounds.min_y).abs()<0.01&&(b.max_y-bounds.max_y).abs()<0.01){
        if let Some((_,texture))=&browser.map_texture{
            painter.image(texture.id(),rect,egui::Rect::from_min_max(
                egui::pos2(0.,0.),egui::pos2(1.,1.)),egui::Color32::WHITE);
        }
    }else if browser.map_loading.is_none(){
        let (tx,rx)=mpsc::channel();
        browser.map_loading=Some(rx);
        browser.map_status="OpenStreetMap-tegels ophalen…".into();
        let dir=kickthemap_cache_dir().join("osm_jobs");
        std::thread::spawn(move||{
            let result=osm_tiles::request_map(bounds,900,480,dir);
            let _=tx.send(result);
        });
    }
    if response.dragged_by(egui::PointerButton::Middle){
        let delta=response.drag_delta();
        let dx=delta.x as f64/(rect.width() as f64)*(bounds.max_x-bounds.min_x);
        let dy=delta.y as f64/(rect.height() as f64)*(bounds.max_y-bounds.min_y);
        browser.map_bounds=Some(Bounds{min_x:bounds.min_x-dx,max_x:bounds.max_x-dx,
            min_y:bounds.min_y+dy,max_y:bounds.max_y+dy});
    }
    let scroll=ui.input(|i|if response.hovered(){i.raw_scroll_delta.y}else{0.});
    if scroll.abs()>0.1{
        let factor=if scroll>0.{0.85}else{1.15};
        let cx=(bounds.min_x+bounds.max_x)*0.5;
        let cy=(bounds.min_y+bounds.max_y)*0.5;
        let hx=(bounds.max_x-bounds.min_x)*0.5*factor;
        let hy=(bounds.max_y-bounds.min_y)*0.5*factor;
        browser.map_bounds=Some(Bounds{min_x:cx-hx,max_x:cx+hx,min_y:cy-hy,max_y:cy+hy});
        browser.map_texture=None;
    }
    let mut nearest:Option<(u64,f32)>=None;
    for (id,p) in points{
        let pos=map_point(rect,bounds,p);
        if !rect.contains(pos){continue;}
        let is_selected=browser.selected.contains(&id);
        painter.circle_filled(pos,if is_selected{7.}else{5.},
            if is_selected{egui::Color32::from_rgb(249,115,22)}
            else{egui::Color32::from_rgb(35,91,167)});
        painter.circle_stroke(pos,if is_selected{7.}else{5.},
            egui::Stroke::new(1.0_f32,egui::Color32::WHITE));
        if let Some(mouse)=response.interact_pointer_pos(){
            let distance=pos.distance(mouse);
            if distance<16. && nearest.is_none_or(|(_,d)|distance<d){
                nearest=Some((id,distance));
            }
        }
    }
    if (response.clicked()||response.double_clicked())&&nearest.is_some(){
        let id=nearest.unwrap().0;
        if !ui.input(|i|i.modifiers.ctrl){browser.selected.clear();}
        browser.selected.insert(id);
        if response.double_clicked(){browser.open_target=Some(id);*action=Some("open-job");}
    }
    ui.small(&browser.map_status);
    ui.small("Klik op een marker om te selecteren; Ctrl voor meervoudige selectie. Dubbelklik opent een job. Muiswiel zoomt en de middelste muisknop verplaatst de kaart.");
}
fn read_download_capture(path:&std::path::Path)->Result<String,String>{
    use std::io::Read;
    const LIMIT:u64=256*1024;
    let file=std::fs::File::open(path).map_err(|_|"Opgeslagen downloadverzoek kan niet worden geopend".to_string())?;
    let mut bytes=Vec::new();
    file.take(LIMIT+1).read_to_end(&mut bytes).map_err(|_|"Opgeslagen downloadverzoek kan niet worden gelezen".to_string())?;
    if bytes.len() as u64>LIMIT{return Err("Opgeslagen downloadverzoek is te groot (maximaal 256 KiB)".into());}
    String::from_utf8(bytes).map_err(|_|"Opgeslagen downloadverzoek moet UTF-8-tekst bevatten".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jobs_grid_filters_all_original_columns_independently(){
        let job=Job{job_id:12345,title:"Sleuf Noord".into(),address:"Markt 5".into(),
            municipality:"Breda".into(),coordinates:"100000, 400000".into(),
            client_date:"2026-10-26".into(),status:9,downloadable:true,prefix:String::new()};
        assert_eq!(JOB_COLUMNS.map(|(name,_)|name),
            ["Jobnaam","Adres","Coordinaten","Aanmaak datum","Status","Gemeente","Job ID"]);
        assert_eq!(job_status_text(9),"Gereed");
        let mut filters:[String;7]=Default::default();
        filters[0]="SLEUF".into();filters[2]="400000".into();
        filters[4]="gereed".into();filters[6]="2345".into();
        assert!(job_matches_filters(&job,&filters));
        filters[1]="ander adres".into();
        assert!(!job_matches_filters(&job,&filters));
        filters[1]="Markt".into();filters[5]="  BREDA ".into();
        assert!(job_matches_filters(&job,&filters));
    }
    #[test]
    fn project_switch_cancels_manual_download_and_ignores_its_late_capture_notice(){
        let mut app=SleufBaseApp::default();
        let (_result_tx,result_rx)=mpsc::channel();
        app.live_download_rx=Some(result_rx);app.live_busy=true;
        let (tx,rx)=mpsc::channel();app.browser_ui.capture_status=Some(rx);
        app.live_download_generation=app.project_generation;
        app.execute_project_action(ProjectAction::New,&egui::Context::default());
        assert!(app.browser_ui.cancel.load(Ordering::Acquire));
        let message=app.message.clone();
        tx.send("Oude vastlegging is nu klaar".into()).unwrap();
        app.poll_kickthemap_browser(&egui::Context::default());
        assert_eq!(app.message,message);assert!(app.browser_ui.capture_status.is_none());
        app.live_download_generation=app.project_generation;
        app.live_download_rx=None;app.live_busy=false;
        let (tx,rx)=mpsc::channel();app.browser_ui.capture_status=Some(rx);
        tx.send("Vastleggen na een al afgeronde download".into()).unwrap();
        app.poll_kickthemap_browser(&egui::Context::default());
        assert_eq!(app.message,message);assert!(app.browser_ui.capture_status.is_none());
    }
    #[test]
    fn saved_download_request_is_bounded_utf8_and_busy_repair_cannot_replace_an_operation(){
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("capture.json");
        std::fs::write(&path,b"{\"status\":\"captured\"}").unwrap();
        assert_eq!(read_download_capture(&path).unwrap(),"{\"status\":\"captured\"}");
        std::fs::write(&path,[255u8]).unwrap();assert!(read_download_capture(&path).unwrap_err().contains("UTF-8"));
        std::fs::write(&path,vec![b' ';256*1024+1]).unwrap();assert!(read_download_capture(&path).unwrap_err().contains("256 KiB"));
        let mut app=SleufBaseApp{live_busy:true,..Default::default()};
        app.message="Lopende taak".into();
        app.repair_kickthemap_download(42,path);
        assert!(app.live_download_rx.is_none());assert_eq!(app.message,"Lopende taak");
    }
    #[test]
    fn switching_projects_aborts_batch_and_never_imports_old_downloads() {
        let mut app=SleufBaseApp::default();
        app.browser_ui.batch_mode=true;
        app.browser_ui.batch_generation=17;
        app.project_generation=18;
        app.browser_ui.queue.push_back((42,RemoteFileKind::GeoTiff));
        app.browser_ui.downloaded_json.push(PathBuf::from("old_project.json"));
        app.browser_ui.downloaded_tiffs.push(PathBuf::from("old_project.tiff"));
        app.poll_kickthemap_browser(&egui::Context::default());
        assert!(!app.browser_ui.batch_mode);
        assert!(app.browser_ui.queue.is_empty());
        assert!(app.browser_ui.downloaded_json.is_empty());
        assert!(app.browser_ui.downloaded_tiffs.is_empty());
        assert!(app.browser_ui.cancel.load(Ordering::Relaxed));
        assert!(app.file_intent.is_none());
    }
}

#[cfg(test)]
mod workflow_ui_tests{
    use super::*;
    fn app()->SleufBaseApp{
        let cookie=sleufbase_rust::kickthemap_browser::BrowserCookie{name:"fixture".into(),value:"local-only".into(),domain:"my.kickthemap.com".into(),path:"/".into(),secure:true};
        let session=KickTheMapLiveClient::from_authenticated_browser(&[cookie],"fixture-csrf").unwrap();let owner=session.snapshot_owner().clone();
        let mut app=SleufBaseApp{live_session:Some(session),live_jobs_owner:Some(owner),..Default::default()};app.browser_ui.open=true;
        app.live_jobs=vec![Job{job_id:42,title:"PS42".into(),address:"Teststraat 1".into(),municipality:"Testplaats".into(),coordinates:String::new(),client_date:"2026-10-28".into(),status:9,downloadable:true,prefix:"fixture@example.test_2026-10-28_12-00-00".into()}];app
    }
    #[test]fn double_click_loads_job_files_in_native_app_without_opening_a_website(){
        for size in [egui::vec2(680.,420.),egui::vec2(1200.,760.)]{
        let mut app=app();let ctx=egui::Context::default();theme::install(&ctx);
        let frame=|app:&mut SleufBaseApp,time,events|ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,size)),time:Some(time),events,..Default::default()},|ctx|app.show_kickthemap_workspace(ctx));
        for i in 0..4{frame(&mut app,i as f64*0.1,vec![]);}
        let out=frame(&mut app,0.5,vec![]);let pos=out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()=="PS42"&&s.clip_rect.contains_rect(t.galley.rect.translate(t.pos.to_vec2()))=>Some(t.pos+t.galley.rect.size()/2.),_=>None}).unwrap();
        for(time,pressed)in[(1.,true),(1.02,false),(1.1,true),(1.12,false)]{frame(&mut app,time,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        assert!(app.browser_ui.batch_mode);assert_eq!(app.browser_ui.queue.len(),2);assert!(app.browser_ui.opening_job.is_none());assert!(app.browser_ui.browser.is_none());assert_eq!(app.browser_ui.launch_requests,0);
        }
    }
    #[test]fn in_flight_download_keeps_live_jobs_visible_instead_of_switching_to_offline_cache(){
        let mut app=app();app.live_session=None;app.live_busy=true;let ctx=egui::Context::default();
        let mut settled=None;for _ in 0..4{settled=Some(ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1200.,760.))),..Default::default()},|ctx|app.show_kickthemap_workspace(ctx)));}let out=settled.unwrap();
        assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="PS42")));
        assert!(!out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().contains("offline cache"))));
        app.browser_ui.status="Langere downloadmelding met uitleg. ".repeat(30);
        let button=|output:&egui::FullOutput|output.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()=="Laad geselecteerde jobs"=>Some(t.pos),_=>None}).unwrap();
        let mut baseline=None;
        for i in 0..16{let output=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1200.,760.))),events:vec![egui::Event::PointerMoved(egui::pos2(50.+i as f32*3.,50.+i as f32*2.))],..Default::default()},|ctx|app.show_kickthemap_workspace(ctx));let pos=button(&output);if i==4{baseline=Some(pos);}if i>4{assert!((pos-baseline.unwrap()).length()<1.,"hover must not grow the jobs window");}}

    }
}
