//! KickTheMap browser-account and jobs workspace for native desktop.
//! Browser auth, filtering, persistent offline snapshots and queued downloads
//! are coordinated without blocking egui's main thread.
use super::*;
use sleufbase_rust::{
    kickthemap_browser::{Browser,BrowserDock},
    jobs_cache::Job,
};
use std::{collections::{BTreeSet,VecDeque},sync::{Arc,atomic::{AtomicBool,Ordering}}};
use sleufbase_rust::osm_tiles::OsmRaster;

#[path="ui_kickthemap_jobs_map.rs"]
mod jobs_map;

fn workspace_cache_directory()->PathBuf {
    #[cfg(not(test))]
    { kickthemap_cache_dir() }
    #[cfg(test)]
    {
        // UI lifecycle tests must not read or write an installed user's jobs.
        static ROOT:std::sync::LazyLock<tempfile::TempDir>=std::sync::LazyLock::new(||tempfile::tempdir().expect("test job cache"));
        ROOT.path().to_path_buf()
    }
}

const JOB_COLUMNS:[(&str,f32);7]=[
    ("Jobnaam",205.),("Adres",290.),("Coordinaten",140.),
    ("Aanmaak datum",110.),("Status",108.),("Gemeente",118.),("Job ID",68.),
];
const JOB_ROW_HEIGHT:f32=34.;
const JOB_COLUMN_GAP:f32=6.;

fn job_columns_for_width(width:f32)->Vec<(usize,f32)>{
    let visible=|column|match column {
        0|1|4=>true,3=>width>=600.,5=>width>=760.,2=>width>=1000.,6=>width>=1120.,_=>false,
    };
    let mut columns=(0..JOB_COLUMNS.len()).filter(|&column|visible(column))
        .map(|column|(column,JOB_COLUMNS[column].1)).collect::<Vec<_>>();
    let available=(width-44.-JOB_COLUMN_GAP*(columns.len()-1) as f32).max(1.);
    let status=if width<480.{72.}else if width<600.{88.}else{108.};
    let fixed=columns.iter().filter(|(column,_)|*column!=0&&*column!=1)
        .map(|(column,w)|if *column==4{status}else{*w}).sum::<f32>();
    let flexible=(available-fixed).max(1.);
    for (column,w) in &mut columns {
        *w=match *column {0=>flexible*0.43,1=>flexible*0.57,4=>status,_=>*w};
    }
    columns
}

fn job_details(job:&Job,loaded:bool)->String{
    format!("{}\nAdres: {}\nGemeente: {}\nStatus: {}\nCoördinaten: {}\nAanmaakdatum: {}\nJob ID: {}\n{}",
        job.title,job.address,job.municipality,job_status_text(job.status),job.coordinates,
        job.client_date,job.job_id,if loaded{"Geladen in dit project"}else{"Nog niet geladen in dit project"})
}

