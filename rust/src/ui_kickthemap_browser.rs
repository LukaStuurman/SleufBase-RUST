//! KickTheMap browser-account and jobs workspace for native desktop.
//! Browser auth, filtering, persistent offline snapshots and queued downloads
//! are coordinated without blocking egui's main thread.
use super::*;
use sleufbase_rust::{
    kickthemap_browser::Browser,
    jobs_cache::Job,
};
use std::{collections::{BTreeSet,VecDeque},sync::{Arc,atomic::{AtomicBool,Ordering}}};

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
    pub(super) capturing:Option<Receiver<(Browser,Result<(KickTheMapLiveClient,Vec<Job>),String>)>>,
    pub(super) selected:BTreeSet<u64>,
    filters:[String;7],
    only_downloadable:bool,
    only_loaded:bool,
    only_selected:bool,
    sort_column:usize,
    sort_descending:bool,
    pub(super) show_login:bool,
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
    status:String,
}
impl Default for BrowserUi {
    fn default()->Self {
        Self {open:false,browser:None,launching:None,capturing:None,
            selected:BTreeSet::new(),filters:Default::default(),
            only_downloadable:false,only_loaded:false,only_selected:false,
            sort_column:3,sort_descending:true,show_login:false,
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
                    let _=status_tx.send(format!("Klaar voor job {id}: download nu de GeoTIFF in het geopende Edge-venster. De vastlegging stopt na twee minuten."));
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
        self.live_busy||self.live_rx.is_some()||self.live_download_rx.is_some()||self.browser_ui.capturing.is_some()
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
                "Offline cache geopend: {} jobs. Log via Edge in voor de huidige gegevens.",
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
        self.browser_ui.status="Een privé Microsoft Edge-venster starten. Log daarin zelf in, inclusief reCAPTCHA.".into();
        std::thread::spawn(move||{
            let _=tx.send(Browser::launch());
        });
    }
    pub(super) fn capture_kickthemap_browser(&mut self) {
        if self.live_operation_busy()||self.browser_ui.batch_mode{return;}
        let Some(browser)=self.browser_ui.browser.take() else {return};
        // The browser's account is not yet known. This flag only requests a
        // second empty response; no cached account/job data enters its session.
        let known_nonempty=!self.live_jobs.is_empty()||self.jobs_cache.as_ref().is_some_and(|c|!c.jobs.is_empty());
        let (tx,rx)=mpsc::channel();
        self.browser_ui.capturing=Some(rx);
        self.browser_ui.status="Sessie van het eigen Edge-venster controleren…".into();
        std::thread::spawn(move|| {
            let result=browser.cookies()
                .and_then(|cookies|KickTheMapLiveClient::from_browser_cookies(&cookies))
                .and_then(|mut session|{
                    if known_nonempty{session.require_empty_confirmation();}
                    match session.fetch_jobs() {
                        Ok(jobs)=>Ok((session,jobs)),
                        Err(original)=>{
                            // Site API endpoints and token behavior can differ
                            // for requests made outside the real browser. Try
                            // the same-origin fetch from this same owned tab.
                            session.fetch_jobs_from_browser(||browser.browser_job_payload()).map(|jobs|(session,jobs))
                                .map_err(|fallback|format!("{original}; browserfallback: {fallback}"))
                        }
                    }
                });
            let _=tx.send((browser,result));
        });
    }
    pub(super) fn poll_kickthemap_browser(&mut self,ctx:&egui::Context) {
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
        match event {
            Some(Ok(Ok(browser)))=>{
                self.browser_ui.browser=Some(browser);
                self.browser_ui.launching=None;
                self.browser_ui.status="Edge staat open. Meld je aan op KickTheMap en klik daarna op 'Sessie overnemen'.".into();
            }
            Some(Ok(Err(err)))=>{
                self.browser_ui.launching=None;
                self.browser_ui.status=format!("Edge starten mislukt: {err}");
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.browser_ui.launching=None;
                self.browser_ui.status="Edge-browserproces is onverwacht gestopt".into();
            }
            _=>{}
        }
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
                        self.live_password.clear();
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
    pub(super) fn show_kickthemap_workspace(&mut self,ctx:&egui::Context) {
        if !self.browser_ui.open {return;}
        let mut open=true;
        let mut action=None;
        let loaded=self.active_job_ids();
        let jobs=&self.live_jobs;
        let cache=self.jobs_cache.as_ref().map(|c|c.jobs.as_slice()).unwrap_or(&[]);
        let live=self.live_session.is_some();
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
        indices.sort_by(|&a,&b|{
            let order=match column {
                4=>source[a].status.cmp(&source[b].status),
                6=>source[a].job_id.cmp(&source[b].job_id),
                _=>job_column_value(&source[a],column).to_lowercase()
                    .cmp(&job_column_value(&source[b],column).to_lowercase()),
            };
            (if browser.sort_descending {order.reverse()}else{order})
                .then(source[a].job_id.cmp(&source[b].job_id))
        });
        let filtered_ids=indices.iter().map(|&i|source[i].job_id).collect::<Vec<_>>();
        egui::Window::new("SleufBase Jobs")
            .open(&mut open).default_width(1280.).default_height(760.)
            .min_width(980.).min_height(560.)
            .resizable(true).show(ctx,|ui|{
                egui::ScrollArea::vertical().max_height(610.).show(ui,|ui|{
                    if ui.small_button(if browser.show_login{"Aanmelding verbergen"}else{"Inloggen / browseraanmelding"}).clicked(){
                        browser.show_login=!browser.show_login;
                    }
                    if browser.show_login || !live {
                    ui.heading("Browseraanmelding");
                    ui.label("SleufBase opent een eigen tijdelijk Edge-profiel. Log rechtstreeks in op KickTheMap en los reCAPTCHA zelf op; er wordt geen wachtwoord of bestaande browsersessie uitgelezen.");
                    ui.horizontal(|ui|{
                        if ui.add_enabled(browser.browser.is_none() &&
                            browser.launching.is_none()&&browser.capturing.is_none(),
                            egui::Button::new("Open KickTheMap in Edge")).clicked() {
                            action=Some("launch");
                        }
                        if ui.add_enabled(browser.browser.is_some()&&browser.capturing.is_none()&&!busy,
                            egui::Button::new("Sessie overnemen")).clicked(){
                            action=Some("capture");
                        }
                        if ui.add_enabled(browser.browser.is_some()&&browser.capturing.is_none()&&!busy,
                            egui::Button::new("Privé Edge-venster sluiten")).clicked(){action=Some("close");}
                        if ui.add_enabled(live&&!busy,
                            egui::Button::new("Vernieuw live jobs")).clicked(){action=Some("refresh");}
                    });
                    if browser.launching.is_some()||browser.capturing.is_some() {ui.spinner();}
                    }
                    if !browser.status.is_empty(){ui.small(&browser.status);}
                    ui.separator();
                    ui.heading(format!("Jobs: {} zichtbaar / {} {}",
                        indices.len(),source.len(),
                        if live {"live"} else {"offline cache — niet live"}));
                    ui.horizontal(|ui|{
                        ui.checkbox(&mut browser.only_loaded,"Geladen Jobs");
                        ui.checkbox(&mut browser.only_selected,"Alleen geselecteerd");
                        ui.checkbox(&mut browser.only_downloadable,"Downloadbaar");
                        if ui.small_button("Selecteer zichtbare").clicked(){
                            browser.selected.extend(filtered_ids.iter().copied());
                        }
                        if ui.small_button("Deselecteer zichtbare").clicked(){
                            for id in &filtered_ids{browser.selected.remove(id);}
                        }
                        if ui.small_button("Wis selectie").clicked(){browser.selected.clear();}
                    });
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
                        ui.horizontal(|ui|{
                            ui.add_sized([26.,25.],egui::Label::new(""));
                            for (column,(_,width)) in JOB_COLUMNS.iter().enumerate(){
                                ui.add_sized([*width,25.],egui::TextEdit::singleline(&mut browser.filters[column]).hint_text("Filter..."));
                            }
                        });
                        egui::ScrollArea::vertical().id_salt("jobs-vertical").max_height(310.)
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
                                            ui.add_sized([*width,24.],egui::Label::new(value.clone()).truncate())
                                                .on_hover_text(value);
                                        }
                                    });
                                }
                            });
                    });
                    ui.small(format!("{} van {} jobs zichtbaar; {} geselecteerd{}",
                        indices.len(),source.len(),browser.selected.len(),
                        if live{""}else{" (offline cache)"}));
                    ui.separator();
                    ui.heading(format!("Downloads — {} geselecteerd",browser.selected.len()));
                    ui.horizontal(|ui|{
                        let can=live&&!busy&&!browser.batch_mode&&
                            !browser.selected.is_empty()&&browser.capturing.is_none();
                        if ui.add_enabled(can,egui::Button::new("Objecten downloaden + laden")).clicked(){
                            action=Some("objects");
                        }
                        if ui.add_enabled(can,egui::Button::new("GeoTIFF downloaden + laden")).clicked(){
                            action=Some("tiffs");
                        }
                        if ui.add_enabled(can,egui::Button::new("Beide bestanden per job")).clicked(){
                            action=Some("both");
                        }
                    });
                    if ui.add_enabled(live&&!busy&&!browser.batch_mode&&browser.selected.len()==1,
                        egui::Button::new("Download herstellen met opgeslagen verzoek…")).clicked(){action=Some("repair");}
                    if ui.add_enabled(live&&!busy&&!browser.batch_mode&&browser.selected.len()==1&&browser.browser.is_some(),
                        egui::Button::new("Handmatige GeoTIFF-download vastleggen in Edge")).clicked(){action=Some("repair-browser");}
                    ui.small("Selecteer één job om een opgeslagen downloadverzoek te controleren. Een werkende downloadwijze wordt voor deze sessie onthouden.");
                    if busy&&!browser.batch_mode&&ui.button("Download annuleren").clicked(){action=Some("cancel");}
                    if browser.batch_mode||busy {ui.spinner();}
                    if browser.batch_mode {
                        ui.label(format!("{} verwerkt, {} in wachtrij",
                            browser.completed,browser.queue.len()));
                        if ui.button("Annuleer batch").clicked(){action=Some("cancel");}
                    }
                    if !browser.failures.is_empty(){
                        ui.collapsing(format!("Downloadfouten: {}",browser.failures.len()),|ui|{
                            for message in browser.failures.iter().rev().take(20){ui.label(message);}
                        });
                    }
                    ui.small("Bestaande downloads worden nooit overschreven. JobFeatures en GeoTIFF krijgen een identificerende sidecar; de bestanden worden pas na validatie in het actieve Rust-project geladen.");
                });
            });
        self.browser_ui.open=open;
        match action {
            Some("launch")=>self.launch_kickthemap_browser(),
            Some("capture")=>self.capture_kickthemap_browser(),
            Some("close")=>{
                self.browser_ui.browser=None;
                self.browser_ui.status="Privé Edge-venster gesloten".into();
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
