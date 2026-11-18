//! Local LAS/LAZ import, Potree window ownership and project integration.
use super::*;
use sleufbase_rust::pointcloud::{self, AnnotationKind, CloudCrs, PointCloudSource};
use sleufbase_rust::pointcloud_server::Session;
use std::{collections::{HashMap, VecDeque}, process::Child, sync::{Arc, atomic::{AtomicBool, Ordering}}};
#[cfg(windows)]
use std::process::{Command, Stdio};

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    queue: VecDeque<PathBuf>,
    form: Option<ImportForm>,
    worker: Option<ImportWorker>,
    editor: Option<Editor>,
    trench: Option<TrenchForm>,
    titles: HashMap<String,String>,
    status: String,
}
impl State {
    pub fn busy(&self) -> bool { self.worker.is_some() || self.editor.is_some() }
    pub fn importing(&self) -> bool { self.worker.is_some() }
    pub fn reset(&mut self) { *self = Self::default(); }
}
struct ImportForm { path: PathBuf, title: String, crs: CloudCrs }
enum ImportEvent { Progress(String), Done(Result<PointCloudSource,String>) }
struct ImportWorker { rx: Receiver<ImportEvent>, cancel: Arc<AtomicBool>, generation: u64 }
impl Drop for ImportWorker { fn drop(&mut self) { self.cancel.store(true,Ordering::Release); } }
struct Editor { session: Session, child: Child, generation: u64, source_id: String, revision: u64, _log: tempfile::NamedTempFile }
impl Drop for Editor { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }
struct TrenchForm { source_id: String, axis_id: String, selected_ids: Vec<String>, label: String, width: f64 }

fn crs_label(crs: &CloudCrs) -> &'static str {
    match crs { CloudCrs::Unknown => "Onbekend / uit bestand lezen", CloudCrs::RdNew => "RD New (EPSG:28992), meters", CloudCrs::Other => "Ander / lokaal coördinatenstelsel" }
}
fn crs_combo(ui:&mut egui::Ui, id:impl std::hash::Hash, crs:&mut CloudCrs) -> bool {
    let before=std::mem::discriminant(crs);
    egui::ComboBox::from_id_salt(id).selected_text(crs_label(crs)).show_ui(ui,|ui|{
        for value in [CloudCrs::Unknown,CloudCrs::RdNew,CloudCrs::Other] {
            if ui.selectable_label(std::mem::discriminant(crs)==std::mem::discriminant(&value),crs_label(&value)).clicked(){*crs=value;}
        }
    });
    before!=std::mem::discriminant(crs)
}