fn job_action_button(ui:&mut egui::Ui,label:&str,enabled:bool,primary:bool)->egui::Response{
    ui.add_enabled(enabled,egui::Button::new(egui::RichText::new(label).strong()
        .color(if primary{Color32::WHITE}else{INK}))
        .fill(if primary{ORANGE}else{Color32::WHITE})
        .stroke(Stroke::new(1_f32,if primary{ORANGE}else{theme::BORDER}))
        .corner_radius(8).min_size(egui::vec2(0.,30.)))
}
fn job_status_text(status:i32)->&'static str{
    match status{
        1=>"Aangemaakt",2=>"Wacht op verwerking",3|4=>"In wachtrij",
        5=>"Verwerken",6=>"Controle nodig",7=>"Upload bezig",
        8=>"Mislukt",9|10=>"Gereed",11=>"Wacht op bestanden",
        _=>"Onbekend",
    }
}
fn job_badge(ui:&mut egui::Ui,text:&str,background:Color32,foreground:Color32) {
    egui::Frame::new().fill(background).corner_radius(7)
        .inner_margin(egui::Margin::symmetric(8,3)).show(ui,|ui|{
            ui.add(egui::Label::new(egui::RichText::new(text).size(12.).color(foreground)).selectable(false));
        });
}
fn job_status_colors(status:i32)->(Color32,Color32) {
    match status {
        9|10=>(Color32::from_rgb(232,246,238),Color32::from_rgb(35,114,74)),
        8=>(Color32::from_rgb(252,236,236),Color32::from_rgb(166,52,52)),
        2..=7|11=>(Color32::from_rgb(255,245,225),Color32::from_rgb(149,94,24)),
        _=>(Color32::from_rgb(238,242,247),theme::MUTED),
    }
}
fn show_job_card(ui:&mut egui::Ui,job:&Job,selected:bool,loaded:bool,
    striped:bool,width:f32,columns:&[(usize,f32)])->(egui::Response,Option<bool>)
{
    let (rect,_)=ui.allocate_exact_size(egui::vec2(width,JOB_ROW_HEIGHT),egui::Sense::hover());
    let response=ui.interact(rect,ui.make_persistent_id(("job-card",job.job_id)),egui::Sense::click());
    let fill=if selected{Color32::from_rgb(255,239,226)}
        else if response.hovered(){Color32::from_rgb(242,245,250)}
        else if striped{theme::SURFACE_ALT}else{Color32::WHITE};
    let border=if selected{Color32::from_rgb(245,178,125)}else{theme::BORDER};
    let painter=ui.painter_at(rect);
    painter.rect_filled(rect,6.,fill);
    painter.rect_stroke(rect,6.,Stroke::new(1_f32,border),egui::StrokeKind::Inside);
    if selected {
        painter.rect_filled(egui::Rect::from_min_size(rect.min+egui::vec2(2.,7.),egui::vec2(3.,20.)),2.,ORANGE);
    }
    // Only the outer allocation advances the virtualized list. Absolute
    // cell placement stays inside this row and keeps IDs stable while scrolling.
    let mut row_ui=ui.new_child(egui::UiBuilder::new().max_rect(rect)
        .id_salt(("job-card",job.job_id)).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let ui=&mut row_ui;
    let mut checked=selected;
    let check=ui.put(egui::Rect::from_min_size(rect.min+egui::vec2(8.,6.),egui::vec2(22.,22.)),
        egui::Checkbox::without_text(&mut checked));
    let mut x=rect.left()+36.;
    for &(column,column_width) in columns {
        let cell=egui::Rect::from_min_size(egui::pos2(x,rect.top()+4.),egui::vec2(column_width,26.));
        let value=job_column_value(job,column);
        if column==0 {
            let text_rect=if loaded {
                painter.circle_filled(egui::pos2(cell.left()+4.,cell.center().y),3.,Color32::from_rgb(35,114,74));
                egui::Rect::from_min_max(cell.min+egui::vec2(12.,0.),cell.max)
            }else{cell};
            ui.put(text_rect,egui::Label::new(egui::RichText::new(&value).strong().size(13.))
                .truncate().halign(egui::Align::Min).selectable(false));
        } else if column==4 {
            let (background,foreground)=job_status_colors(job.status);
            painter.rect_filled(cell.shrink2(egui::vec2(0.,2.)),5.,background);
            ui.put(cell.shrink2(egui::vec2(6.,0.)),egui::Label::new(egui::RichText::new(&value).size(12.).color(foreground))
                .truncate().halign(egui::Align::Min).selectable(false));
        } else {
            let text=egui::RichText::new(if value.is_empty(){"—"}else{&value})
                .color(if column==1{INK}else{theme::MUTED});
            ui.put(cell,egui::Label::new(text).truncate().halign(egui::Align::Min).selectable(false)).on_hover_text(&value);
        }
        x+=column_width+JOB_COLUMN_GAP;
    }
    (response.on_hover_text(job_details(job,loaded)),check.changed().then_some(checked))
}

fn show_job_header(ui:&mut egui::Ui,width:f32,columns:&[(usize,f32)],
    browser:&mut BrowserUi,filtered_ids:&[u64],inline_filters:bool){
    let height=if inline_filters{56.}else{28.};
    let (rect,_)=ui.allocate_exact_size(egui::vec2(width,height),egui::Sense::hover());
    ui.painter_at(rect).rect_filled(rect,6.,Color32::WHITE);
    let mut child=ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt("job-column-headings"));
    let mut all=!filtered_ids.is_empty()&&filtered_ids.iter().all(|id|browser.selected.contains(id));
    if child.put(egui::Rect::from_min_size(rect.min+egui::vec2(8.,3.),egui::vec2(22.,22.)),
        egui::Checkbox::without_text(&mut all)).on_hover_text("Alle zichtbare jobs selecteren of deselecteren").changed(){
        if all {
            browser.selected.extend(filtered_ids.iter().copied());
            browser.selection_anchor=filtered_ids.last().copied();
        }else{
            for id in filtered_ids{browser.selected.remove(id);}
            if browser.selection_anchor.is_some_and(|id|!browser.selected.contains(&id)){browser.selection_anchor=None;}
        }
    }
    let mut x=rect.left()+36.;
    for &(column,column_width) in columns {
        let name=match column{2=>"Coördinaten",3=>"Datum",6=>"ID",_=>JOB_COLUMNS[column].0};
        let arrow=if browser.sort_column==column{if browser.sort_descending{" ▼"}else{" ▲"}}else{""};
        let cell=egui::Rect::from_min_size(egui::pos2(x,rect.top()),egui::vec2(column_width,27.));
        let text=egui::RichText::new(format!("{name}{arrow}")).strong().size(12.)
            .color(if browser.sort_column==column{ORANGE}else{INK});
        if child.put(cell,egui::Button::new(text).frame(false).wrap_mode(egui::TextWrapMode::Truncate))
            .on_hover_text(format!("Sorteren op {}",JOB_COLUMNS[column].0)).clicked(){
            if browser.sort_column==column{browser.sort_descending=!browser.sort_descending;}
            else{browser.sort_column=column;browser.sort_descending=column==3;}
        }
        if inline_filters {
            let filter=egui::Rect::from_min_size(egui::pos2(x,rect.top()+29.),egui::vec2(column_width,24.));
            child.put(filter,egui::TextEdit::singleline(&mut browser.filters[column])
                .id(egui::Id::new(("kickthemap-job-filter",column)))
                .desired_width(column_width).hint_text("Filter…").margin(egui::vec2(5.,3.)));
        }
        x+=column_width+JOB_COLUMN_GAP;
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
enum OpenedJob {Ready(String),SigninRequired}
struct OpeningJob {
    events: Receiver<Result<(Browser, Result<OpenedJob, String>), String>>,
    job: Job,
    owner: sleufbase_rust::jobs_cache::SnapshotOwner,
    cancel: Arc<AtomicBool>,
    preserve_editor: bool,
    reused_browser: bool,
    allow_recovery:bool,
    project_generation:u64,
    expected_url:Option<String>,
    capture_generation: Option<u64>,
}
impl Drop for OpeningJob {
    fn drop(&mut self) { self.cancel.store(true, Ordering::Release); }
}
#[derive(Clone)]
struct PendingJobResume {
    job_id:u64,
    owner:sleufbase_rust::jobs_cache::SnapshotOwner,
    project_generation:u64,
    expected_url:Option<String>,
    capture_generation:Option<u64>,
}
struct JobLogin {
    events:Receiver<(Browser,Result<(KickTheMapLiveClient,Vec<Job>),String>)>,
    cancel:Arc<AtomicBool>,
    pending:PendingJobResume,
}
impl Drop for JobLogin {
    fn drop(&mut self){self.cancel.store(true,Ordering::Release);}
}

enum RepairSource{
    File(PathBuf),
    Browser{channel:sleufbase_rust::kickthemap_browser::BrowserChannel,expected_url:String},
}
pub(super) struct BrowserUi {
    pub(super) open:bool,
    pub(super) browser:Option<Browser>,
    pub(super) dock:Option<BrowserDock>,
    pub(super) host:isize,
    pub(super) web_visible:bool,
    pub(super) job_window_open:bool,
    #[cfg(test)] pub(super) launch_requests:usize,
    pub(super) capturing:Option<Receiver<Result<BrowserLogin,String>>>,
    login_status:Option<Receiver<&'static str>>,
    login_cancel:Arc<AtomicBool>,
    pub(super) startup_login_checked:bool,
    pub(super) restoring_login:bool,
    pub(super) show_password:bool,
    pub(super) selected:BTreeSet<u64>,
    selection_anchor:Option<u64>,
    filters:[String;7],
    only_downloadable:bool,
    only_loaded:bool,
    only_selected:bool,
    sort_column:usize,
    sort_descending:bool,
    map_open:bool,
    map_bounds:Option<Bounds>,
    map_interaction:jobs_map::Interaction,
    map_texture:Option<(Bounds,egui::TextureHandle)>,
    map_loading:Option<Receiver<Result<OsmRaster,String>>>,
    map_status:String,
    opening_job:Option<OpeningJob>,
    job_login:Option<JobLogin>,
    editor_owner:Option<sleufbase_rust::jobs_cache::SnapshotOwner>,
    pub(super) tools:kick_browser_tools::State,
    pub(super) reloading:Option<kick_browser_reload_ui::JobReload>,
    open_target:Option<u64>,
    pub(super) queue:VecDeque<(u64,RemoteFileKind)>,
    pub(super) batch_mode:bool,
    pub(super) batch_generation:u64,
    pub(super) downloaded_json:Vec<PathBuf>,
    pub(super) downloaded_tiffs:Vec<PathBuf>,
    pub(super) completed:usize,
    pub(super) failures:Vec<String>,
    pub(super) cancel:Arc<AtomicBool>,
    pub(super) download_notice:Option<Receiver<String>>,
    capture_status:Option<Receiver<String>>,
    manual_capture_job:Option<u64>,
    pub(super) status:String,
}
impl BrowserUi {
    pub(super) fn cancel_pending_job_open(&self) {
        if let Some(task)=&self.opening_job {task.cancel.store(true,Ordering::Release);}
        if let Some(task)=&self.job_login {task.cancel.store(true,Ordering::Release);}
    }
    pub(super) fn is_opening_job(&self)->bool {self.opening_job.is_some()||self.job_login.is_some()}

    fn select_row(&mut self, ordered:&[u64], position:usize, extend:bool) {
        let id=ordered[position];
        if extend {
            if let Some(anchor)=self.selection_anchor
                .filter(|id|self.selected.contains(id))
                .and_then(|id|ordered.iter().position(|&candidate|candidate==id))
            {
                self.selected.extend(ordered[anchor.min(position)..=anchor.max(position)].iter().copied());
                return;
            }
        }else{
            self.selected.clear();
        }
        self.selected.insert(id);
        self.selection_anchor=Some(id);
    }
}
#[cfg(test)]
#[test]
fn ctrl_click_selects_the_inclusive_range_in_current_list_order(){
    let mut browser=BrowserUi::default();
    let ordered=[9,3,17,6,2];
    browser.select_row(&ordered,3,false);
    browser.select_row(&ordered,1,true);
    assert_eq!(browser.selected,BTreeSet::from([3,6,17]));
    browser.select_row(&ordered,4,true);
    assert_eq!(browser.selected,BTreeSet::from([2,3,6,17]));
    browser.select_row(&ordered,0,false);
    assert_eq!(browser.selected,BTreeSet::from([9]));
    browser.select_row(&ordered[1..],2,true);
    assert_eq!(browser.selected,BTreeSet::from([6,9]));
}
impl Default for BrowserUi {
    fn default()->Self {
        Self {open:false,browser:None,dock:None,host:0,web_visible:false,job_window_open:false,capturing:None,login_status:None,login_cancel:Arc::default(),
            startup_login_checked:false,
            restoring_login:false,
            show_password:false,
            #[cfg(test)] launch_requests:0,
            selected:BTreeSet::new(),selection_anchor:None,filters:Default::default(),
            only_downloadable:false,only_loaded:false,only_selected:false,
            sort_column:3,sort_descending:true,
            map_open:false,map_bounds:None,map_interaction:Default::default(),map_texture:None,map_loading:None,
            map_status:String::new(),opening_job:None,job_login:None,editor_owner:None,
            tools:Default::default(),reloading:None,open_target:None,
            queue:VecDeque::new(),batch_mode:false,
            batch_generation:0,
            downloaded_json:Vec::new(),downloaded_tiffs:Vec::new(),
            completed:0,failures:Vec::new(),cancel:Arc::new(AtomicBool::new(false)),
            download_notice:None,capture_status:None,manual_capture_job:None,status:String::new()}
    }
}
pub(super) struct BrowserLogin {
    browser:Browser,
    session:KickTheMapLiveClient,
    jobs:Vec<Job>,
    remember:bool,
    cookies:Option<serde_json::Value>,
}

#[cfg(not(test))]
fn check_browser_cancel(cancel:&AtomicBool)->Result<(),String>{
    if cancel.load(Ordering::Acquire){Err("Aanmelden geannuleerd.".into())}else{Ok(())}
}

#[cfg(not(test))]
fn verify_browser_login(browser:&Browser,csrf:&str,known_nonempty:bool,cancel:&AtomicBool)
    ->Result<(KickTheMapLiveClient,Vec<Job>),String>
{
    check_browser_cancel(cancel)?;
    let cookies=browser.cookies()?;
    let user_agent=browser.user_agent()?;
    check_browser_cancel(cancel)?;
    let mut session=KickTheMapLiveClient::from_authenticated_browser_with_user_agent(&cookies,csrf,&user_agent)?;
    if known_nonempty{session.require_empty_confirmation();}
    let jobs=session.fetch_jobs_from_browser(||{
        check_browser_cancel(cancel)?;
        browser.browser_job_payload()
    })?;
    check_browser_cancel(cancel)?;
    Ok((session,jobs))
}

#[cfg(not(test))]
fn wait_for_job_login(mut browser:Browser,known_nonempty:bool,cancel:&AtomicBool)
    ->(Browser,Result<(KickTheMapLiveClient,Vec<Job>),String>)
{
    let result=(||{
        let deadline=Instant::now()+Duration::from_secs(10*60);
        let mut failures=0;
        while Instant::now()<deadline {
            check_browser_cancel(cancel)?;
            if browser.is_closed(){return Err("Het KickTheMap-aanmeldvenster is gesloten.".into());}
            if let Ok(csrf)=browser.csrf_token(){
                match verify_browser_login(&browser,&csrf,known_nonempty,cancel) {
                    Ok(session)=>return Ok(session),
                    Err(error)=>{
                        check_browser_cancel(cancel)?;
                        failures+=1;
                        if failures>=3{return Err(format!("De browseraanmelding kon de joblijst niet bevestigen: {error}"));}
                    }
                }
            }else{failures=0;}
            for _ in 0..10 {
                check_browser_cancel(cancel)?;
                std::thread::sleep(Duration::from_millis(150));
            }
        }
        Err("Aanmelden heeft te lang geduurd. Open de job opnieuw om verder te gaan.".into())
    })();
    // Return the owned WebView on every outcome. Only the UI can adopt its
    // identity or discard a cancelled request; workers never show it again.
    (browser,result)
}

#[cfg(not(test))]
fn connect_browser(
    dock:BrowserDock,host:isize,remember:bool,restore_only:bool,
    known_nonempty:bool,
    cancel:&AtomicBool,
    status:&mpsc::Sender<&'static str>,
)->Result<BrowserLogin,String>{
    use sleufbase_rust::kickthemap_session as stored;
    let check_cancel=||{
        if cancel.load(Ordering::Relaxed){Err("Aanmelden geannuleerd.".to_owned())}else{Ok(())}
    };
    check_cancel()?;
    let mut browser=Browser::launch_embedded(dock,host,remember)?;
    if stored::automatic_login_disabled() {
        // A failed previous cleanup must not silently revive an old account.
        browser.reset_signin()?;
    }
    let saved=remember.then(stored::load).and_then(Result::ok).flatten();
    let mut restored=false;
    if let Some(cookies)=saved {
        // WebView2 can report a page target before its first URL is committed.
        let deadline=Instant::now()+Duration::from_secs(4);
        loop{
            check_cancel()?;
            if browser.restore_session(&cookies).is_ok(){restored=true;break;}
            if browser.is_closed(){return Err("Het inlogvenster is gesloten.".into());}
            if Instant::now()>=deadline{break;}
            std::thread::sleep(Duration::from_millis(250));
        }
        if !restored && !restore_only{browser.reset_signin()?;}
    }
    if restore_only && !restored {
        // Older installs may only have the persistent WebView2 profile.
        // Navigate to the authenticated landing page to try that session.
        browser.open_account_home()?;
    }
    if !restored && !restore_only{
        check_cancel()?;
        browser.set_visible(true)?;
        let _=status.send("Meld je aan bij KickTheMap. De joblijst opent automatisch.");
    }
    let restore_until=Instant::now()+Duration::from_secs(10);
    let deadline=Instant::now()+Duration::from_secs(10*60);
    let mut job_errors=0;
    while Instant::now()<deadline {
        check_cancel()?;
        if browser.is_closed(){return Err("Het inlogvenster is gesloten.".into());}
        if let Ok(csrf)=browser.csrf_token(){
            match verify_browser_login(&browser,&csrf,known_nonempty,cancel){
                Ok((session,jobs))=>{
                    check_cancel()?;
                    browser.set_visible(false)?;
                    let cookies=if remember{browser.session_cookies().ok()}else{None};
                    return Ok(BrowserLogin{browser,session,jobs,remember,cookies});
                }
                Err(error) if !restored && !restore_only=>{
                    job_errors+=1;
                    if job_errors>=3{return Err(format!("Jobs ophalen mislukt: {error}"));}
                }
                Err(_)=>{}
            }
        }else{
            job_errors=0;
        }
        if (restored || restore_only) && Instant::now()>=restore_until{
            if restore_only {
                return Err("De bewaarde KickTheMap-sessie kon niet worden hersteld. Meld je opnieuw aan.".into());
            }
            browser.reset_signin()?;
            check_cancel()?;
            browser.set_visible(true)?;
            let _=status.send("Meld je opnieuw aan bij KickTheMap. De joblijst opent automatisch.");
            restored=false;
        }
        for _ in 0..10{
            check_cancel()?;
            std::thread::sleep(Duration::from_millis(150));
        }
    }
    Err("Aanmelden heeft te lang geduurd. Probeer opnieuw.".into())
}

impl SleufBaseApp {
    fn finish_kickthemap_login(&mut self,login:BrowserLogin,automatic:bool){
        self.adopt_kickthemap_login(login,automatic,true);
    }
    fn adopt_kickthemap_login(&mut self,login:BrowserLogin,automatic:bool,persist:bool){
        use sleufbase_rust::kickthemap_session as stored;
        let storage=if !persist{Ok(())}else if login.remember{
            stored::save(login.cookies.unwrap_or(serde_json::Value::Null))
        }else{stored::sign_out()};
        // Saved browser cookies never share identity with an editable email.
        self.shell.login_email.clear();
        self.shell.login_password.clear();
        self.shell.login_pending_account=None;
        if self.live_jobs_owner.as_ref()!=Some(login.session.snapshot_owner()){
            self.browser_ui.selected.clear();
            self.browser_ui.selection_anchor=None;
            self.jobs_cache=None;
        }
        self.live_jobs_owner=Some(login.session.snapshot_owner().clone());
        self.live_session=Some(login.session);
        self.live_jobs=login.jobs;
        if !self.project_sources_owned() && self.verify_kickthemap_raster_crs() { self.dirty = true; }
        self.reset_kickthemap_editor();
        self.browser_ui.editor_owner=self.live_jobs_owner.clone();
        self.browser_ui.browser=Some(login.browser);
        if !automatic { self.browser_ui.open=true; }
        self.browser_ui.web_visible=false;
        self.shell.login_open=false;
        self.save_live_cache();
        self.browser_ui.status=format!("KickTheMap verbonden: {} live jobs.",self.live_jobs.len());
        if storage.is_err(){
            self.browser_ui.status.push_str(if stored::automatic_login_disabled() {
                " Automatisch aanmelden staat uit. De bewaarde aanmelding kon niet volledig worden bijgewerkt of verwijderd."
            } else {
                " De eerdere bewaarde aanmelding kon niet worden vervangen of uitgeschakeld."
            });
        }
    }
    fn start_kickthemap_job_login(&mut self,browser:Browser,pending:PendingJobResume){
        let (tx,rx)=mpsc::channel();
        let cancel=Arc::new(AtomicBool::new(false));
        let known_nonempty=!self.live_jobs.is_empty();
        self.browser_ui.tools.cancel();
        self.browser_ui.tools=Default::default();
        self.browser_ui.editor_owner=None;
        self.browser_ui.browser=None;
        self.browser_ui.job_login=Some(JobLogin{events:rx,cancel:cancel.clone(),pending});
        self.browser_ui.job_window_open=true;
        self.browser_ui.status="Meld je aan in het aparte KickTheMap-venster. De geselecteerde job opent daarna automatisch.".into();
        self.message=self.browser_ui.status.clone();
        #[cfg(not(test))]
        std::thread::spawn(move||{
            let _=tx.send(wait_for_job_login(browser,known_nonempty,&cancel));
        });
        #[cfg(test)]
        {let _=(browser,known_nonempty,cancel,tx);}
    }
    fn poll_kickthemap_job_login(&mut self,ctx:&egui::Context){
        let event=self.browser_ui.job_login.as_ref().map(|task|task.events.try_recv());
        let result=match event {
            Some(Ok(result))=>Ok(result),
            Some(Err(TryRecvError::Disconnected))=>Err("Browseraanmelding onderbroken. Open de job opnieuw.".to_owned()),
            Some(Err(TryRecvError::Empty))=>{ctx.request_repaint_after(Duration::from_millis(150));return;}
            None=>return,
        };
        let Some(task)=self.browser_ui.job_login.take() else{return;};
        let pending=&task.pending;
        let same_owner=self.live_jobs_owner.as_ref()==Some(&pending.owner)
            &&self.live_session.as_ref().is_some_and(|session|session.snapshot_owner()==&pending.owner);
        let current_project=pending.project_generation==self.project_generation;
        let cancelled=task.cancel.load(Ordering::Acquire);
        match result {
            Ok((browser,Ok((session,jobs)))) if same_owner&&current_project&&!cancelled&&!browser.close_requested()=>{
                let same_job=pending.expected_url.as_deref().is_some_and(|expected|
                    session.verified_job_url(pending.job_id)==Some(expected))
                    &&jobs.iter().any(|job|job.job_id==pending.job_id);
                // Only a fully verified browser account replaces the native
                // client. Saved login choices remain unchanged by recovery.
                self.adopt_kickthemap_login(BrowserLogin{browser,session,jobs,remember:false,cookies:None},true,false);
                if same_job {
                    self.open_kickthemap_job_attempt(pending.job_id,pending.capture_generation,false);
                }else{
                    self.browser_ui.open=true;
                    self.browser_ui.status="Browseraanmelding bevestigd. De eerdere job hoort niet aantoonbaar bij deze sessie; selecteer de gewenste job opnieuw.".into();
                }
            }
            result=>{
                let error=match result {
                    Ok((browser,result))=>{
                        let _=browser.set_visible(false);
                        // Never expose an unverified recovery profile as a
                        // fallback for the old native client's SnapshotOwner.
                        result.err().unwrap_or_else(||"Browseraanmelding geannuleerd.".into())
                    }
                    Err(error)=>error,
                };
                if same_owner {self.hide_kickthemap_job_window();}
                self.browser_ui.status=if !same_owner {"Browseraanmelding niet overgenomen: de actieve aanmelding is gewijzigd.".into()}
                    else if !current_project {"Project gewijzigd; de eerdere browseractie is geannuleerd.".into()}
                    else if cancelled {"Browseraanmelding geannuleerd. Je eerdere KickTheMap-sessie blijft beschikbaar.".into()}
                    else {format!("Browseraanmelding niet overgenomen: {error}")};
            }
        }
        self.message=self.browser_ui.status.clone();
    }
    fn repair_kickthemap_download(&mut self,id:u64,path:PathBuf) {
        self.start_kickthemap_repair(id,RepairSource::File(path));
    }
    fn capture_kickthemap_download(&mut self,id:u64){
        // A native login may not have a browser yet. Resolve and open this
        // live job first: capture hooks cannot survive a later navigation.
        self.open_kickthemap_job(id,Some(self.project_generation));
    }
    fn begin_kickthemap_download_capture(&mut self,id:u64){
        if !self.browser_ui.job_window_open||self.live_operation_busy()||self.browser_ui.batch_mode{return;}
        if self.browser_ui.tools.job.as_ref().is_none_or(|job|job.job_id!=id)
            ||self.browser_ui.tools.expected_url.is_empty(){return;}
        let Some(browser)=self.browser_ui.browser.as_ref() else{return;};
        let channel=browser.channel();
        let expected_url=self.browser_ui.tools.expected_url.clone();
        self.start_kickthemap_repair(id,RepairSource::Browser{channel,expected_url});
    }
    pub(super) fn manual_kickthemap_capture_active(&self)->bool{
        self.browser_ui.manual_capture_job.is_some()&&self.live_download_rx.is_some()
    }
    pub(super) fn cancel_kickthemap_download_capture(&mut self){
        let pending=self.browser_ui.opening_job.as_ref()
            .filter(|task|task.capture_generation.is_some());
        if let Some(task)=pending {task.cancel.store(true,Ordering::Release);}
        let recovering=self.browser_ui.job_login.as_ref()
            .filter(|task|task.pending.capture_generation.is_some());
        if let Some(task)=recovering {task.cancel.store(true,Ordering::Release);}
        let active=self.manual_kickthemap_capture_active();
        if pending.is_none()&&recovering.is_none()&&!active{return;}
        if active{self.browser_ui.cancel.store(true,Ordering::Release);}
        self.browser_ui.status="Handmatige GeoTIFF-vastlegging annuleren…".into();
        self.message=self.browser_ui.status.clone();
    }
    fn start_kickthemap_repair(&mut self,id:u64,source:RepairSource) {
        if self.live_operation_busy() || self.browser_ui.batch_mode { return; }
        let Some(session)=self.live_session.as_ref() else {return;};
        if self.live_jobs_owner.as_ref()!=Some(session.snapshot_owner()) {
            self.message="Vernieuw eerst de jobs van de huidige sessie".into();return;
        }
        let Some(job)=self.live_jobs.iter().find(|j|j.job_id==id).cloned() else {return;};
        let manual_capture=matches!(&source,RepairSource::Browser{..});
        let mut session=self.live_session.take().unwrap();
        let (tx,rx)=mpsc::channel();
        let (notice_tx,notice_rx)=mpsc::sync_channel(1);
        let (status_tx,status_rx)=mpsc::sync_channel(1);
        self.browser_ui.capture_status=Some(status_rx);
        self.browser_ui.manual_capture_job=manual_capture.then_some(id);
        self.browser_ui.download_notice=Some(notice_rx);
        self.live_download_generation=self.project_generation;
        self.live_download_rx=Some(rx);
        self.live_busy=true;
        self.browser_ui.cancel.store(false,Ordering::SeqCst);
        let cancel=Arc::clone(&self.browser_ui.cancel);
        self.message=if manual_capture{format!("GeoTIFF-vastlegging voorbereiden voor job {id}…")}
            else{format!("Downloadwijze controleren met job {id}…")};
        self.browser_ui.status=self.message.clone();
        std::thread::spawn(move || {
            let capture=match source{
                RepairSource::File(path)=>read_download_capture(&path),
                RepairSource::Browser{channel,expected_url}=>channel.arm_job_download_capture(id,&expected_url,&cancel).and_then(|handle|{
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
        self.live_busy||self.live_rx.is_some()||self.live_refresh.is_some()||self.live_download_rx.is_some()||self.live_batch_rx.is_some()||self.browser_ui.capturing.is_some()||self.browser_ui.is_opening_job()
            ||self.browser_ui.reloading.is_some()||self.browser_ui.tools.writing()
    }
    pub(super) fn open_kickthemap_workspace(&mut self) {
        if self.browser_ui.is_opening_job() { self.cancel_kickthemap_job_open(); }
        else if self.browser_ui.capturing.is_some() { self.cancel_kickthemap_login(); }
        else { self.hide_kickthemap_pane(); }
        self.browser_ui.open=true;
        if self.jobs_cache.is_none(){self.open_newest_jobs_cache();}
    }
    fn open_newest_jobs_cache(&mut self) {
        if let Some(cache) = Self::newest_jobs_cache() {
            if !self.live_operation_busy() {
                self.browser_ui.status=format!("Offline cache geopend: {} jobs.",cache.jobs.len());
            }
            self.jobs_cache=Some(cache);
        }
    }
    pub(super) fn newest_jobs_cache()->Option<JobsCache> {
        let dir=workspace_cache_directory().join("jobs_cache");
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
        newest.map(|(_,cache)|cache)
    }
    pub(super) fn start_kickthemap_login(&mut self,remember:bool) {
        self.browser_ui.startup_login_checked=true;
        self.start_embedded_login(remember, false);
    }
    #[cfg(all(windows, not(test)))]
    pub(super) fn resume_kickthemap_on_startup(&mut self) {
        if self.browser_ui.startup_login_checked || self.live_operation_busy() {
            return;
        }
        self.browser_ui.startup_login_checked = true;
        if self.live_session.is_some() || self.shell.login_open || self.browser_ui.web_visible {
            return;
        }
        if sleufbase_rust::kickthemap_session::exists() {
            self.start_embedded_login(true, true);
        } else if sleufbase_rust::kickthemap_session::account_exists() {
            self.start_saved_login();
        }
    }
    fn start_embedded_login(&mut self, remember:bool, restore_only:bool) {
        if self.live_operation_busy()||self.browser_ui.batch_mode{return;}
        let known_nonempty=!self.live_jobs.is_empty()||self.jobs_cache.as_ref().is_some_and(|c|!c.jobs.is_empty());
        #[cfg(not(test))]
        let dock=match BrowserDock::new(){
            Ok(dock)=>dock,
            Err(error)=>{
                self.browser_ui.status=format!("KickTheMap-paneel openen: {error}");
                if restore_only { self.failed_kickthemap_restore(); }
                return;
            }
        };
        self.reset_kickthemap_editor();
        self.browser_ui.browser=None;
        let (tx,rx)=mpsc::channel();
        let (status_tx,status_rx)=mpsc::channel();
        self.browser_ui.capturing=Some(rx);
        self.browser_ui.login_status=Some(status_rx);
        self.browser_ui.login_cancel=Arc::default();
        self.browser_ui.restoring_login=restore_only;
        self.browser_ui.status=if restore_only { "Je bewaarde aanmelding herstellen…" } else { "Verbinding maken met KickTheMap…" }.into();
        self.message=self.browser_ui.status.clone();
        #[cfg(not(test))]
        {
            let cancel=Arc::clone(&self.browser_ui.login_cancel);
            let host=self.browser_ui.host;
            self.browser_ui.dock=Some(dock.clone());
            self.browser_ui.web_visible=!restore_only;
            self.shell.login_open=!restore_only;
            std::thread::spawn(move|| {
                let _=tx.send(connect_browser(dock,host,remember,restore_only,known_nonempty,&cancel,&status_tx));
            });
        }
        #[cfg(test)]
        {
            let _=(remember,known_nonempty,status_tx);
            self.browser_ui.launch_requests+=1;
            let _=tx.send(Err("Test: inlogvenster niet gestart".into()));
        }
    }
    fn failed_kickthemap_restore(&mut self) {
        // An editable email or native credential does not identify the account
        // inside the browser profile. Never switch identities automatically.
        self.shell.login_saved_attempted=true;
        self.open_kickthemap_login();
    }
    #[cfg(test)]
    pub(super) fn test_restore_kickthemap_login(&mut self) {
        self.start_embedded_login(true, true);
    }
    pub(super) fn cancel_kickthemap_login(&mut self) {
        self.browser_ui.startup_login_checked=true;
        self.shell.login_password.clear();
        self.browser_ui.show_password=false;
        self.browser_ui.login_cancel.store(true,Ordering::Relaxed);
        self.hide_kickthemap_job_window();
        self.hide_kickthemap_pane();
        self.browser_ui.status="Aanmelden geannuleerd.".into();
    }
    pub(super) fn poll_kickthemap_browser(&mut self,ctx:&egui::Context) {
        let window_closed=self.browser_ui.dock.as_ref().is_some_and(BrowserDock::take_window_close);
        let process_closed=self.browser_ui.browser.as_mut().is_some_and(|browser|browser.is_detached()&&browser.is_closed());
        if window_closed||process_closed {
            self.hide_kickthemap_job_window();
            if process_closed {self.browser_ui.browser=None;self.browser_ui.dock=None;}
            self.browser_ui.status="KickTheMap-venster gesloten. Open dezelfde job om verder te gaan.".into();
        }
        if self.browser_ui.manual_capture_job.is_some()&&self.live_download_rx.is_none(){
            self.browser_ui.manual_capture_job=None;
            self.browser_ui.capture_status=None;
            self.browser_ui.status=self.message.clone();
        }
        let changed_project=self.browser_ui.opening_job.as_ref().is_some_and(|task|
            task.project_generation!=self.project_generation)
            ||self.browser_ui.job_login.as_ref().is_some_and(|task|task.pending.project_generation!=self.project_generation);
        if changed_project {
            self.hide_kickthemap_job_window();
        }
        let opening=self.browser_ui.opening_job.as_ref().map(|task|task.events.try_recv());
        let result=match opening {
            Some(Ok(result))=>Some(result),
            Some(Err(TryRecvError::Disconnected))=>Some(Err("Jobvenster openen onderbroken".into())),
            Some(Err(TryRecvError::Empty))=>{ctx.request_repaint_after(Duration::from_millis(100));None},
            None=>None,
        };
        let mut capture_after_open=None;
        if let Some(result)=result {
            if let Some(task)=self.browser_ui.opening_job.take() {
                let same_owner=self.live_jobs_owner.as_ref()==Some(&task.owner)
                    && self.live_session.as_ref().is_some_and(|session|session.snapshot_owner()==&task.owner);
                let cancelled=task.cancel.load(Ordering::Acquire)||task.project_generation!=self.project_generation;
                if same_owner {
                    let recovery_required=matches!(&result,Ok((_,Ok(OpenedJob::SigninRequired))));
                    // Only the owning UI may show/focus an accepted job. A
                    // cancelled worker must never undo a later hide/login.
                    let result=match result {
                        Ok((browser,Ok(OpenedJob::Ready(url)))) if !cancelled=>{
                            let visible=browser.set_visible(true).map(|()|OpenedJob::Ready(url));
                            Ok((browser,visible))
                        }
                        Ok((browser,Ok(OpenedJob::SigninRequired))) if !cancelled&&task.allow_recovery=>{
                            let visible=browser.set_visible(true).map(|()|OpenedJob::SigninRequired);
                            Ok((browser,visible))
                        }
                        result=>result,
                    };
                    match result {
                        Ok((browser,Ok(OpenedJob::Ready(url)))) if !cancelled=>{
                            let same_editor=task.preserve_editor && self.browser_ui.editor_owner.as_ref()==Some(&task.owner)
                                && self.browser_ui.tools.job.as_ref().is_some_and(|job|job.job_id==task.job.job_id)
                                && self.browser_ui.tools.expected_url==url;
                            if !same_editor {self.browser_ui.tools.set_job(task.job.clone(),url);}
                            self.browser_ui.editor_owner=Some(task.owner.clone());
                            self.browser_ui.browser=Some(browser);
                            self.browser_ui.job_window_open=true;
                            self.browser_ui.status=format!("{} geopend in de KickTheMap-browser",task.job.title);
                            if task.capture_generation==Some(self.project_generation){
                                capture_after_open=Some(task.job.job_id);
                            }
                        }
                        Ok((browser,Ok(OpenedJob::SigninRequired))) if !cancelled&&task.allow_recovery=>{
                            self.start_kickthemap_job_login(browser,PendingJobResume{
                                job_id:task.job.job_id,owner:task.owner.clone(),
                                project_generation:task.project_generation,expected_url:task.expected_url.clone(),
                                capture_generation:task.capture_generation,
                            });
                        }
                        result=>{
                            let error=match result {
                                Ok((browser,result))=>{
                                    // A failed live verification must not destroy a
                                    // previously open drawing with unsaved geometry.
                                    let signin=matches!(&result,Ok(OpenedJob::SigninRequired));
                                    if task.reused_browser&&!recovery_required {self.browser_ui.browser=Some(browser);}
                                    if signin&&!cancelled {"De browser is aangemeld, maar deze job verwijst opnieuw naar aanmelden. Controleer je toegang en open de job opnieuw.".into()}
                                    else{result.err().unwrap_or_else(||"Openen geannuleerd".into())}
                                }
                                Err(error)=>error,
                            };
                            self.hide_kickthemap_job_window();
                            self.browser_ui.status=if changed_project {"Project gewijzigd; jobvenster of handmatige GeoTIFF-vastlegging niet gestart.".into()}
                                else if cancelled {"Jobvenster openen geannuleerd.".into()}
                                else {format!("Job openen mislukt: {error}")};
                        }
                    }
                }else{
                    // This result belongs to the former account. Dropping its
                    // owned browser must not hide a newer account's dock.
                    self.browser_ui.status="Jobvenster niet overgenomen: de aanmelding is gewijzigd.".into();
                }
                self.message=self.browser_ui.status.clone();
            }
        }
        // OpeningJob owns a different cancellation flag; dropping it must not
        // cancel the capture that starts after the browser is back in the UI.
        if let Some(id)=capture_after_open{self.begin_kickthemap_download_capture(id);}
        self.poll_kickthemap_job_login(ctx);
        jobs_map::poll_background(ctx,&mut self.browser_ui);
        match self.browser_ui.capture_status.as_ref().map(|rx|rx.try_recv()){
            Some(Ok(status))=>{
                if self.live_download_rx.is_some()&&self.live_busy&&self.live_download_generation==self.project_generation
                    &&!self.browser_ui.cancel.load(Ordering::Acquire){self.browser_ui.status=status.clone();self.message=status;}
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
        if let Some(status)=self.browser_ui.login_status.as_ref().and_then(|rx|rx.try_iter().last()){
            if !self.browser_ui.login_cancel.load(Ordering::Relaxed){
                self.browser_ui.status=status.into();
            }
        }
        if let Some(event)=self.browser_ui.capturing.as_ref().map(Receiver::try_recv){
            let result=match event {
                Ok(result)=>Some(result),
                Err(TryRecvError::Disconnected)=>Some(Err("Aanmelden onderbroken. Probeer opnieuw.".into())),
                Err(TryRecvError::Empty)=>{ctx.request_repaint_after(Duration::from_millis(150));None}
            };
            if let Some(result)=result{
                self.browser_ui.capturing=None;
                self.browser_ui.login_status=None;
                let automatic=std::mem::take(&mut self.browser_ui.restoring_login);
                if !self.browser_ui.login_cancel.load(Ordering::Relaxed){
                    match result{
                        Ok(login)=>self.finish_kickthemap_login(login,automatic),
                        Err(error)=>{
                            self.hide_kickthemap_pane();
                            self.browser_ui.status=error;
                            if automatic { self.failed_kickthemap_restore(); }
                        }
                    }
                }
                self.message=self.browser_ui.status.clone();
            }
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
        let root=workspace_cache_directory();
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
    pub(super) fn reset_kickthemap_editor(&mut self) {
        self.hide_kickthemap_job_window();
        self.browser_ui.opening_job=None; // Drop cancels the worker and rejects late results.
        self.browser_ui.job_login=None;
        self.browser_ui.tools.cancel();
        self.browser_ui.tools=Default::default();
        self.browser_ui.editor_owner=None;
    }

    pub(super) fn cancel_kickthemap_job_open(&mut self) {
        if self.browser_ui.is_opening_job() {
            // Keep the receiver until the worker returns an existing browser;
            // cancellation must not close an unsaved provider drawing.
            self.browser_ui.cancel_pending_job_open();
            self.browser_ui.status="Jobvenster openen annuleren…".into();
            self.message=self.browser_ui.status.clone();
        }
        self.hide_kickthemap_job_window();
    }

    fn open_selected_kickthemap_job(&mut self,id:u64){
        self.open_kickthemap_job(id,None);
    }
    fn open_kickthemap_job(&mut self,id:u64,capture_generation:Option<u64>){
        self.open_kickthemap_job_attempt(id,capture_generation,true);
    }
    fn open_kickthemap_job_attempt(&mut self,id:u64,capture_generation:Option<u64>,allow_recovery:bool){
        if self.live_operation_busy()||self.browser_ui.batch_mode{return;}
        let Some(session)=self.live_session.as_ref() else {
            self.browser_ui.status="Meld je eerst aan bij KickTheMap.".into();return;
        };
        let owner=session.snapshot_owner().clone();
        if self.live_jobs_owner.as_ref()!=Some(&owner) {
            self.browser_ui.status="Vernieuw eerst de jobs van de huidige aanmelding.".into();return;
        }
        let Some(job)=self.live_jobs.iter().find(|job|job.job_id==id).cloned() else {
            self.browser_ui.status="Deze job staat niet in de huidige live joblijst.".into();return;
        };
        let same_owner=self.browser_ui.editor_owner.as_ref()==Some(&owner);
        let reuse_browser=same_owner && self.browser_ui.browser.is_some()&&self.browser_ui.dock.is_some();
        let expected_url=session.verified_job_url(id).map(str::to_owned);
        let user_agent=session.browser_user_agent().map(str::to_owned);
        // Keep the complete authenticated WebView profile when it exists.
        // Native-only clients may attempt a narrow transfer; an unavailable
        // snapshot goes straight to visible browser sign-in instead of failing.
        let cookies=if reuse_browser{None}else{session.embedded_session_cookies().ok()};
        #[cfg(not(test))]
        let dock=match if reuse_browser {self.browser_ui.dock.as_ref().map(|dock|Ok(dock.detached_clone())).unwrap_or_else(BrowserDock::detached)}
            else {BrowserDock::detached()} {
            Ok(dock)=>dock,
            Err(error)=>{self.browser_ui.status=error.to_string();return;}
        };
        #[cfg(not(test))]
        {
            if allow_recovery {
                if let Err(error)=dock.begin_open(){self.browser_ui.status=error.to_string();return;}
            }
            if !reuse_browser {
                if let Some(user_agent)=user_agent.as_deref(){
                    if let Err(error)=dock.set_user_agent(user_agent){self.browser_ui.status=error;return;}
                }
            }
            let settings=self.project_settings.as_ref().unwrap_or(&self.migrated_settings);
            let prepared=sleufbase_rust::kickthemap_page_editor::PageConfig::from_settings(settings)
                .initialization_script().and_then(|script|dock.set_page_editor(&script));
            if let Err(error)=prepared {self.browser_ui.status=error;return;}
            let _=dock.set_title(&format!("{} — KickTheMap — SleufBase",job.title));
        }
        self.browser_ui.tools.cancel();
        let preserve_editor=reuse_browser && self.browser_ui.tools.job.as_ref().is_some_and(|job|job.job_id==id);
        if !preserve_editor {
            self.browser_ui.tools=Default::default();
        }
        let browser=if reuse_browser {self.browser_ui.browser.take()} else {None};
        let (tx,rx)=mpsc::channel();
        let cancel=Arc::new(AtomicBool::new(false));
        self.browser_ui.opening_job=Some(OpeningJob {events:rx,job,owner,cancel:cancel.clone(),preserve_editor,reused_browser:reuse_browser,
            project_generation:self.project_generation,expected_url,capture_generation,allow_recovery});
        self.browser_ui.web_visible=false;
        self.browser_ui.job_window_open=true;
        self.shell.login_open=false;
        self.browser_ui.status=if capture_generation.is_some(){format!("Job {id} openen voor handmatige GeoTIFF-vastlegging…")}
            else{format!("Job {id} openen in de KickTheMap-browser…")};
        #[cfg(not(test))]
        {
            let host=self.browser_ui.host;
            self.browser_ui.dock=Some(dock.clone());
            let _=std::thread::Builder::new().name("kickthemap-open-job".into()).spawn(move||{
                let result=(||{
                    let mut browser=match browser {
                        Some(browser)=>browser,
                        None=>Browser::launch_embedded_with_cancel(dock,host,false,&cancel)?,
                    };
                    if reuse_browser {
                        if let Err(error)=browser.promote_to_detached(&cancel){return Ok((browser,Err(error)));}
                    }
                    let result=if reuse_browser {
                        browser.open_job_hidden(id,&cancel)
                    }else if let Some(cookies)=cookies {
                        browser.prepare_authenticated_job_hidden(&cookies,id,&cancel)
                    }else{
                        let result=browser.open_signin_hidden(&cancel).map(|()|OpenedJob::SigninRequired);
                        return Ok((browser,result));
                    };
                    let result=match result {
                        Ok(url)=>Ok(OpenedJob::Ready(url)),
                        Err(error) if Browser::requires_signin(&error)=>Ok(OpenedJob::SigninRequired),
                        Err(error)=>Err(error),
                    };
                    Ok((browser,result))
                })();
                let _=tx.send(result);
            });
        }
        #[cfg(test)]
        {
            let _=(browser,cookies,cancel,user_agent);
            self.browser_ui.launch_requests+=1;
            let _=tx.send(Err("Test: jobbrowser niet gestart".into()));
        }
    }
    pub(super) fn show_kickthemap_workspace(&mut self,ctx:&egui::Context) {
        if !self.browser_ui.open {return;}
        let mut open=true;
        let mut action=None;
        let loaded=self.active_job_ids();
        let jobs=&self.live_jobs;
        let cache=self.jobs_cache.as_ref().map(|c|c.jobs.as_slice()).unwrap_or(&[]);
        // Keep the current list visible while its client is borrowed by a
        // worker (including replacement login); busy actions remain disabled.
        let live=self.live_session.is_some()||(self.live_busy&&self.live_jobs_owner.is_some())
            ||self.browser_ui.reloading.as_ref().is_some_and(|reload|
                self.live_jobs_owner.as_ref()==Some(reload.owner()));
        let busy=self.live_operation_busy();
        let refreshing=self.live_refresh.is_some();
        let downloading=self.live_download_rx.is_some()||self.browser_ui.batch_mode;
        let signing_in=self.browser_ui.capturing.is_some()||self.browser_ui.job_login.is_some()||(self.live_rx.is_some()&&self.shell.login_is_native);
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
        let active_filters=browser.filters.iter().filter(|value|!value.trim().is_empty()).count()
            +usize::from(browser.only_loaded)+usize::from(browser.only_selected)+usize::from(browser.only_downloadable);
        let capture_active=(browser.manual_capture_job.is_some()&&self.live_download_rx.is_some())
            ||browser.opening_job.as_ref().is_some_and(|task|task.capture_generation.is_some())
            ||browser.job_login.as_ref().is_some_and(|task|task.pending.capture_generation.is_some());
        egui::Window::new("SleufBase Jobs")
            .open(&mut open).default_width((ctx.screen_rect().width()-48.).clamp(340.,1120.))
            .default_height((ctx.screen_rect().height()-50.).clamp(260.,760.))
            .constrain_to(ctx.screen_rect().shrink(12.))
            .min_width(340.).min_height(240.)
            .movable(!browser.map_open || jobs_map::window_movable(ctx,&browser.map_interaction))
            .resizable(true).show(ctx,|ui|{
                settings_ui::control_style(ui);
                ui.set_max_width(ui.available_width());
                ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                ui.spacing_mut().item_spacing=egui::vec2(6.,4.);
                ui.spacing_mut().button_padding=egui::vec2(9.,5.);
                let visuals=ui.visuals_mut();
                for widget in [&mut visuals.widgets.inactive,&mut visuals.widgets.hovered,&mut visuals.widgets.active]{
                    widget.corner_radius=8.into();
                }

                // Reserve one compact action group; the list/map owns all of
                // the remaining height instead of a fixed-height content area.
                egui::TopBottomPanel::bottom("kickthemap-download-actions")
                    .frame(theme::card(10,8,6)).show_inside(ui,|ui|{
                    let width=ui.available_width();
                    let narrow=width<480.;
                    let can=live&&!busy&&!browser.batch_mode
                        &&!browser.selected.is_empty()&&browser.capturing.is_none();
                    let one=can&&browser.selected.len()==1;
                    ui.horizontal_wrapped(|ui|{
                        ui.label(egui::RichText::new(format!("{} geselecteerd · {} / {} jobs",
                            browser.selected.len(),indices.len(),source.len())).small().color(theme::MUTED));
                        if width>=520. {
                            if live{job_badge(ui,"Verbonden",Color32::from_rgb(232,246,238),Color32::from_rgb(35,114,74));}
                            else{job_badge(ui,"Offline cache",Color32::from_rgb(255,245,225),Color32::from_rgb(149,94,24));}
                        }
                    });
                    ui.horizontal_wrapped(|ui|{
                        if job_action_button(ui,if narrow{"Laad selectie"}else{"Laad geselecteerde jobs"},can,true)
                            .on_hover_text("Download objectpunten en GeoTIFF en laad ze in dit project.").clicked(){
                            action=Some("both");
                        }
                        if job_action_button(ui,if narrow{"Open browser"}else{"Open in KickTheMap-browser"},one,false)
                            .on_hover_text("Open één geselecteerde job in het aparte KickTheMap-venster.").clicked(){
                            action=Some("open-job");
                        }
                        if width>=800.&&job_action_button(ui,"Objecten opnieuw laden",one,false)
                            .on_hover_text("Werk de objecten van de geselecteerde job bij vanuit KickTheMap.").clicked(){
                            action=Some("reload-job");
                        }
                        ui.menu_button("Meer",|ui|{
                            if ui.add_enabled(can,egui::Button::new("Alleen objectpunten laden")).clicked(){action=Some("objects");ui.close_menu();}
                            if ui.add_enabled(can,egui::Button::new("Alleen GeoTIFF laden")).clicked(){action=Some("tiffs");ui.close_menu();}
                            if width<800.&&ui.add_enabled(one,egui::Button::new("Objecten opnieuw laden")).clicked(){action=Some("reload-job");ui.close_menu();}
                            ui.separator();
                            if ui.add_enabled(one,egui::Button::new("Handmatige GeoTIFF vastleggen")).clicked(){action=Some("repair-browser");ui.close_menu();}
                            if ui.add_enabled(one,egui::Button::new("Download herstellen met opgeslagen verzoek…")).clicked(){action=Some("repair");ui.close_menu();}
                            ui.separator();
                            ui.small("Kies één job voor de browser, opnieuw laden of handmatige vastlegging.");
                        });
                    });
                    if busy||browser.batch_mode||capture_active {
                        ui.horizontal_wrapped(|ui|{
                            ui.spinner();
                            if capture_active {
                                ui.label(if browser.job_login.is_some(){"Aanmelden in het aparte venster voor GeoTIFF…"}
                                    else if browser.opening_job.is_some(){"Job openen voor GeoTIFF…"}else{"Wachten op GeoTIFF in het jobvenster…"});
                                if ui.button("Vastlegging annuleren").clicked(){action=Some("cancel-capture");}
                            }else if downloading {
                                ui.label(if browser.batch_mode{format!("{} klaar · {} in wachtrij",browser.completed,browser.queue.len())}
                                    else{"Bestand downloaden…".into()});
                                if ui.button("Download annuleren").clicked(){action=Some("cancel");}
                            }else if refreshing{ui.label("Jobs vernieuwen…");}
                            else if signing_in {
                                ui.label("Aanmelden bij KickTheMap…");
                                if ui.button("Aanmelden annuleren").clicked(){action=Some("cancel-login");}
                            }else if browser.opening_job.is_some(){
                                ui.label("Jobvenster openen…");
                                if ui.button("Openen annuleren").clicked(){action=Some("cancel-open");}
                            }else if browser.reloading.is_some(){
                                ui.label("Objecten ophalen…");
                                if ui.button("Ophalen annuleren").clicked(){action=Some("cancel-reload");}
                            }
                            else{ui.label("KickTheMap-taak afronden…");}
                        });
                    }
                    if !browser.failures.is_empty(){
                        ui.menu_button(format!("Downloadfouten: {}",browser.failures.len()),|ui|{
                            egui::ScrollArea::vertical().max_height(180.).show(ui,|ui|{
                                for message in browser.failures.iter().rev().take(20){ui.label(message);}
                            });
                        });
                    }
                    if !browser.status.is_empty(){
                        ui.add(egui::Label::new(egui::RichText::new(&browser.status).small()).truncate())
                            .on_hover_text(&browser.status);
                    }
                });

                ui.horizontal_wrapped(|ui|{
                    if ui.selectable_label(!browser.map_open,"Lijst").clicked(){browser.map_open=false;browser.map_interaction=Default::default();}
                    if ui.selectable_label(browser.map_open,"Kaart").clicked(){
                        if !browser.map_open{browser.map_bounds=None;}browser.map_open=true;
                    }
                    ui.separator();
                    let filter_label=if active_filters==0{"Filter / selectie".into()}else{format!("Filters ({active_filters}) / selectie")};
                    ui.menu_button(filter_label,|ui|{
                        ui.set_max_width((ctx.screen_rect().width()-50.).min(320.));
                        egui::ScrollArea::vertical().max_height((ctx.screen_rect().height()-100.).max(100.)).show(ui,|ui|{
                            ui.checkbox(&mut browser.only_loaded,"Geladen jobs");
                            ui.checkbox(&mut browser.only_selected,"Alleen geselecteerd");
                            ui.checkbox(&mut browser.only_downloadable,"Downloadbaar");
                            ui.horizontal_wrapped(|ui|{
                                if ui.button("Alles selecteren").clicked(){
                                    browser.selected.extend(filtered_ids.iter().copied());
                                    browser.selection_anchor=filtered_ids.last().copied();
                                }
                                if ui.button("Selectie wissen").clicked(){browser.selected.clear();browser.selection_anchor=None;}
                            });
                            ui.separator();
                            egui::ComboBox::from_id_salt("jobs-sort-column").selected_text(JOB_COLUMNS[browser.sort_column].0)
                                .show_ui(ui,|ui|{
                                    for (column,(name,_)) in JOB_COLUMNS.iter().enumerate(){
                                        if ui.selectable_label(browser.sort_column==column,*name).clicked(){
                                            browser.sort_column=column;browser.sort_descending=column==3;
                                        }
                                    }
                                });
                            ui.checkbox(&mut browser.sort_descending,"Aflopend sorteren");
                            ui.separator();
                            for (column,(name,_)) in JOB_COLUMNS.iter().enumerate(){
                                ui.label(*name);
                                ui.add(egui::TextEdit::singleline(&mut browser.filters[column])
                                    .desired_width(230.).hint_text("Filter…"));
                            }
                            if ui.button("Alle filters wissen").clicked(){
                                browser.filters=Default::default();browser.only_loaded=false;
                                browser.only_selected=false;browser.only_downloadable=false;
                            }
                        });
                    });
                    if ui.add_enabled(live&&!busy,egui::Button::new("Vernieuwen")).clicked(){action=Some("refresh");}
                    if ui.add_enabled(!busy,egui::Button::new(if live{"Account"}else{"Aanmelden"})).clicked(){action=Some("login-dialog");}
                });
                ui.add_space(4.);
                if browser.map_open {
                    show_jobs_map(ui,source,&indices,browser,&mut action);
                }else{
                    // The scrollbar always reserves the same space, keeping
                    // headings and virtualized rows aligned during resizing.
                    let width=(ui.available_width()-ui.spacing().scroll.allocated_width()).max(1.);
                    let columns=job_columns_for_width(width);
                    let inline_filters=width>=600.&&ui.available_height()>=240.;
                    show_job_header(ui,width,&columns,browser,&filtered_ids,inline_filters);
                    ui.spacing_mut().item_spacing.y=2.;
                    if indices.is_empty(){
                        ui.add_space(8.);
                        ui.strong("Geen jobs gevonden");
                        ui.colored_label(theme::MUTED,if source.is_empty(){
                            "Meld je aan of vernieuw de joblijst."
                        }else{"Pas de filters aan om meer jobs te tonen."});
                        if !source.is_empty()&&ui.button("Filters wissen").clicked(){
                            browser.filters=Default::default();browser.only_loaded=false;
                            browser.only_selected=false;browser.only_downloadable=false;
                        }
                    }else{
                        egui::ScrollArea::vertical().id_salt("jobs-vertical")
                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                            .max_height(ui.available_height().max(1.)).auto_shrink([false,false])
                            .show_rows(ui,JOB_ROW_HEIGHT,indices.len(),|ui,rows|{
                                for position in rows{
                                    let job=&source[indices[position]];
                                    let extend=ui.input(|i|i.modifiers.ctrl||i.modifiers.command||i.modifiers.shift);
                                    let (response,checked)=show_job_card(ui,job,browser.selected.contains(&job.job_id),
                                        loaded.contains(&job.job_id),position%2!=0,width,&columns);
                                    if let Some(selected)=checked {
                                        if extend{browser.select_row(&filtered_ids,position,true);}
                                        else if selected{browser.selected.insert(job.job_id);browser.selection_anchor=Some(job.job_id);}
                                        else{
                                            browser.selected.remove(&job.job_id);
                                            if browser.selection_anchor==Some(job.job_id){browser.selection_anchor=None;}
                                        }
                                    }else{
                                        if response.clicked(){browser.select_row(&filtered_ids,position,extend);}
                                        if response.double_clicked()&&!extend&&live&&!busy&&!browser.batch_mode{
                                            browser.select_row(&filtered_ids,position,false);
                                            action=Some("both");
                                        }
                                    }
                                }
                            });
                    }
                }
            });
        self.browser_ui.open=open;
        if !open{self.browser_ui.map_interaction=Default::default();}
        match action {
            Some("login-dialog")=>self.open_kickthemap_login(),
            Some("cancel-login")=>self.cancel_kickthemap_login(),
            Some("open-job")=>{
                if let Some(id)=self.browser_ui.open_target.take().or(selected_single){
                    self.open_selected_kickthemap_job(id);
                }
            },
            Some("reload-job")=>{
                if let Some(id)=self.browser_ui.selected.first().copied(){self.start_kickthemap_object_reload(id);}
            },
            Some("cancel-capture")=>self.cancel_kickthemap_download_capture(),
            Some("cancel-open")=>self.cancel_kickthemap_job_open(),
            Some("cancel-reload")=>{
                if let Some(reload)=&self.browser_ui.reloading{reload.cancel();}
                self.browser_ui.status="Objecten ophalen annuleren…".into();
            },
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

pub(super) fn job_rd_coordinates(text:&str)->Option<[f64;2]>{
    // Match the two leading numbers in the original Jobs map. Both RD and
    // longitude/latitude are accepted; malformed values are not plotted.
    let mut words=Vec::new();
    let mut number=String::new();
    for c in text.chars().chain(std::iter::once(' ')){
        if c.is_ascii_digit()||((c=='-'||c=='.'||c==',')&&number.len()<30) {
            if c==',' {number.push('.');} else{number.push(c);}
        }else if !number.is_empty(){
            if let Ok(n)=number.trim_end_matches('.').parse::<f64>(){words.push(n);}
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
fn show_jobs_map(ui:&mut egui::Ui,source:&[Job],indices:&[usize],
    browser:&mut BrowserUi,action:&mut Option<&'static str>){
    jobs_map::show(ui,source,indices,browser,action);
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

    fn native_editor_fixture()->SleufBaseApp {
        let cookie=sleufbase_rust::kickthemap_browser::BrowserCookie {
            name:"fixture".into(),value:"local-only".into(),domain:"my.kickthemap.com".into(),
            path:"/".into(),secure:true,
        };
        let session=KickTheMapLiveClient::from_authenticated_browser(&[cookie],"fixture-csrf").unwrap();
        let mut app=SleufBaseApp::default();
        app.live_jobs_owner=Some(session.snapshot_owner().clone());
        app.live_session=Some(session);
        app.live_jobs=vec![Job {job_id:42,title:"Kabeljob".into(),address:String::new(),
            municipality:String::new(),coordinates:String::new(),client_date:String::new(),
            status:9,downloadable:true,prefix:String::new()}];
        app.browser_ui.open=true;
        app.browser_ui.selected.insert(42);
        app
    }

    #[test]
    fn manual_geotiff_action_opens_the_selected_job_without_a_previous_browser() {
        let mut app=native_editor_fixture();
        let owner=app.live_jobs_owner.clone();
        let ctx=egui::Context::default();theme::install(&ctx);
        let render=|app:&mut SleufBaseApp,events|ctx.run(egui::RawInput {
            screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(1450.,1000.))),
            events,..Default::default()
        },|ctx|app.show_kickthemap_workspace(ctx));
        let position=|output:&egui::FullOutput,label:&str|output.shapes.iter().find_map(|shape|match &shape.shape {
            egui::Shape::Text(text) if text.galley.text()==label=>{
                let point=text.pos+text.galley.rect.center().to_vec2();
                shape.clip_rect.contains(point).then_some(point)
            },_=>None,
        }).unwrap_or_else(||panic!("visible action missing: {label}"));
        let mut output=render(&mut app,vec![]);
        for _ in 0..4 {output=render(&mut app,vec![]);}
        for label in ["Meer","Handmatige GeoTIFF vastleggen"] {
            let point=position(&output,label);
            render(&mut app,vec![egui::Event::PointerMoved(point)]);
            for pressed in [true,false] {
                output=render(&mut app,vec![egui::Event::PointerButton {
                    pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default(),
                }]);
            }
            for _ in 0..3 {output=render(&mut app,vec![]);}
        }
        assert_eq!(app.browser_ui.launch_requests,1);
        let task=app.browser_ui.opening_job.as_ref().expect("the manual action first opens the selected job");
        assert_eq!(task.job.job_id,42);
        assert_eq!(task.capture_generation,Some(app.project_generation));
        assert!(app.browser_ui.job_window_open);
        assert!(!app.browser_ui.web_visible);
        assert!(!ctx.memory(|memory|memory.any_popup_open()),"the menu closes before the separate browser opens");
        assert!(app.live_download_rx.is_none()&&!app.manual_kickthemap_capture_active(),"never arm before the job document is ready");
        app.poll_kickthemap_browser(&ctx); // No real browser is launched by UI tests.
        assert!(!app.browser_ui.job_window_open&&app.live_download_rx.is_none());
        assert_eq!(app.live_jobs_owner,owner);
        assert_eq!(app.live_session.as_ref().map(|session|session.snapshot_owner()),owner.as_ref());
    }

    #[test]
    fn pending_manual_capture_is_cancelled_on_project_cancel_or_account_change() {
        for change in ["project","cancel","hide","login","account"] {
            let mut app=native_editor_fixture();
            app.capture_kickthemap_download(42);
            let task=app.browser_ui.opening_job.as_ref().unwrap();
            let cancel=task.cancel.clone();
            let generation=task.capture_generation.unwrap();
            match change {
                "project"=>app.execute_project_action(ProjectAction::New,&egui::Context::default()),
                "cancel"=>app.cancel_kickthemap_job_open(),
                "hide"=>app.hide_kickthemap_job_window(),
                "login"=>app.open_kickthemap_login(),
                _=>app.live_jobs_owner=None,
            }
            if matches!(change,"cancel"|"hide"|"login") {
                assert!(cancel.load(Ordering::Acquire),"hiding the job window cancels the pending intent before its reply");
                assert!(app.browser_ui.opening_job.is_some(),"retain the receiver so an existing browser can return");
            }
            app.poll_kickthemap_browser(&egui::Context::default());
            assert!(cancel.load(Ordering::Acquire));
            assert!(app.browser_ui.opening_job.is_none()&&app.live_download_rx.is_none());
            assert!(!app.manual_kickthemap_capture_active()&&!app.browser_ui.job_window_open);
            assert!(app.live_session.is_some(),"an aborted browser opening does not borrow the native login");
            if change=="project" {
                assert_ne!(generation,app.project_generation);
                assert!(app.browser_ui.status.contains("Project gewijzigd"));
            } else if change=="account" {
                assert!(app.browser_ui.status.contains("aanmelding is gewijzigd"));
            } else {assert!(app.browser_ui.status.contains("geannuleerd"));}
        }
    }

    #[test]
    fn manual_capture_page_can_cancel_and_return_its_session_without_clearing_the_editor() {
        let mut app=native_editor_fixture();
        let session=app.live_session.take().unwrap();
        let owner=session.snapshot_owner().clone();
        let (download_tx,download_rx)=mpsc::channel();
        app.live_download_rx=Some(download_rx);app.live_busy=true;
        app.live_download_generation=app.project_generation;
        app.browser_ui.manual_capture_job=Some(42);
        app.browser_ui.job_window_open=true;
        let expected_url="https://www.my.kickthemap.com/viewJob/fixture_2026-11-16";
        app.browser_ui.tools.set_job(app.live_jobs[0].clone(),expected_url.into());
        let ctx=egui::Context::default();theme::install(&ctx);
        let render=|app:&mut SleufBaseApp,events|ctx.run(egui::RawInput {
            screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(1200.,760.))),
            events,..Default::default()
        },|ctx|app.show_kickthemap_workspace(ctx));
        let mut output=render(&mut app,vec![]);
        for _ in 0..3 {output=render(&mut app,vec![]);}
        let point=output.shapes.iter().find_map(|shape|match &shape.shape {
            egui::Shape::Text(text) if text.galley.text()=="Vastlegging annuleren"=>{
                let point=text.pos+text.galley.rect.center().to_vec2();
                shape.clip_rect.contains(point).then_some(point)
            },_=>None,
        }).expect("capture cancellation stays accessible in Jobs while the separate job page is shown");
        render(&mut app,vec![egui::Event::PointerMoved(point)]);
        for pressed in [true,false] {render(&mut app,vec![egui::Event::PointerButton {
            pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default(),
        }]);}
        assert!(app.browser_ui.cancel.load(Ordering::Acquire));
        assert!(app.browser_ui.job_window_open,"cancelling capture does not close the job browser");
        let (status_tx,status_rx)=mpsc::channel();app.browser_ui.capture_status=Some(status_rx);
        status_tx.send("Klaar na annuleren".into()).unwrap();
        let cancelled_status=app.browser_ui.status.clone();
        app.poll_kickthemap_browser(&ctx);
        assert_eq!(app.browser_ui.status,cancelled_status,"ignore a late armed notice after cancellation");
        app.browser_ui.cancel.store(false,Ordering::Release);
        app.hide_kickthemap_job_window();
        assert!(app.browser_ui.cancel.load(Ordering::Acquire),"leaving the visible page also cancels its capture");
        assert!(!app.browser_ui.job_window_open);
        assert_eq!(app.browser_ui.tools.job.as_ref().unwrap().job_id,42);
        assert_eq!(app.browser_ui.tools.expected_url,expected_url);
        download_tx.send((session,Err("Vastlegging geannuleerd".into()))).ok().unwrap();
        app.poll_live_download(&ctx);app.poll_kickthemap_browser(&ctx);
        assert_eq!(app.live_session.as_ref().map(|session|session.snapshot_owner()),Some(&owner));
        assert!(!app.manual_kickthemap_capture_active()&&!app.live_busy);
        assert!(app.live_download_rx.is_none()&&app.file_intent.is_none());
    }

    #[test]
    fn jobs_button_opens_editor_after_native_login_without_a_previous_browser() {
        let mut app=native_editor_fixture();
        let owner=app.live_jobs_owner.clone();
        let ctx=egui::Context::default();theme::install(&ctx);
        let render=|app:&mut SleufBaseApp,events|ctx.run(egui::RawInput {
            screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(1450.,1000.))),
            events,..Default::default()
        },|ctx|app.show_kickthemap_workspace(ctx));
        let mut output=render(&mut app,vec![]);
        for _ in 0..4 {output=render(&mut app,vec![]);}
        let position=output.shapes.iter().find_map(|shape|match &shape.shape {
            egui::Shape::Text(text) if text.galley.text()=="Open in KickTheMap-browser"=>{
                let point=text.pos+text.galley.rect.center().to_vec2();
                shape.clip_rect.contains(point).then_some(point)
            },_=>None,
        }).expect("the browser entry must be visible in the normal jobs actions");
        render(&mut app,vec![egui::Event::PointerMoved(position)]);
        for pressed in [true,false] {
            render(&mut app,vec![egui::Event::PointerButton {pos:position,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);
        }
        assert_eq!(app.browser_ui.launch_requests,1);
        assert!(app.browser_ui.is_opening_job() && app.browser_ui.job_window_open);
        app.poll_kickthemap_browser(&ctx); // deterministic test worker failure
        assert!(!app.browser_ui.is_opening_job() && !app.browser_ui.job_window_open);
        assert_eq!(app.live_jobs_owner,owner);
        assert_eq!(app.live_session.as_ref().map(|session|session.snapshot_owner()),owner.as_ref());
        assert!(app.browser_ui.status.contains("Job openen mislukt"));
    }

    #[test]
    fn cancelling_open_rejects_queued_result_without_losing_the_native_account() {
        let mut app=native_editor_fixture();
        app.open_selected_kickthemap_job(42);
        let cancel=app.browser_ui.opening_job.as_ref().unwrap().cancel.clone();
        app.cancel_kickthemap_job_open();
        assert!(cancel.load(Ordering::Acquire));
        assert!(!app.browser_ui.job_window_open);
        assert!(app.browser_ui.is_opening_job(),"keep receiver until an existing browser can be returned");
        app.poll_kickthemap_browser(&egui::Context::default());
        assert!(!app.browser_ui.is_opening_job());
        assert!(app.live_session.is_some() && app.live_jobs_owner.is_some());
        assert_eq!(app.browser_ui.status,"Jobvenster openen geannuleerd.");
    }

    #[test]
    fn cached_or_unowned_job_cannot_start_an_editor() {
        let mut app=native_editor_fixture();
        app.open_selected_kickthemap_job(99);
        assert_eq!(app.browser_ui.launch_requests,0);
        app.live_jobs_owner=None;
        app.open_selected_kickthemap_job(42);
        assert_eq!(app.browser_ui.launch_requests,0);
        assert!(!app.browser_ui.job_window_open);
    }
    #[test]
    fn dragging_jobs_window_taller_reveals_more_rows(){
        let cookie=sleufbase_rust::kickthemap_browser::BrowserCookie{
            name:"fixture".into(),value:"local-only".into(),domain:"my.kickthemap.com".into(),
            path:"/".into(),secure:true,
        };
        let session=KickTheMapLiveClient::from_authenticated_browser(&[cookie],"fixture-csrf").unwrap();
        let mut app=SleufBaseApp{live_session:Some(session),..Default::default()};
        app.browser_ui.open=true;
        app.live_jobs=(1..80).map(|job_id|Job{
            job_id,title:format!("Resize fixture {job_id}"),address:String::new(),
            municipality:String::new(),coordinates:String::new(),client_date:String::new(),
            status:9,downloadable:true,prefix:String::new(),
        }).collect();
        let ctx=egui::Context::default();
        theme::install(&ctx);
        let render=|app:&mut SleufBaseApp,events|ctx.run(egui::RawInput{
            screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(1500.,1150.))),
            events,..Default::default()
        },|ctx|app.show_kickthemap_workspace(ctx));
        let visible=|output:&egui::FullOutput|output.shapes.iter().filter(|shape|matches!(
            &shape.shape, egui::Shape::Text(text) if text.galley.text().starts_with("Resize fixture ")
                &&shape.clip_rect.contains(text.pos+text.galley.rect.center().to_vec2())
        )).count();
        let mut output=render(&mut app,vec![]);
        for _ in 0..5{output=render(&mut app,vec![]);}
        let before=visible(&output);
        let rect=ctx.memory(|memory|memory.area_rect(egui::Id::new("SleufBase Jobs"))).unwrap();
        let start=rect.center_bottom();
        // Additional compact rows must use the newly available list height.
        let end=start+egui::vec2(0.,180.);
        let button=|pos,pressed|egui::Event::PointerButton{
            pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default(),
        };
        render(&mut app,vec![egui::Event::PointerMoved(start)]);
        render(&mut app,vec![button(start,true)]);
        render(&mut app,vec![egui::Event::PointerMoved(end)]);
        render(&mut app,vec![button(end,false)]);
        for _ in 0..5{output=render(&mut app,vec![]);}
        let after=visible(&output);
        assert!(after>=before+3,"visible jobs before {before}, after {after}");
    }
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

    #[test]
    fn busy_jobs_window_only_offers_download_cancellation_for_downloads() {
        for operation in ["refresh","login","download"] {
            let mut app=app();let session=app.live_session.take().unwrap();app.live_busy=true;
            let (refresh_tx,refresh_rx)=mpsc::channel();
            let (login_tx,login_rx)=mpsc::channel();
            let (download_tx,download_rx)=mpsc::channel();
            match operation {
                "refresh"=>app.live_refresh=Some(LiveJobsRefresh{owner:session.snapshot_owner().clone(),events:refresh_rx}),
                "login"=>{app.live_rx=Some(login_rx);app.shell.login_is_native=true;},
                _=>app.live_download_rx=Some(download_rx),
            }
            let ctx=egui::Context::default();theme::install(&ctx);let mut output=None;
            for _ in 0..4 {
                output=Some(ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(1200.,760.))),..Default::default()},|ctx|app.show_kickthemap_workspace(ctx)));
            }
            let output=output.unwrap();
            let visible=|label:&str|output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()==label));
            assert_eq!(visible("Download annuleren"),operation=="download","{operation}");
            assert!(visible(match operation{"refresh"=>"Jobs vernieuwen…","login"=>"Aanmelden bij KickTheMap…",_=>"Bestand downloaden…"}));
            assert!(visible("PS42"));
            if operation=="refresh" {
                app.open_kickthemap_login();let ctx=egui::Context::default();theme::install(&ctx);
                let mut output=None;
                for _ in 0..4 {
                    output=Some(ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(1200.,760.))),..Default::default()},|ctx|app.show_kickthemap_login(ctx)));
                }
                let output=output.unwrap();
                let visible=|label:&str|output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()==label));
                assert!(visible("Sluiten")&&visible("Je jobs worden vernieuwd…"));
                assert!(!visible("Annuleren"));
            }
            drop((refresh_tx,login_tx,download_tx,session));
        }
    }
}

#[cfg(test)]
mod login_restore_tests {
    use super::*;

    #[test]
    fn failed_browser_restore_requires_account_confirmation_without_native_fallback() {
        let mut app=SleufBaseApp::default();
        app.shell.login_email="earlier-native-account@example.test".into();
        app.test_restore_kickthemap_login();
        assert!(!app.shell.login_open && !app.browser_ui.web_visible && !app.browser_ui.open);
        app.poll_kickthemap_browser(&egui::Context::default());
        assert!(app.shell.login_open);
        assert!(app.shell.login_saved_attempted);
        assert!(!app.browser_ui.web_visible && !app.browser_ui.open);
        assert!(!app.live_operation_busy());
        assert!(app.live_rx.is_none());
        assert_eq!(app.browser_ui.launch_requests,1);
    }

    #[test]
    fn cancelling_browser_restore_does_not_reopen_login_or_retry_credentials() {
        let mut app=SleufBaseApp::default();
        app.test_restore_kickthemap_login();
        app.shell.login_password="test-only".into();
        app.browser_ui.show_password=true;
        app.cancel_kickthemap_login();
        app.poll_kickthemap_browser(&egui::Context::default());
        assert!(!app.shell.login_open && !app.browser_ui.web_visible && !app.browser_ui.open);
        assert!(!app.live_operation_busy());
        assert!(!app.browser_ui.restoring_login);
        assert_eq!(app.browser_ui.status,"Aanmelden geannuleerd.");
        assert!(app.browser_ui.startup_login_checked);
        assert!(app.shell.login_password.is_empty() && !app.browser_ui.show_password);
        assert_eq!(app.browser_ui.launch_requests,1);
    }
}