impl SleufBaseApp {
    pub(super) fn queue_pointcloud_files(&mut self, files:Vec<PathBuf>) {
        if files.len()>32 { self.message="Open maximaal 32 puntenwolken tegelijk.".into();return; }
        if self.pointcloud_ui.busy() { self.message="Sluit eerst de puntenwolkeditor of rond de import af.".into();return; }
        self.pointcloud_ui.open=true;
        for file in files { if !self.pointcloud_ui.queue.contains(&file){self.pointcloud_ui.queue.push_back(file);} }
    }
    pub(super) fn choose_pointcloud_files(&mut self) {
        if let Some(files)=rfd::FileDialog::new().set_title("Puntenwolken openen")
            .add_filter("LAS / LAZ puntenwolk",&["las","laz"]).pick_files() {
            self.queue_pointcloud_files(files);
        }
    }
    pub(super) fn pointcloud_blocks_action(&mut self) -> bool {
        self.collect_pointcloud_edits();
        if self.pointcloud_ui.busy() {
            self.pointcloud_ui.open=true;
            self.message="Sluit eerst de puntenwolkeditor of rond de import af. De tekenobjecten worden daarbij in het project overgenomen.".into();
            true
        } else { false }
    }
    fn collect_pointcloud_edits(&mut self) {
        let mut changed=false;
        if let Some(editor)=self.pointcloud_ui.editor.as_mut() {
            while let Some(update)=editor.session.poll() {
                if editor.generation!=self.project_generation || update.source_id!=editor.source_id || update.revision<=editor.revision { continue; }
                if let Some(source)=self.point_clouds.iter_mut().find(|s|s.id==update.source_id) {
                    source.annotations=update.annotations;
                    editor.revision=update.revision;
                    changed=true;
                }
            }
        }
        if changed { self.sync_pointcloud_datasets();self.dirty=true; }
    }
    pub(super) fn sync_pointcloud_datasets(&mut self) {
        let selected=self.selected.and_then(|index|self.datasets.get(index)).map(|d|d.source_path.clone());
        self.datasets.retain(|d|!pointcloud::is_cloud_dataset(d));
        for source in &self.point_clouds {
            if !source.annotations.is_empty() {
                if let Ok(dataset)=source.to_dataset() { self.datasets.push(dataset); }
            }
        }
        self.selected=selected.and_then(|path|self.datasets.iter().position(|d|d.source_path==path));
    }
    pub(super) fn poll_pointcloud(&mut self,ctx:&egui::Context) {
        self.collect_pointcloud_edits();
        let ended=self.pointcloud_ui.editor.as_mut().and_then(|e|e.child.try_wait().ok().flatten());
        if let Some(status)=ended {
            if let Some(editor)=self.pointcloud_ui.editor.as_mut(){editor.session.close();}
            self.collect_pointcloud_edits();
            self.pointcloud_ui.editor=None;
            self.pointcloud_ui.status=if status.success(){"Editor gesloten. Sla het project op om je tekenobjecten te bewaren.".into()}
                else{"De puntenwolkeditor is gestopt. Reeds opgeslagen tekenobjecten blijven in het project.".into()};
        }
        let mut done=None;
        if let Some(worker)=&self.pointcloud_ui.worker {
            loop { match worker.rx.try_recv() {
                Ok(ImportEvent::Progress(message))=>self.pointcloud_ui.status=message,
                Ok(ImportEvent::Done(result))=>{done=Some((worker.generation,result));break;},
                Err(TryRecvError::Empty)=>break,
                Err(TryRecvError::Disconnected)=>{done=Some((worker.generation,Err("De puntenwolkimport is onverwacht gestopt. Het bronbestand is behouden.".into())));break;}
            }}
        }
        if let Some((generation,result))=done {
            self.pointcloud_ui.worker=None;
            if generation==self.project_generation {
                match result {
                    Ok(source)=>{
                        let id=source.id.clone();
                        self.pointcloud_ui.status=format!("{} — {} punten. Gereed om te tekenen.",source.title,source.point_count);
                        if let Some(existing)=self.point_clouds.iter_mut().find(|s|s.id==id){*existing=source;}
                        else{self.point_clouds.push(source);}
                        self.sync_pointcloud_datasets();self.dirty=true;
                        if self.pointcloud_ui.queue.is_empty(){self.open_pointcloud_editor(&id);}
                    }
                    Err(error)=>{self.pointcloud_ui.status=error.clone();self.message=error;}
                }
            }
        }
        if self.pointcloud_ui.busy(){ctx.request_repaint_after(Duration::from_millis(100));}
        if self.pointcloud_ui.form.is_none() && !self.pointcloud_ui.busy() && self.file_intent.is_none() {
            if let Some(path)=self.pointcloud_ui.queue.pop_front() {
                let canonical=path.canonicalize().unwrap_or_else(|_|path.clone());
                if let Some(source)=self.point_clouds.iter().find(|s|s.source_path==canonical) {
                    let id=source.id.clone();self.open_pointcloud_editor(&id);
                } else {
                    let title=path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    self.pointcloud_ui.form=Some(ImportForm{path,title,crs:CloudCrs::Unknown});
                }
            }
        }
    }
    fn start_pointcloud_import(&mut self, form:ImportForm, existing:Option<PointCloudSource>,ctx:&egui::Context) {
        if self.project_sources_owned() || self.file_intent.is_some() || self.export_intent.is_some() || self.template_job.is_some() {
            self.message="Rond de lopende taak eerst af.".into();self.pointcloud_ui.form=Some(form);return;
        }
        let (tx,rx)=mpsc::channel();let cancel=Arc::new(AtomicBool::new(false));let flag=cancel.clone();
        let repaint=ctx.clone();
        self.pointcloud_ui.status="Puntenwolk voorbereiden…".into();
        self.pointcloud_ui.worker=Some(ImportWorker{rx,cancel,generation:self.project_generation});
        std::thread::spawn(move||{
            let progress=|message|{let _=tx.send(ImportEvent::Progress(message));repaint.request_repaint();};
            let result=if let Some(source)=existing {pointcloud::import_existing(&source,&flag,progress)}
                else{pointcloud::import(&form.path,&form.title,form.crs,&flag,progress)};
            let _=tx.send(ImportEvent::Done(result));repaint.request_repaint();
        });
    }
    fn relink_pointcloud(&mut self, id:&str, path:PathBuf,ctx:&egui::Context){
        let Some(mut source)=self.point_clouds.iter().find(|s|s.id==id).cloned()else{return};
        let (tx,rx)=mpsc::channel();let cancel=Arc::new(AtomicBool::new(false));let flag=cancel.clone();
        let repaint=ctx.clone();
        self.pointcloud_ui.status="Het gekozen bestand vergelijken met de oorspronkelijke puntenwolk…".into();
        self.pointcloud_ui.worker=Some(ImportWorker{rx,cancel,generation:self.project_generation});
        std::thread::spawn(move||{
            let result=source.relink_with_cancel(&path,&flag).map(|()|source);
            let _=tx.send(ImportEvent::Done(result));repaint.request_repaint();
        });
    }
    fn open_pointcloud_editor(&mut self,id:&str) {
        if self.pointcloud_ui.busy() || self.project_sources_owned() || self.file_intent.is_some() || self.export_intent.is_some() || self.template_job.is_some(){
            self.message="Rond de lopende taak af en sluit een eventueel geopend puntenwolkvenster.".into();return;
        }
        let Some(source)=self.point_clouds.iter().find(|s|s.id==id).cloned()else{return};
        #[cfg(not(windows))]
        { let _=source;self.pointcloud_ui.status="De ingebouwde puntenwolkeditor vereist Windows en WebView2.".into(); }
        #[cfg(windows)]
        {
            let result=(||{
                if self.browser_ui.host==0{return Err("Het hoofdvenster is nog niet gereed.".to_string());}
                source.validate_cache()?;
                let session=Session::start(source)?;
                let log=tempfile::NamedTempFile::new().map_err(|e|e.to_string())?;
                let stdout=log.reopen().map_err(|e|e.to_string())?;let stderr=log.reopen().map_err(|e|e.to_string())?;
                use std::os::windows::process::CommandExt;
                let child=Command::new(std::env::current_exe().map_err(|e|e.to_string())?)
                    .arg("--pointcloud-webview").arg(&session.url).arg(self.browser_ui.host.to_string())
                    .stdin(Stdio::null()).stdout(stdout).stderr(stderr).creation_flags(0x08000000)
                    .spawn().map_err(|e|format!("Puntenwolkeditor openen mislukt: {e}"))?;
                Ok(Editor{session,child,generation:self.project_generation,source_id:id.into(),revision:0,_log:log})
            })();
            match result {
                Ok(editor)=>{self.pointcloud_ui.editor=Some(editor);self.pointcloud_ui.status="De puntenwolkeditor is geopend. Sluit het venster voordat je het project opslaat of exporteert.".into();}
                Err(error)=>{self.pointcloud_ui.status=error.clone();self.message=error;}
            }
        }
    }
    fn export_pointcloud_objects(&mut self,id:&str) {
        let Some(source)=self.point_clouds.iter().find(|s|s.id==id)else{return};
        if source.annotations.is_empty(){self.pointcloud_ui.status="Teken eerst punten of lijnen in de editor.".into();return;}
        let Some(path)=rfd::FileDialog::new().set_title("Getekende objecten exporteren")
            .add_filter("DXF",&["dxf"]).set_file_name(format!("{}-objecten.dxf",source.title)).save_file()else{return};
        let dataset=Dataset{job_id:0,title:source.title.clone(),source_path:source.source_path.clone(),
            features:source.annotations.iter().map(|a|sleufbase_rust::Feature{
                object_name:a.name.clone(),source_name:a.name.clone(),attributes:a.attributes.clone(),
                geometry:match a.kind{AnnotationKind::Point=>Geometry::Point(a.vertices[0]),AnnotationKind::Polyline=>Geometry::Polyline(a.vertices.clone())},
            }).collect()};
        let result=self.project_export_settings().and_then(|settings|sleufbase_rust::object_dxf::write_with_layers(
            &path,&[dataset],&[],None,&[],&settings.object_rules,settings.object_dxf_options()));
        self.pointcloud_ui.status=match result{Ok(_)=>format!("Objecten geëxporteerd met de originele XYZ-coördinaten: {}",path.display()),Err(error)=>error};
    }
    pub(super) fn show_pointcloud_manager(&mut self,ctx:&egui::Context) {
        if !self.pointcloud_ui.open { return; }
        let mut open=true;let mut action=None::<(String,String)>;let mut choose=false;
        let enabled=!self.pointcloud_ui.busy() && !self.project_exchange_busy() && !self.overview_ui.busy() && self.file_intent.is_none() && self.export_intent.is_none() && self.template_job.is_none();
        let mut changed=false;let mut validation_error=None;
        egui::Window::new("Puntenwolken · LAS / LAZ").id(egui::Id::new("pointcloud-manager"))
            .open(&mut open).default_width(760.).default_height(440.).min_width(480.).show(ctx,|ui|{
            ui.label("Open een scan, teken punten en lijnen en gebruik de meetobjecten in je proefsleuven.");
            ui.horizontal_wrapped(|ui|{
                if ui.add_enabled(enabled,egui::Button::new("LAS/LAZ openen…").fill(ORANGE)).clicked(){choose=true;}
                if self.pointcloud_ui.worker.is_some(){ui.spinner();
                    if ui.button("Import annuleren").clicked(){if let Some(w)=&self.pointcloud_ui.worker{w.cancel.store(true,Ordering::Release);}}
                }
            });
            if !self.pointcloud_ui.status.is_empty(){ui.add(egui::Label::new(&self.pointcloud_ui.status).wrap());}
            ui.separator();
            if self.point_clouds.is_empty(){ui.label("Nog geen puntenwolken in dit project.");}
            egui::ScrollArea::vertical().max_height(460.).show(ui,|ui|{
                for source in &mut self.point_clouds {
                    ui.push_id(source.id.clone(),|ui|{
                        egui::Frame::new().fill(theme::SURFACE_ALT).inner_margin(12).corner_radius(8).show(ui,|ui|{
                            ui.add_enabled_ui(enabled,|ui|{
                                let title=self.pointcloud_ui.titles.entry(source.id.clone()).or_insert_with(||source.title.clone());
                                let response=ui.add(egui::TextEdit::singleline(title).char_limit(1024).desired_width(f32::INFINITY));
                                let title_valid=!title.trim().is_empty()&&title.len()<=1024&&!title.chars().any(char::is_control);
                                if response.changed()&&title_valid{source.title=title.clone();changed=true;}
                                if !title_valid{
                                    ui.small("Vul een geldige naam in. Tot die tijd blijft de vorige naam bewaard.");
                                    if response.lost_focus(){*title=source.title.clone();}
                                }
                                ui.small(format!("{} scanpunten · {} tekenobjecten",source.point_count,source.annotations.len()));
                                ui.label(source.source_path.display().to_string()).on_hover_text("Originele puntenwolk");
                                let previous_crs=source.crs;
                                if crs_combo(ui,"source-crs",&mut source.crs){
                                    match source.validate(){
                                        Ok(())=>changed=true,
                                        Err(error)=>{source.crs=previous_crs;validation_error=Some(error);}
                                    }
                                }
                                if !source.projection.is_empty(){ui.small(format!("Broninformatie: {}",source.projection.chars().take(160).collect::<String>()));}
                                if !matches!(source.crs,CloudCrs::RdNew){ui.small("Je kunt tekenen en DXF exporteren. Overnemen op de kaart en naar een proefsleuf vereist RD-coördinaten in meters.");}
                                ui.horizontal_wrapped(|ui|{
                                    for (label,key) in [("Open editor","edit"),("Maak proefsleuf…","trench"),("Objecten naar DXF…","export"),("Bronbestand opnieuw kiezen…","relink"),("Weergave opnieuw opbouwen","rebuild"),("Verwijder uit project","remove")] {
                                        if ui.button(label).clicked(){action=Some((key.into(),source.id.clone()));}
                                    }
                                });
                            });
                        });ui.add_space(8.);
                    });
                }
            });
            ui.small("Tekenobjecten worden in het SleufBase-project bewaard. Bewaar de oorspronkelijke LAS/LAZ mee wanneer je het project naar een andere computer verplaatst.");
        });
        self.pointcloud_ui.open=open;
        if let Some(error)=validation_error{self.pointcloud_ui.status=error;}
        if changed{self.sync_pointcloud_datasets();self.dirty=true;}
        if choose{self.choose_pointcloud_files();}
        if let Some((action,id))=action{match action.as_str(){
            "edit"=>self.open_pointcloud_editor(&id),
            "export"=>self.export_pointcloud_objects(&id),
            "remove"=>{self.point_clouds.retain(|s|s.id!=id);self.sync_pointcloud_datasets();self.dirty=true;},
            "trench"=>{
                if let Some(source)=self.point_clouds.iter().find(|s|s.id==id){
                    let axis=source.annotations.iter().find(|a|matches!(a.kind,AnnotationKind::Polyline)&&a.vertices.len()==2).map(|a|a.id.clone()).unwrap_or_default();
                    self.pointcloud_ui.trench=Some(TrenchForm{source_id:id,axis_id:axis.clone(),
                        selected_ids:source.annotations.iter().filter(|a|a.id!=axis).map(|a|a.id.clone()).collect(),label:source.title.clone(),width:0.6});
                }
            },
            "relink"=>{
                if let Some(path)=rfd::FileDialog::new().set_title("Hetzelfde LAS/LAZ-bronbestand aanwijzen").add_filter("Puntenwolk",&["las","laz"]).pick_file(){
                    self.relink_pointcloud(&id,path,ctx);
                }
            },
            "rebuild"=>{if let Some(source)=self.point_clouds.iter().find(|s|s.id==id).cloned(){
                self.start_pointcloud_import(ImportForm{path:source.source_path.clone(),title:source.title.clone(),crs:source.crs.clone()},Some(source),ctx);
            }},_=>{}
        }}
        self.show_pointcloud_import(ctx);
        self.show_pointcloud_trench(ctx);
    }
    fn show_pointcloud_import(&mut self,ctx:&egui::Context){
        let Some(mut form)=self.pointcloud_ui.form.take()else{return};
        let mut create=false;let mut cancel=false;
        let response=egui::Modal::new(egui::Id::new("pointcloud-import")).show(ctx,|ui|{
            ui.heading("Puntenwolk importeren");ui.label(form.path.display().to_string());
            ui.label("Naam");ui.text_edit_singleline(&mut form.title);
            ui.label("Coördinatenstelsel");crs_combo(ui,"import-crs",&mut form.crs);
            ui.label("Kies RD alleen als de bron daadwerkelijk RD New in meters gebruikt. De originele XYZ-coördinaten blijven behouden.");
            ui.horizontal(|ui|{
                if ui.add_enabled(!form.title.trim().is_empty(),egui::Button::new("Openen en voorbereiden").fill(ORANGE)).clicked(){create=true;}
                if ui.button("Annuleren").clicked(){cancel=true;}
            });
        });
        if create{self.start_pointcloud_import(form,None,ctx);}
        else if !cancel&&!response.should_close(){self.pointcloud_ui.form=Some(form);}
    }
    fn show_pointcloud_trench(&mut self,ctx:&egui::Context){
        let Some(mut form)=self.pointcloud_ui.trench.take()else{return};
        let Some(source)=self.point_clouds.iter().find(|s|s.id==form.source_id)else{return};
        let mut create=false;let mut cancel=false;
        let response=egui::Modal::new(egui::Id::new("pointcloud-trench")).show(ctx,|ui|{
            ui.set_max_width(640.);ui.heading("Maak proefsleuf van meetobjecten");
            ui.label("Kies een rechte lijn met twee punten als sleufas. De hoogtes van de eindpunten vormen het maaiveld; teken deze op het maaiveld en controleer ze in het bewerkscherm.");
            ui.label("Naam proefsleuf");ui.text_edit_singleline(&mut form.label);
            let selected=source.annotations.iter().find(|a|a.id==form.axis_id).map_or("Kies een sleufas",|a|a.name.as_str());
            egui::ComboBox::from_id_salt("cloud-trench-axis").selected_text(selected).show_ui(ui,|ui|{
                for a in source.annotations.iter().filter(|a|matches!(a.kind,AnnotationKind::Polyline)&&a.vertices.len()==2){
                    ui.selectable_value(&mut form.axis_id,a.id.clone(),&a.name);
                }
            });
            ui.horizontal(|ui|{ui.label("Sleufbreedte (m)");ui.add(egui::DragValue::new(&mut form.width).range(0.1..=100.).speed(0.05));});
            ui.label("Kabels/leidingen om over te nemen");
            egui::ScrollArea::vertical().max_height(210.).show(ui,|ui|{
                for a in source.annotations.iter().filter(|a|a.id!=form.axis_id){
                    let mut selected=form.selected_ids.contains(&a.id);
                    if ui.checkbox(&mut selected,format!("{} ({})",a.name,if matches!(a.kind,AnnotationKind::Point){"punt"}else{"lijn"})).changed(){
                        form.selected_ids.retain(|id|id!=&a.id);if selected{form.selected_ids.push(a.id.clone());}
                    }
                }
            });
            ui.label("Punten binnen de sleuf worden overgenomen. Van lijnen worden alleen de echte kruisingen met de sleufas overgenomen, met geïnterpoleerde hoogte. De volledige lijnen blijven in de puntenwolk bewaard.");
            let preview=sleufbase_rust::pointcloud_trench::prepare(source,&form.axis_id,&form.selected_ids,form.label.clone(),form.width)
                .and_then(|prepared|{
                    if self.virtual_trenches.iter().any(|t|t.label==prepared.trench.label){Err("Er bestaat al een proefsleuf met deze naam. Kies een andere naam.".into())}
                    else{Ok(prepared)}
                });
            match &preview {
                Ok(p)=>{ui.label(format!("{} kabelpunten; {} geselecteerde objecten liggen buiten de sleuf of kruisen de as niet.",p.trench.objects.len(),p.skipped));},
                Err(error)=>{ui.colored_label(Color32::from_rgb(160,60,30),error);}
            }
            ui.horizontal(|ui|{
                if ui.add_enabled(preview.is_ok()&&!self.pointcloud_ui.busy(),egui::Button::new("Proefsleuf maken en bewerken").fill(ORANGE)).clicked(){create=true;}
                if ui.button("Annuleren").clicked(){cancel=true;}
            });
        });
        if create {
            match sleufbase_rust::pointcloud_trench::prepare(source,&form.axis_id,&form.selected_ids,form.label.clone(),form.width){
                Ok(prepared)=>{
                    if self.virtual_trenches.iter().any(|t|t.label==prepared.trench.label){
                        self.pointcloud_ui.status="Er bestaat al een proefsleuf met deze naam. Kies een andere naam.".into();self.pointcloud_ui.trench=Some(form);return;
                    }
                    let index=self.virtual_trenches.len();self.virtual_trenches.push(prepared.trench);
                    self.trench_markers=self.virtual_trenches.iter().map(sleufbase_rust::cross_section::markers).collect();
                    self.refresh_virtual_references();self.virtual_map.selected=Some(index);self.dirty=true;
                    self.pointcloud_ui.status="Proefsleuf aangemaakt. Controleer de kabelgegevens en maaiveldhoogtes; daarna is de sleuf beschikbaar in de sjabloonexport.".into();
                    self.pointcloud_ui.open=false;self.open_virtual_editor(index);
                },Err(error)=>{self.pointcloud_ui.status=error;self.pointcloud_ui.trench=Some(form);}
            }
        }else if !cancel&&!response.should_close(){self.pointcloud_ui.trench=Some(form);}
    }
}
