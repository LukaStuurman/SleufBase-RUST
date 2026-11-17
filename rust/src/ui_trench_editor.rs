//! Native editor for virtual trenches and MarXact measurements.
//! All edits are staged: the current project changes only after validation.
use super::*;
use sleufbase_rust::{
    virtual_trench::{TrenchPoint,VirtualTrench},
    virtual_trench_editor as edit,
};
use egui::Vec2;
#[path="ui_trench_ahn.rs"]
mod ahn_fill;
#[path="ui_trench_object.rs"]
mod object_ui;
#[path="ui_trench_section.rs"]
mod section;
#[path="ui_trench_workspace.rs"]
mod workspace;

#[derive(Clone,PartialEq)]
struct ObjectForm {
    role:String,
    name:String,
    source:String,
    attributes:[String;3],
    chainage:f64,
    offset:f64,
    elevation:f64,
    has_elevation:bool,
    from_end:bool,
    diameter:f64,
    quantity:u32,
    angle:f64,
    rgb:Option<[u8;3]>,
    use_depth:bool,
    depth:f64,
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    #[test]
    fn clearing_source_references_and_notes_cannot_be_undone_by_saving_an_open_draft(){
        let mut t=VirtualTrench::new("Test".into(),Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap();
        t.metadata=serde_json::json!({"template_reference_point":{"x":155001.,"y":463001.},"tiff_comment_text":"Note","keep":"Ø"});
        let mut app=SleufBaseApp{virtual_trenches:vec![t],..Default::default()};app.open_virtual_editor(0);
        app.trench_editor.as_mut().unwrap().add_or_replace().unwrap();
        app.clear_raster_references();app.clear_map_comments();
        let next=app.trench_editor.take().unwrap().build().unwrap();
        assert_eq!(next.objects.len(),1);assert_eq!(next.metadata["keep"],"Ø");assert!(next.metadata.get("template_reference_point").is_none());assert!(next.metadata.get("tiff_comment_text").is_none());
    }
}
impl Default for ObjectForm {
    fn default()->Self {
        Self {
            role:"kabel".into(),name:"LS".into(),source:"LS".into(),
            attributes:Default::default(),
            chainage:0.,offset:0.,elevation:0.,has_elevation:false,
            from_end:false,diameter:0.,quantity:1,angle:0.,rgb:None,use_depth:false,depth:0.7,
        }
    }
}
fn object_quantity(object:&TrenchPoint)->u32{object.metadata.get("quantity").and_then(serde_json::Value::as_u64).and_then(|n|u32::try_from(n).ok()).unwrap_or_else(||object.attributes[2].split(|c:char|!c.is_ascii_digit()).find_map(|v|v.parse::<u32>().ok()).filter(|n|*n>=1&&*n<=1000).unwrap_or_else(||if object.attributes[2].to_lowercase().contains("dubbel"){2}else{1}))}
fn object_diameter(object:&TrenchPoint)->f64{
    sleufbase_rust::profile::diameter_mm(&sleufbase_rust::Feature{
        object_name:String::new(),source_name:String::new(),
        geometry:sleufbase_rust::Geometry::Point(object.position),
        attributes:object.attributes.clone(),
    }).filter(|d|d.is_finite()&&*d>=0.).unwrap_or(0.)
}
// Use the active type palette for imported objects without a fixed color.
// A per-object color keeps precedence; ACI 7 follows the entry form's dark
// contrast color so it remains visible on the light editor background.
fn utility_color(source:&str,rgb:Option<[u8;3]>,rules:&[sleufbase_rust::Rule])->Color32 {
    let [r,g,b]=rgb.or_else(||sleufbase_rust::matching_rule(source,rules).map(|rule| {
        if rule.color_aci==7 {[17,24,39]}
        else {sleufbase_rust::cad::aci_rgb(i32::from(rule.color_aci))}
    })).unwrap_or([37,99,235]);
    Color32::from_rgb(r,g,b)
}
impl ObjectForm {
    fn from_object(trench:&VirtualTrench,object:&TrenchPoint)->Self {
        let (chainage,offset)=edit::coordinates(trench,object.position);
        Self {
            role:object.role.clone(),name:object.object_name.clone(),
            source:object.source_name.clone(),
            attributes:object.attributes.clone(),chainage,offset,
            elevation:object.position.z.unwrap_or(0.),
            has_elevation:object.position.z.is_some(),
            from_end:false,diameter:object_diameter(object),
            quantity:object_quantity(object),
            angle:object.metadata.get("crossing_angle_degrees").and_then(serde_json::Value::as_f64).unwrap_or(0.),
            rgb:sleufbase_rust::virtual_raster::display_rgb(&object.metadata),use_depth:object.metadata.get("surface_mode").and_then(serde_json::Value::as_str)==Some("start")&&object.metadata.get("sleufbase_depth_meters").and_then(serde_json::Value::as_f64).is_some_and(|v|v.is_finite()&&v>=0.),
            depth:object.metadata.get("sleufbase_depth_meters").and_then(serde_json::Value::as_f64).unwrap_or(0.7),
        }
    }
    /// Reuse only editable cable properties, never coordinates, absolute Z or
    /// the source object's measurement/reference metadata.
    fn for_new_entry(&self)->Self {
        Self {role:self.role.clone(),name:self.name.clone(),source:self.source.clone(),
            attributes:self.attributes.clone(),diameter:self.diameter,quantity:self.quantity,
            angle:self.angle,rgb:self.rgb,use_depth:self.use_depth,depth:self.depth,
            ..Default::default()}
    }
    fn has_pending_entry(&self)->bool{let mut empty=Self::default();empty.from_end=self.from_end;self!=&empty}
    fn distance(&self,length:f64)->f64{if self.from_end{length-self.chainage}else{self.chainage}}
    fn set_distance(&mut self,length:f64,distance:f64)->Result<(),String>{
        if !length.is_finite()||length<=0.||!distance.is_finite()||distance<0.||distance>length{return Err("De afstand moet binnen de sleuf vallen".into());}
        self.chainage=if self.from_end{length-distance}else{distance};Ok(())
    }
    fn update_quantity_attribute(&mut self){
        let re=regex::Regex::new(r"(?i)\bdubbel\b").unwrap();
        let original=re.replace_all(&self.attributes[2],"");
        let number=regex::Regex::new(r"[0-9]+(?:[.,][0-9]+)?\s*[xX]?").unwrap();
        let value=if self.quantity>1{self.quantity.to_string()}else{String::new()};
        self.attributes[2]=if number.is_match(&original){number.replacen(&original,1,value.as_str()).trim().to_owned()}else{format!("{value} {}",original.trim()).trim().to_owned()};
    }
    fn to_object(&self,trench:&VirtualTrench)->Result<TrenchPoint,String> {
        if !self.angle.is_finite()||self.angle.abs()>80.||self.quantity==0||self.quantity>1000||!self.diameter.is_finite()||!(0.0..=10000.).contains(&self.diameter){return Err("Controleer diameter, aantal en kabelhoek".into());}
        if self.use_depth&&(!self.depth.is_finite()||!(0.0..=1000.).contains(&self.depth)){return Err("Controleer de diepte in meters".into());}
        let mut metadata=serde_json::json!({"crossing_angle_degrees":self.angle,"quantity":self.quantity});
        if let Some(rgb)=self.rgb{metadata["display_rgb"]=serde_json::json!(rgb);}
        if self.use_depth{metadata["surface_mode"]=serde_json::json!("start");metadata["surface_status"]=serde_json::json!("start");metadata["surface_z"]=serde_json::json!(trench.start.z);metadata["sleufbase_depth_meters"]=serde_json::json!(self.depth);}
        let z=if self.use_depth{trench.start.z.map(|z|z-self.depth)}else{self.has_elevation.then_some(self.elevation)};
        Ok(TrenchPoint {metadata,
            role:self.role.clone(),object_name:self.name.clone(),
            source_name:self.source.clone(),
            attributes:self.attributes.clone(),
            position:edit::position(trench,self.chainage,self.offset,
                z)?,
        })
    }
}

pub(super) struct Editor {
    entry_settings:Option<std::sync::Arc<MigratedSettings>>,
    ahn_job:Option<ahn_fill::Job>,
    pub index:usize,
    original:VirtualTrench,
    draft:VirtualTrench,
    label:String,
    start:Vertex,
    end:Vertex,
    width:f64,
    selected:Option<usize>,
    section_drag:Option<section::Drag>,
    jump_to_objects:Option<bool>,
    objects_tab:bool,
    pub(super) picking_object:bool,
    entry_started:bool,
    object:ObjectForm,
    prepared_entry:Option<ObjectForm>,
    last_entry:Option<ObjectForm>,
    confirm_delete:bool,
    error:String,
    paste:String,
    notice:String,
    shift_x:f64,
    shift_y:f64,
    dekband_start:f64,
    dekband_end:f64,
    dekband_name:String,
    dekband_rgb:[u8;3],
    dekband_start_depth:f64,
    dekband_end_depth:f64,
}
#[derive(Clone, Copy)]
enum Action {Save,Copy,Remove}

impl Editor {
    fn new(index:usize,source:&VirtualTrench)->Self {
        Self{
            entry_settings:None,
            ahn_job:None,
            index,original:source.clone(),draft:source.clone(),
            label:source.label.clone(),start:source.start,
            end:source.end,width:source.width_meters,
            selected:None,section_drag:None,jump_to_objects:Some(true),objects_tab:true,picking_object:false,entry_started:false,object:ObjectForm::default(),prepared_entry:None,last_entry:None,
            confirm_delete:false,error:String::new(),paste:String::new(),notice:String::new(),shift_x:0.,shift_y:0.,
            dekband_start:0.,dekband_end:source.length(),
            dekband_name:"Dekband".into(),dekband_rgb:[17,24,39],
            dekband_start_depth:0.,dekband_end_depth:0.,
        }
    }
    fn build_preserving_order(&self)->Result<VirtualTrench,String> {
        let mut result=self.draft.clone();
        edit::edit_axis_preserving_order(&mut result,self.label.clone(),self.start,self.end,self.width)?;
        edit::check(&result)?;
        Ok(result)
    }
    fn build(&self)->Result<VirtualTrench,String> {
        let mut result=self.build_preserving_order()?;
        edit::sort_objects(&mut result);
        Ok(result)
    }
    fn add_or_replace(&mut self)->Result<(),String> {
        let mut next=self.draft.clone();
        edit::edit_axis_preserving_order(&mut next,self.label.clone(),self.start,self.end,self.width)?;
        let mut object=self.object.to_object(&next)?;
        if let Some(old)=self.selected.and_then(|i|self.draft.objects.get(i)){
            let typed=object.metadata.take();
            object.metadata=old.metadata.clone();
            if !object.metadata.is_object(){object.metadata=serde_json::json!({});}
            if let Some(typed)=typed.as_object(){object.metadata.as_object_mut().unwrap().extend(typed.clone());}
            if !self.object.use_depth && (object.position.z!=old.position.z || (old.metadata.get("surface_mode").and_then(serde_json::Value::as_str)==Some("start")&&old.metadata.get("sleufbase_depth_meters").is_some())) {
                let meta=object.metadata.as_object_mut();
                if let Some(meta)=meta {
                    meta.remove("sleufbase_depth_meters");
                    meta.insert("surface_mode".into(),serde_json::json!("manual"));
                    meta.insert("surface_status".into(),serde_json::json!("manual"));
                }
            }
        }
        if let Some(index)=self.selected {
            next.objects[index]=object;
        }else{
            next.objects.push(object);
        }
        edit::check(&next)?;
        self.draft=next;
        self.last_entry=Some(self.object.for_new_entry());
        self.prepared_entry=None;
        self.selected=None;
        self.object=ObjectForm::default();self.entry_started=false;
        Ok(())
    }
    /// Commit the active form before copying, importing, or leaving the tab.
    /// An unchanged selected object must not become a second new entry.
    fn finish_entry(&mut self)->Result<(),String> {
        if self.entry_changed(){self.add_or_replace()?;}
        self.selected=None;
        self.object=ObjectForm::default();
        self.prepared_entry=None;
        self.entry_started=false;
        self.section_drag=None;
        Ok(())
    }
    fn completed_draft(&mut self)->Result<VirtualTrench,String> {
        self.finish_entry()?;
        self.build()
    }
    fn entry_changed(&self)->bool {
        if let Some(index)=self.selected {
            let mut original=ObjectForm::from_object(&self.draft,&self.draft.objects[index]);
            original.from_end=self.object.from_end;
            self.object!=original
        }else if let Some(template)=&self.prepared_entry {
            let mut baseline=template.clone();baseline.from_end=self.object.from_end;
            self.entry_started||self.object!=baseline
        }else{
            self.entry_started||self.object.has_pending_entry()
        }
    }
    fn add_and_repeat(&mut self)->Result<(),String> {
        // An unchanged selected measurement must keep its original metadata.
        // A new entry is explicit, but an unused repeat template is not a cable.
        if self.selected.is_none() {
            if self.prepared_entry.is_some()&&!self.entry_changed(){
                return Err("Kies eerst een positie voor de volgende kabel of leiding".into());
            }
            self.add_or_replace()?;
        }
        self.prepare_similar_entry()
    }
    fn reset_entry(&mut self) {
        self.object=self.selected.and_then(|index|self.draft.objects.get(index))
            .map(|object|ObjectForm::from_object(&self.draft,object)).unwrap_or_default();
        self.prepared_entry=None;
        self.entry_started=false;
        self.section_drag=None;
        self.error.clear();
        self.notice=if self.selected.is_some(){
            "De invoer van deze kabel of leiding is hersteld. Eerdere conceptwijzigingen blijven bewaard."
        }else{
            "Invoer gewist. Klik in de doorsnede voor een nieuwe kabel of leiding."
        }.into();
    }
    fn prepare_similar_entry(&mut self)->Result<(),String> {
        let template=if self.selected.is_some()||self.entry_changed(){
            self.object.for_new_entry()
        }else{
            self.prepared_entry.clone().or_else(||self.last_entry.clone()).ok_or("Selecteer eerst een kabel of voeg er een toe")?
        };
        self.finish_entry()?;
        self.object=template.clone();
        self.prepared_entry=Some(template);
        self.notice="Eigenschappen overgenomen. Klik een nieuwe plek in de doorsnede of vul een andere positie in.".into();
        self.error.clear();
        Ok(())
    }
    fn select_object(&mut self,index:usize)->bool {
        if self.selected==Some(index){return true;}
        if self.entry_changed(){
            if let Err(error)=self.add_or_replace(){self.error=error;return false;}
        }
        self.selected=Some(index);
        self.prepared_entry=None;
        self.object=ObjectForm::from_object(&self.draft,&self.draft.objects[index]);
        self.entry_started=false;
        self.error.clear();
        self.notice.clear();
        true
    }
    fn sync_axis(&mut self)->Result<(),String>{
        if self.draft.start!=self.start||self.draft.end!=self.end{
            self.draft=self.build()?;
        }
        Ok(())
    }
    pub(super) fn pick_object_position(&mut self,world:[f64;2])->Result<(),String>{
        if !world.iter().all(|v|v.is_finite()){return Err("Ongeldige kaartpositie".into());}
        let dx=self.end.x-self.start.x;let dy=self.end.y-self.start.y;let length=dx.hypot(dy);
        if length<=1e-9{return Err("Stel eerst verschillende begin- en eindpunten in".into());}
        self.object.chainage=(((world[0]-self.start.x)*dx+(world[1]-self.start.y)*dy)/length).clamp(0.,length);
        self.object.offset=0.;self.object.from_end=false;self.entry_started=true;self.picking_object=false;self.jump_to_objects=Some(true);
        self.notice="Positie gekozen. Kies type, diameter en aantal en klik op Kabel/leiding toevoegen.".into();Ok(())
    }
    fn draw_preview(&self,ui:&mut egui::Ui,rules:&[sleufbase_rust::Rule]) {
        // Never clone/validate thousands of measured objects per animation
        // frame. The project stays untouched until the Save action.
        let t=&self.draft;
        let start=self.start;
        let end=self.end;
        let width=self.width;
        let len=(end.x-start.x).hypot(end.y-start.y);
        let outline=if edit::is_measured(t) {
            t.measured_boundary()
        } else {
            let denom=len.max(1e-9);
            let nx=-(end.y-start.y)/denom*width.max(0.1)*0.5;
            let ny=(end.x-start.x)/denom*width.max(0.1)*0.5;
            let mut points=vec![
                (start.x+nx,start.y+ny),(start.x-nx,start.y-ny),
                (end.x-nx,end.y-ny),(end.x+nx,end.y+ny)
            ];
            points.push(points[0]);
            points
        };
        let (rect,_response)=ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(1.),190.),egui::Sense::hover()
        );
        let painter=ui.painter_at(rect);
        painter.rect_filled(rect,6.,Color32::from_rgb(250,250,250));
        let stride=(t.objects.len()/2048).max(1);
        let sampled=t.objects.iter().step_by(stride).collect::<Vec<_>>();
        let mut xs=vec![start.x,end.x];
        let mut ys=vec![start.y,end.y];
        for &(x,y) in &outline {xs.push(x);ys.push(y);}
        for obj in &sampled {xs.push(obj.position.x);ys.push(obj.position.y);}
        let min_x=xs.iter().copied().fold(f64::INFINITY,f64::min);
        let max_x=xs.iter().copied().fold(f64::NEG_INFINITY,f64::max);
        let min_y=ys.iter().copied().fold(f64::INFINITY,f64::min);
        let max_y=ys.iter().copied().fold(f64::NEG_INFINITY,f64::max);
        let w=(max_x-min_x).max(1.0);
        let h=(max_y-min_y).max(1.0);
        let scale=((rect.width()-50.) as f64/w).min((rect.height()-42.) as f64/h);
        let center_x=(min_x+max_x)*0.5;
        let center_y=(min_y+max_y)*0.5;
        let center=rect.center();
        let point=|x:f64,y:f64|Pos2::new(
            center.x+((x-center_x)*scale) as f32,
            center.y-((y-center_y)*scale) as f32
        );
        for line in outline.windows(2) {
            painter.line_segment([point(line[0].0,line[0].1),point(line[1].0,line[1].1)],
                Stroke::new(1_f32,Color32::BLACK));
        }
        if len>1e-9 {
            let stride=(t.dekband_rows.len()/1024).max(1);
            for band in t.dekband_rows.iter().step_by(stride) {
                let start_m=band.start_chainage.clamp(0.,len);
                let end_m=band.end_chainage.clamp(0.,len);
                if end_m<=start_m {continue;}
                let at=|m:f64|{
                    let f=m/len;
                    point(start.x+(end.x-start.x)*f,start.y+(end.y-start.y)*f)
                };
                let c=band.display_rgb.unwrap_or([17,24,39]);
                painter.line_segment([at(start_m),at(end_m)],
                    Stroke::new(3_f32,Color32::from_rgb(c[0],c[1],c[2])));
            }
        }
        painter.circle_filled(point(start.x,start.y),4.,Color32::GREEN);
        painter.circle_filled(point(end.x,end.y),4.,Color32::RED);
        for obj in sampled {
            if obj.is_measurement_helper(){continue;}
            if let Some((a,b))=sleufbase_rust::cross_section::object_segment_on_boundary(t,obj,&outline){
                let color=utility_color(&obj.source_name,sleufbase_rust::virtual_raster::display_rgb(&obj.metadata),rules);
                painter.line_segment([point(a.0,a.1),point(b.0,b.1)],Stroke::new(2_f32,color));
            }
            painter.circle_filled(point(obj.position.x,obj.position.y),3.,INK);
        }
        if let Some(i)=self.selected {
            if let Some(obj)=t.objects.get(i) {
                painter.circle_filled(point(obj.position.x,obj.position.y),5.,ORANGE);
            }
        }
        painter.text(rect.left_top()+Vec2::new(8.,6.),egui::Align2::LEFT_TOP,
            format!("Bovenaanzicht | lengte {:.2} m | breedte {:.2} m | {} objecten",
                len,width,t.objects.len()),
            egui::FontId::proportional(11.),INK);
    }
}
impl SleufBaseApp {
    pub(super) fn clear_virtual_editor_metadata(&mut self,key:&str,index:Option<usize>){
        if let Some(editor)=self.trench_editor.as_mut().filter(|e|index.is_none_or(|i|i==e.index)){
            for t in [&mut editor.original,&mut editor.draft]{if let Some(m)=t.metadata.as_object_mut(){m.remove(key);}}
        }
    }
    pub(super) fn refresh_virtual_editor_settings(&mut self) {
        if self.trench_editor.is_none() {return;}
        // Refresh only after an actual settings save. Keep the staged geometry
        // and object form intact, and preserve project/legacy rule precedence.
        let settings=self.project_export_settings();
        if let Some(editor)=self.trench_editor.as_mut() {
            match settings {
                Ok(settings)=>editor.entry_settings=Some(std::sync::Arc::new(settings)),
                Err(error)=>editor.error=format!("Controleer de projectinstellingen: {error}"),
            }
        }
    }
    pub(super) fn open_virtual_editor(&mut self,index:usize) {
        self.cancel_virtual_map();
        if self.export_intent.is_some() || self.template_job.is_some()
            || self.file_intent.is_some() {
            self.message="Rond de lopende laad- of exporttaak eerst af".into();
            return;
        }
        if let Some(t)=self.virtual_trenches.get(index) {
            match self.project_export_settings() {
                Ok(settings)=>{
                    let mut editor=Editor::new(index,t);
                    editor.entry_settings=Some(std::sync::Arc::new(settings));
                    self.trench_editor=Some(editor);
                },
                Err(error)=>self.message=format!("Controleer de projectinstellingen: {error}"),
            }
        }
    }
    pub(super) fn show_virtual_editor(&mut self,ctx:&egui::Context) {
        let Some(mut editor)=self.trench_editor.take() else{return};
        if editor.index>=self.virtual_trenches.len() {return;}
        editor.poll_ahn();
        // Resolve project (including legacy) choices once when opening; sharing
        // this snapshot avoids cloning every rule during each UI frame.
        let entry_settings=editor.entry_settings.clone();
        let settings=entry_settings.as_deref().unwrap_or(&self.migrated_settings);
        if editor.picking_object {
            let mut cancel=false;
            egui::Window::new("Kabel/leiding op de kaart plaatsen").collapsible(false).resizable(false)
                .fixed_pos(egui::pos2(20.,45.)).show(ctx,|ui|{ui.label("Klik een positie op de sleuf. Daarna kies je het type en de afmetingen.");cancel=ui.button("Annuleer puntplaatsing").clicked();});
            if cancel{editor.picking_object=false;self.cancel_virtual_map();}
            self.trench_editor=Some(editor);return;
        }
        let reset_scroll=if let Some(tab)=editor.jump_to_objects.take(){editor.objects_tab=tab;true}else{false};
        let mut pick_requested=false;
        let mut close_requested=false;
        let mut open=true;
        let mut action=None;
        let viewport=ctx.screen_rect();
        let maximum=egui::vec2((viewport.width()-64.).max(280.),(viewport.height()-100.).max(200.));
        egui::Window::new("Proefsleuf bewerken").id(egui::Id::new("virtual-trench-editor"))
            .open(&mut open).collapsible(false)
            // Only seed the first position; remembered moves and screen constraints remain active.
            .default_pos(viewport.min+egui::vec2(24.,24.))
            .default_size(egui::vec2(1020.,650.).min(maximum))
            .min_size(egui::vec2(320.,240.).min(maximum))
            .max_size(maximum)
            .constrain_to(viewport.shrink(12.))
            .resizable(true).show(ctx,|ui|theme::bounded_window_body(ui,|ui|{
                settings_ui::control_style(ui);
                ui.set_max_width(ui.available_width());
                ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT),|ui|{
                    // Lay out the real footer first; wrapping, notices and delete
                    // confirmation now reserve their actual height in this frame.
                    ui.add_enabled_ui(editor.ahn_job.is_none(),|ui|{
                        if editor.confirm_delete {
                            ui.horizontal(|ui|{
                                if ui.button("Ja, verwijderen").clicked(){action=Some(Action::Remove);}
                                if ui.button("Annuleren").clicked(){editor.confirm_delete=false;}
                            });
                            ui.colored_label(DARK,"Alleen deze proefsleuf verwijderen? Alle meetobjecten ervan worden ook verwijderd.");
                        }
                        ui.horizontal_wrapped(|ui|{
                            if ui.add(egui::Button::new(egui::RichText::new("Opslaan in project").color(Color32::WHITE)).fill(ORANGE)).clicked() {action=Some(Action::Save);}
                            if ui.button("Sluiten zonder opslaan").clicked(){close_requested=true;}
                        });
                        if editor.objects_tab{editor.show_entry_actions(ui);}
                    });
                    if !editor.error.is_empty()||!editor.notice.is_empty() {
                        let feedback_height=44_f32.min(ui.available_height().max(0.));
                        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(),feedback_height),
                            egui::Layout::top_down(egui::Align::LEFT),|ui|{
                            egui::ScrollArea::vertical().id_salt("virtual-editor-feedback")
                                .max_height(feedback_height).min_scrolled_height(0.).auto_shrink([false,false])
                                .show(ui,|ui|{
                                    if !editor.error.is_empty(){ui.colored_label(DARK,&editor.error);}
                                    else{ui.small(&editor.notice);}
                                });
                        });
                    }
                    ui.separator();
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT),|ui|{
                theme::card(10,12,8).show(ui,|ui|{
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui|{
                        ui.heading(&editor.label);
                        ui.weak(format!("{:.2} m lang  ·  {:.2} m breed  ·  {} kabels / leidingen",
                            (editor.end.x-editor.start.x).hypot(editor.end.y-editor.start.y),editor.width,editor.draft.objects.len()).replace('.',","));
                    });
                });
                ui.horizontal_wrapped(|ui|{
                    if ui.selectable_label(editor.objects_tab,"Doorsnede").clicked()&&!editor.objects_tab{
                        match editor.sync_axis(){
                            Ok(())=>editor.jump_to_objects=Some(true),
                            Err(error)=>editor.error=error,
                        }
                    }
                    if ui.selectable_label(!editor.objects_tab,"Sleufgegevens").clicked()&&editor.objects_tab{
                        let result=editor.finish_entry();
                        match result{
                            Ok(())=>editor.jump_to_objects=Some(false),
                            Err(error)=>editor.error=error,
                        }
                    }
                    ui.weak("Plaatsen en bewerken · wijzigingen blijven in concept tot je opslaat");
                });
                let scroll=egui::ScrollArea::vertical().id_salt("virtual-editor-body").animated(false)
                    .max_height(ui.available_height().max(0.)).min_scrolled_height(0.).auto_shrink([false,false]);
                let scroll=if reset_scroll{scroll.vertical_scroll_offset(0.)}else{scroll};
                scroll.show(ui,|ui|{
                    if !editor.objects_tab||editor.ahn_job.is_some(){editor.ahn_controls(ui,ctx);}
                    ui.add_enabled_ui(editor.ahn_job.is_none(),|ui|{
                    let measured=edit::is_measured(&editor.original);
                    if !editor.objects_tab{
                    ui.heading("Proefsleufgegevens");
                    ui.horizontal(|ui|{
                        ui.label("Naam");
                        ui.text_edit_singleline(&mut editor.label);
                        ui.label(format!("Job {}",editor.original.job_id));
                    });
                    if measured {
                        ui.weak("MarXact: originele 3D-meetgrenzen en uitlijning blijven beschermd. Gebruik 'Handmatige kopie' om de as te wijzigen.");
                    }
                    ui.add_enabled_ui(!measured,|ui|{ui.horizontal(|ui|{ui.label("Sleufbreedte");ui.add(egui::DragValue::new(&mut editor.width).range(0.1..=100.).speed(0.05).suffix(" m"));});});
                    ui.collapsing("Geometrie en hoogtes (optioneel)",|ui|{
                    ui.add_enabled_ui(!measured,|ui|{
                        ui.horizontal(|ui|{
                            ui.label("Begin RD X");
                            ui.add(egui::DragValue::new(&mut editor.start.x).speed(0.1));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(&mut editor.start.y).speed(0.1));
                        });
                        ui.horizontal(|ui|{
                            ui.label("Einde RD X");
                            ui.add(egui::DragValue::new(&mut editor.end.x).speed(0.1));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(&mut editor.end.y).speed(0.1));
                        });

                        ui.horizontal(|ui|{
                            let old_start=editor.start.z;
                            let old_end=editor.end.z;
                            let mut has_begin=editor.start.z.is_some();
                            let mut begin_z=editor.start.z.unwrap_or(0.);
                            ui.checkbox(&mut has_begin,"Beginhoogte Z");
                            ui.add_enabled(has_begin,egui::DragValue::new(&mut begin_z).speed(0.05));
                            editor.start.z=has_begin.then_some(begin_z);
                            let mut has_end=editor.end.z.is_some();
                            let mut end_z=editor.end.z.unwrap_or(0.);
                            ui.checkbox(&mut has_end,"Eindhoogte Z");
                            ui.add_enabled(has_end,egui::DragValue::new(&mut end_z).speed(0.05));
                            editor.end.z=has_end.then_some(end_z);
                            if old_start!=editor.start.z {editor.mark_endpoint_height(true);}
                            if old_end!=editor.end.z {editor.mark_endpoint_height(false);}
                        });
                        if ui.button("Begin- en eindpunt omkeren").clicked() {
                            match editor.build() {
                                Ok(mut next)=>match edit::reverse_axis(&mut next) {
                                    Ok(())=>{
                                        editor.start=next.start;
                                        editor.end=next.end;
                                        editor.draft=next;
                                        editor.error.clear();
                                    },
                                    Err(e)=>editor.error=e,
                                },
                                Err(e)=>editor.error=e,
                            }
                        }
                        ui.horizontal(|ui|{
                            ui.label("Verschuif gehele sleuf RD ΔX");
                            ui.add(egui::DragValue::new(&mut editor.shift_x).speed(0.1));
                            ui.label("ΔY");
                            ui.add(egui::DragValue::new(&mut editor.shift_y).speed(0.1));
                            if ui.small_button("Pas verplaatsing toe").clicked() {
                                match editor.build() {
                                    Ok(mut next)=>match edit::translate(&mut next,editor.shift_x,editor.shift_y) {
                                        Ok(())=>{
                                            editor.start=next.start;
                                            editor.end=next.end;
                                            editor.draft=next;
                                            editor.shift_x=0.;
                                            editor.shift_y=0.;
                                            editor.error.clear();
                                        },
                                        Err(e)=>editor.error=e,
                                    },
                                    Err(e)=>editor.error=e,
                                }
                            }
                        });
                    });
                    });
                    editor.draw_preview(ui,&settings.object_rules);
                    ui.separator();
                    ui.collapsing(format!("Dekbandsegmenten ({})",editor.draft.dekband_rows.len()),|ui|{
                        ui.small("De dekband volgt de meetas tussen twee kettingmaten en verschijnt als gekleurde langsstreep in het gegenereerde virtuele raster.");
                        ui.horizontal(|ui|{
                            ui.label("Van (m)");
                            ui.add(egui::DragValue::new(&mut editor.dekband_start).speed(0.05));
                            ui.label("Tot (m)");
                            ui.add(egui::DragValue::new(&mut editor.dekband_end).speed(0.05));
                            ui.label("Beschrijving");
                            ui.text_edit_singleline(&mut editor.dekband_name);
                        });
                        ui.horizontal(|ui|{
                            ui.label("Kleur");
                            ui.color_edit_button_srgb(&mut editor.dekband_rgb);
                            ui.label("Diepte begin/eind (m)");
                            ui.add(egui::DragValue::new(&mut editor.dekband_start_depth).range(0.0..=1000.).speed(0.05));
                            ui.add(egui::DragValue::new(&mut editor.dekband_end_depth).range(0.0..=1000.).speed(0.05));
                            if ui.add_enabled(editor.draft.dekband_rows.len()<10_000,
                                egui::Button::new("Voeg dekband toe")).clicked() {
                                let segment=sleufbase_rust::virtual_trench::Dekband{start_depth:editor.dekband_start_depth,end_depth:editor.dekband_end_depth,
                                    start_chainage:editor.dekband_start,
                                    end_chainage:editor.dekband_end,
                                    source_name:editor.dekband_name.clone(),
                                    display_rgb:Some(editor.dekband_rgb),
                                };
                                let mut next=editor.draft.clone();
                                next.dekband_rows.push(segment);
                                match edit::check(&next) {
                                    Ok(())=>{
                                        editor.draft=next;
                                        editor.error.clear();
                                    },
                                    Err(err)=>editor.error=err,
                                }
                            }
                        });
                        let mut remove=None;
                        egui::ScrollArea::vertical().max_height(170.).show(ui,|ui|{
                            for (i,band) in editor.draft.dekband_rows.iter_mut().enumerate(){
                                ui.push_id(i,|ui|{
                                    ui.horizontal(|ui|{
                                        ui.label(format!("{}:",i+1));
                                        ui.add(egui::DragValue::new(&mut band.start_chainage).speed(0.05));
                                        ui.label("–");
                                        ui.add(egui::DragValue::new(&mut band.end_chainage).speed(0.05));
                                        ui.label("Diepte");
                                        ui.add(egui::DragValue::new(&mut band.start_depth).range(0.0..=1000.).speed(0.05));
                                        ui.add(egui::DragValue::new(&mut band.end_depth).range(0.0..=1000.).speed(0.05));
                                        ui.text_edit_singleline(&mut band.source_name);
                                        let rgb=band.display_rgb.get_or_insert([17,24,39]);
                                        ui.color_edit_button_srgb(rgb);
                                        if ui.small_button("Verwijder").clicked(){remove=Some(i);}
                                    });
                                });
                            }
                        });
                        if let Some(index)=remove{editor.draft.dekband_rows.remove(index);}
                    });
                    }else{
                    pick_requested=editor.show_objects_workspace(ui,&settings.object_rules,&settings.kickthemap_material_choices,&settings.kickthemap_profile_extra_choices);
                    ui.collapsing("Excel: meerdere meetobjecten importeren / kopiëren",|ui|{
                        ui.small("Plak tab-gescheiden cellen uit Excel: Rol, Object, Bron, Kettingmaat, Dwarsafstand, optionele Z en 3 attributen. Komma als decimaalteken is toegestaan.");
                        ui.add(egui::TextEdit::multiline(&mut editor.paste)
                            .desired_rows(5).hint_text("LS\\tKabel\\tKickTheMap\\t2,5\\t-0,3\\t-1,2"));
                        if ui.button("Importeer geplakte Excel-regels").clicked() {
                            let result=editor.finish_entry();
                            match result.and_then(|_|editor.build()){
                                Ok(mut next)=>match edit::import_tabular(&mut next,&editor.paste) {
                                    Ok(count)=>{
                                        editor.draft=next;
                                        editor.error.clear();
                                        editor.notice=format!("{count} meetobjecten toegevoegd; klik op Opslaan in project om definitief te bewaren.");
                                        editor.paste.clear();
                                    }
                                    Err(e)=>editor.error=e,
                                },
                                Err(e)=>editor.error=e,
                            }
                        }
                        if ui.button("Kopieer alle meetobjecten naar Excel").clicked() {
                            match editor.completed_draft().and_then(|t|edit::export_tabular(&t)){
                                Ok(table)=>ui.ctx().copy_text(table),
                                Err(e)=>editor.error=e,
                            }
                        }
                    });
                    }
                    ui.collapsing("Meer sleufopties",|ui|{
                        if ui.button("Handmatige kopie").clicked(){action=Some(Action::Copy);}
                        if ui.button("Proefsleuf verwijderen…").clicked(){editor.confirm_delete=true;}
                    });
                    });
                });
                });
                });
            }));
        if close_requested{open=false;}
        if let Some(cmd)=action {
            match cmd {
                Action::Save=>{
                    let result=editor.completed_draft().and_then(|mut next| {
                        // A sidebar visibility change may be newer than this draft.
                        let show=layer_visibility_ui::trench_is_visible(&self.virtual_trenches[editor.index]);
                        layer_visibility_ui::set_visible(&mut next.metadata,show)?;
                        next.validate()?;
                        self.check_trench_metadata_replacement(editor.index,&next)?;
                        Ok(next)
                    });
                    match result {
                        Ok(next)=>{
                            self.virtual_trenches[editor.index]=next;
                            self.trench_markers.clear();
                            self.dirty=true;
                            self.message="Virtuele proefsleuf en meetobjecten opgeslagen".into();
                            return;
                        },
                        Err(e)=>editor.error=e,
                    }
                },
                Action::Copy=>{
                    match editor.completed_draft().and_then(|t|edit::manual_copy(&t,"(handmatig)")).and_then(|mut copy| {
                        layer_visibility_ui::set_visible(&mut copy.metadata,true)?;
                        Ok(copy)
                    }){
                        Ok(copy)=>{
                            self.virtual_trenches.push(copy);
                            self.trench_markers.clear();
                            self.dirty=true;
                            self.message="Handmatige kopie gemaakt; de originele MarXact-meting is behouden".into();
                            let mut next=Editor::new(self.virtual_trenches.len()-1,self.virtual_trenches.last().unwrap());
                            next.entry_settings=editor.entry_settings.clone();
                            self.trench_editor=Some(next);
                            return;
                        },
                        Err(e)=>editor.error=e,
                    }
                },
                Action::Remove=>{
                    self.virtual_trenches.remove(editor.index);
                    self.trench_markers.clear();
                    self.refresh_virtual_references();
                    self.dirty=true;
                    self.message="Virtuele proefsleuf verwijderd".into();
                    return;
                },
            }
        }
        if open {
            if pick_requested{editor.picking_object=true;}
            let index=editor.index;self.trench_editor=Some(editor);
            if pick_requested{self.start_editor_point_pick(index);}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renaming_keeps_start_depth_but_editing_z_protects_manual_measurement(){
        let mut trench=VirtualTrench::new("PS1".into(),Vertex{x:155000.,y:463000.,z:Some(5.)},Vertex{x:155010.,y:463000.,z:Some(5.)},0.6).unwrap();
        trench.objects.push(TrenchPoint{role:"object".into(),object_name:"LS".into(),source_name:"LS".into(),position:Vertex{x:155002.,y:463000.,z:Some(4.)},attributes:Default::default(),metadata:serde_json::json!({"surface_mode":"start","surface_z":5.,"sleufbase_depth_meters":1.,"point_id":"source-1"})});
        let mut editor=Editor::new(0,&trench);editor.selected=Some(0);editor.object=ObjectForm::from_object(&trench,&trench.objects[0]);editor.object.name="Hernoemd".into();
        editor.add_or_replace().unwrap();assert_eq!(editor.draft.objects[0].metadata["sleufbase_depth_meters"],1.);assert_eq!(editor.draft.objects[0].metadata["surface_mode"],"start");
        editor.selected=Some(0);editor.object=ObjectForm::from_object(&editor.draft,&editor.draft.objects[0]);editor.object.use_depth=false;editor.object.elevation=3.25;editor.add_or_replace().unwrap();
        assert_eq!(editor.draft.objects[0].metadata["surface_mode"],"manual");assert_eq!(editor.draft.objects[0].metadata["point_id"],"source-1");
        editor.draft.start_metadata=serde_json::json!({"surface_mode":"auto"});
        sleufbase_rust::virtual_map::apply_surface_height(&mut editor.draft,sleufbase_rust::virtual_map::MapPoint::Start,Some(8.)).unwrap();
        assert_eq!(editor.draft.objects[0].position.z,Some(3.25));
    }
    #[test]
    fn dialog_stages_edits_without_changing_live_project_until_commit() {
        let mut app=SleufBaseApp::default();
        app.virtual_trenches.push(VirtualTrench::new("Before".into(),
            Vertex{x:100.,y:200.,z:None},Vertex{x:110.,y:200.,z:None},0.6).unwrap());
        app.open_virtual_editor(0);
        let mut editor=app.trench_editor.take().unwrap();
        editor.label="After".into();
        editor.object.chainage=3.;
        editor.object.offset=0.2;
        editor.add_or_replace().unwrap();
        assert_eq!(app.virtual_trenches[0].label,"Before");
        assert!(app.virtual_trenches[0].objects.is_empty());
        let next=editor.build().unwrap();
        app.virtual_trenches[0]=next;
        assert_eq!(app.virtual_trenches[0].label,"After");
        assert_eq!(app.virtual_trenches[0].objects.len(),1);
        assert!((app.virtual_trenches[0].objects[0].position.y-200.2).abs()<1e-9);
    }
}

#[cfg(test)]
mod usability_tests {
    use super::*;
    fn fixture()->SleufBaseApp {
        let t=VirtualTrench::new("PS12".into(),Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap();
        let mut app=SleufBaseApp{virtual_trenches:vec![t],..Default::default()};
        app.open_virtual_editor(0);app
    }
    fn frame(app:&mut SleufBaseApp,ctx:&egui::Context,size:Vec2,events:Vec<egui::Event>)->egui::FullOutput{
        ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,size)),events,..Default::default()},|ctx|app.show_virtual_editor(ctx))
    }
    fn position(out:&egui::FullOutput,label:&str)->Pos2 {
        out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some(t.pos+t.galley.rect.size()/2.),_=>None}).expect(label)
    }
    #[test]
    fn section_navigation_adds_an_object_then_saves_the_draft(){
        let ctx=egui::Context::default();theme::install(&ctx);let mut app=fixture();let size=Vec2::new(1200.,760.);
        for _ in 0..4 {frame(&mut app,&ctx,size,vec![]);}
        for label in ["Doorsnede","Kabel/leiding toevoegen","Opslaan in project"]{
            let mut out=frame(&mut app,&ctx,size,vec![]);
            for _ in 0..4 {out=frame(&mut app,&ctx,size,vec![]);}
            let pos=position(&out,label);assert!(pos.y<size.y);
            for pressed in [true,false] {frame(&mut app,&ctx,size,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
            if label=="Kabel/leiding toevoegen"{assert!(app.virtual_trenches[0].objects.is_empty());assert_eq!(app.trench_editor.as_ref().unwrap().draft.objects.len(),1);}
        }
        assert!(app.trench_editor.is_none());assert_eq!(app.virtual_trenches[0].objects.len(),1);
    }
    #[test]
    fn canvas_place_and_drag_changes_only_the_draft_until_saved(){
        let ctx=egui::Context::default();
        theme::install(&ctx);
        let mut app=fixture();
        let size=egui::vec2(1200.,760.);
        let canvas=|out:&egui::FullOutput|{
            out.shapes.iter().find_map(|shape|match &shape.shape {
                egui::Shape::Rect(r) if r.fill==Color32::from_rgb(239,230,218)=>Some(r.rect),
                _=>None,
            }).unwrap()
        };
        let press=|pos,pressed|egui::Event::PointerButton{
            pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default(),
        };
        let mut out=frame(&mut app,&ctx,size,vec![]);
        for _ in 0..4{out=frame(&mut app,&ctx,size,vec![]);}
        let plot=canvas(&out);
        let start=plot.left_top()+egui::vec2(plot.width()*0.25,plot.height()*0.25);
        frame(&mut app,&ctx,size,vec![egui::Event::PointerMoved(start),press(start,true)]);
        frame(&mut app,&ctx,size,vec![press(start,false)]);
        let editor=app.trench_editor.as_ref().unwrap();
        assert!(editor.entry_started);
        assert!((editor.object.chainage-2.5).abs()<1e-5);
        let end=plot.left_top()+egui::vec2(plot.width()*0.75,plot.height()*0.5);
        frame(&mut app,&ctx,size,vec![egui::Event::PointerMoved(start),press(start,true)]);
        frame(&mut app,&ctx,size,vec![egui::Event::PointerMoved(end)]);
        frame(&mut app,&ctx,size,vec![press(end,false)]);
        assert!(app.virtual_trenches[0].objects.is_empty());
        let editor=app.trench_editor.as_ref().unwrap();
        assert!((editor.object.chainage-7.5).abs()<1e-5);
        assert!((editor.object.depth-0.75).abs()<1e-5);
        let out=frame(&mut app,&ctx,size,vec![]);
        position(&out,"7,50 m vanaf begin   ·   Diepte 0,75 m");
        let save=position(&out,"Opslaan in project");
        frame(&mut app,&ctx,size,vec![egui::Event::PointerMoved(save),press(save,true)]);
        frame(&mut app,&ctx,size,vec![press(save,false)]);
        let objects=&app.virtual_trenches[0].objects;
        assert_eq!(objects.len(),1);
        assert!((objects[0].position.x-155007.5).abs()<1e-5);
        assert!((objects[0].position.z.unwrap()-1.25).abs()<1e-5);
    }
    #[test]
    fn crossing_preview_stays_fixed_during_a_mouse_drag(){
        let ctx=egui::Context::default();
        theme::install(&ctx);
        let mut app=fixture();
        let size=egui::vec2(1200.,760.);
        let preview=|output:&egui::FullOutput|{
            output.shapes.iter().find_map(|shape|match &shape.shape{
                egui::Shape::Rect(rect) if rect.fill==Color32::from_rgb(242,245,248)
                    &&rect.rect.size()==egui::vec2(108.,54.)=>Some(rect.rect),
                _=>None,
            }).expect("bovenaanzicht")
        };
        let mut output=frame(&mut app,&ctx,size,vec![]);
        for _ in 0..4{output=frame(&mut app,&ctx,size,vec![]);}
        let rect=preview(&output);
        let pointer=|angle:f64|{
            let (sin,cos)=angle.to_radians().sin_cos();
            rect.center()+egui::vec2(sin as f32,-cos as f32)*21.
        };
        let button=|pos,pressed|egui::Event::PointerButton{
            pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default(),
        };
        let start=pointer(0.);
        frame(&mut app,&ctx,size,vec![egui::Event::PointerMoved(start),button(start,true)]);
        for angle in [47.,-62.,18.]{
            let output=frame(&mut app,&ctx,size,vec![egui::Event::PointerMoved(pointer(angle))]);
            assert_eq!(preview(&output),rect);
            let actual=app.trench_editor.as_ref().unwrap().object.angle;
            assert!((actual-angle).abs()<0.001,"verwacht {angle}, kreeg {actual}");
        }
        frame(&mut app,&ctx,size,vec![button(pointer(18.),false)]);
        assert_eq!(preview(&frame(&mut app,&ctx,size,vec![])),rect);
    }
    #[test]
    fn save_stays_visible_and_validation_preserves_project_at_multiple_sizes_and_dpi(){
        for size in [Vec2::new(680.,420.),Vec2::new(1200.,760.),Vec2::new(1500.,930.)] {
            for dpi in [1.,1.25,2.] {
                let ctx=egui::Context::default();theme::install(&ctx);ctx.set_pixels_per_point(dpi);
                let mut app=fixture();app.trench_editor.as_mut().unwrap().end=app.virtual_trenches[0].start;
                for _ in 0..3 {frame(&mut app,&ctx,size,vec![]);}
                let out=frame(&mut app,&ctx,size,vec![]);
                let pos=position(&out,"Opslaan in project");
                assert!(pos.x>0.&&pos.x<size.x&&pos.y>0.&&pos.y<size.y,"{size:?} {dpi}: {pos:?}");
                for pressed in [true,false] {frame(&mut app,&ctx,size,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
                assert!(app.trench_editor.is_some());assert_eq!(app.virtual_trenches[0].width_meters,0.6);
                assert!(!app.trench_editor.as_ref().unwrap().error.is_empty());
                app.trench_editor.as_mut().unwrap().end=app.virtual_trenches[0].end;
                app.trench_editor.as_mut().unwrap().width=0.8;
                let out=frame(&mut app,&ctx,size,vec![]);let pos=position(&out,"Opslaan in project");
                for pressed in [true,false] {frame(&mut app,&ctx,size,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
                assert!(app.trench_editor.is_none());assert_eq!(app.virtual_trenches[0].width_meters,0.8);
            }
        }
    }
}

#[cfg(test)]
mod cable_entry_tests {
    use super::*;
    #[test]
    fn diameter_bundle_material_depth_and_angle_survive_project_and_export(){
        let t=VirtualTrench::new("PS12".into(),Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap();
        let mut e=Editor::new(0,&t);
        e.object.name="Waterleiding".into();e.object.source="water".into();e.object.role="leiding".into();
        e.object.chainage=8.5;e.object.attributes=["PE".into(),"110".into(),"3".into()];e.object.diameter=110.;e.object.quantity=3;e.object.angle=35.;e.object.use_depth=true;e.object.depth=0.7;
        e.add_or_replace().unwrap();let result=e.build().unwrap();
        assert_eq!(result.objects.len(),1);assert!((result.objects[0].position.x-155008.5).abs()<1e-8);assert_eq!(result.objects[0].position.z,Some(1.3));
        let project=NativeProject{virtual_trenches:vec![result],..Default::default()};
        let restored:NativeProject=serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
        let t=&restored.virtual_trenches[0];let form=ObjectForm::from_object(t,&t.objects[0]);
        assert_eq!(form.diameter,110.);assert_eq!(form.quantity,3);assert_eq!(form.angle,35.);
        let dataset=t.to_dataset(12,false);let feature=&dataset.features[0];
        assert_eq!(sleufbase_rust::profile::diameter_mm(feature),Some(110.));
        let label=sleufbase_rust::profile_leaders::description(feature,"Waterleiding","Ø").unwrap();
        assert!(label.contains("Bundel: 3X")&&label.contains("Ø110")&&label.contains("PE"));
        let marker=sleufbase_rust::cross_section::markers(t).remove(0);assert!((marker.end.x-marker.start.x).abs()>0.1);
        e.object.angle=81.;assert!(e.add_or_replace().is_err());assert_eq!(e.draft.objects.len(),1);
    }
    #[test]
    fn actual_map_click_chooses_position_in_the_draft_and_escape_cancels(){
        for(size,along)in[(egui::vec2(680.,420.),0.),(egui::vec2(1200.,760.),8.5)]{
            let ctx=egui::Context::default();theme::install(&ctx);
            let t=VirtualTrench::new("PS".into(),Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap();
            let mut app=SleufBaseApp{virtual_trenches:vec![t],..Default::default()};app.open_virtual_editor(0);
            app.navigation.focus=Some(location_ui::Focus{center:[155005.,463000.],meters_per_pixel:0.02});
            app.start_editor_point_pick(0);
            let frame=|app:&mut SleufBaseApp,events:Vec<egui::Event>|{
                let mut result=None;
                let _=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,size)),events,..Default::default()},|ctx|{
                    egui::CentralPanel::default().show(ctx,|ui|{
                        app.navigation.pointer_tool=app.virtual_map.captures();
                        let view=canvas(ui,&[],&[],&[],&[],&app.virtual_trenches,&[],&[],None,&[],None,None,false,&mut app.zoom,&mut app.pan,&app.migrated_settings.object_rules,Some(&mut app.navigation)).unwrap();
                        app.handle_virtual_map(&view);result=Some(view);
                    });app.show_virtual_editor(ctx);
                });result.unwrap()
            };
            frame(&mut app,vec![]);let view=frame(&mut app,vec![]);let pos=view.project([155000.+along,463000.]);
            for pressed in [true,false]{frame(&mut app,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
            let editor=app.trench_editor.as_ref().unwrap();assert!(!editor.picking_object);assert!((editor.object.chainage-along).abs()<1e-6);assert!(editor.entry_started);
            assert!(app.virtual_trenches[0].objects.is_empty()&&editor.draft.objects.is_empty());
            app.start_editor_point_pick(0);frame(&mut app,vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
            assert!(!app.trench_editor.as_ref().unwrap().picking_object);assert!(!app.virtual_map.captures());assert!(app.virtual_trenches[0].objects.is_empty());
        }
    }
}
#[cfg(test)]
mod simple_form_tests {
    use super::*;
    fn frame(app:&mut SleufBaseApp,ctx:&egui::Context,events:Vec<egui::Event>)->egui::FullOutput{
        ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(1200.,760.))),events,..Default::default()},|ctx|app.show_virtual_editor(ctx))
    }
    fn pos(out:&egui::FullOutput,text:&str)->Pos2{
        out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==text=>Some(t.pos+t.galley.rect.size()/2.),_=>None}).expect(text)
    }
    fn click(app:&mut SleufBaseApp,ctx:&egui::Context,p:Pos2)->egui::FullOutput{
        for pressed in [true,false]{frame(app,ctx,vec![egui::Event::PointerMoved(p),egui::Event::PointerButton{pos:p,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        frame(app,ctx,vec![])
    }
    #[test]
    fn type_dropdown_and_end_distance_save_one_cable_without_a_second_add_action(){
        let t=VirtualTrench::new("PS12".into(),Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap();
        let ctx=egui::Context::default();theme::install(&ctx);let mut app=SleufBaseApp{virtual_trenches:vec![t],..Default::default()};app.open_virtual_editor(0);
        let mut out=frame(&mut app,&ctx,vec![]);for _ in 0..3{out=frame(&mut app,&ctx,vec![]);}
        let selected=app.migrated_settings.object_rules.iter().find(|r|r.keywords.iter().any(|k|k.eq_ignore_ascii_case("ls"))).unwrap().display_label().to_owned();
        let out=click(&mut app,&ctx,pos(&out,&selected));let out=click(&mut app,&ctx,pos(&out,"Waterleiding"));
        assert_eq!(app.trench_editor.as_ref().unwrap().object.source,"water");
        let mut out=click(&mut app,&ctx,pos(&out,"Exacte positie of kaartpunt"));
        for _ in 0..8{out=frame(&mut app,&ctx,vec![]);}
        click(&mut app,&ctx,pos(&out,"Eindpunt"));
        let form=&mut app.trench_editor.as_mut().unwrap().object;assert!(form.from_end);form.set_distance(10.,object_ui::parse_decimal("1,5").unwrap()).unwrap();
        assert_eq!(form.distance(10.),1.5);assert_eq!(form.chainage,8.5);assert!(form.set_distance(10.,11.).is_err());
        let out=frame(&mut app,&ctx,vec![]);click(&mut app,&ctx,pos(&out,"Opslaan in project"));
        assert!(app.trench_editor.is_none());assert_eq!(app.virtual_trenches[0].objects.len(),1);
        assert_eq!(app.virtual_trenches[0].objects[0].source_name,"water");assert!((app.virtual_trenches[0].objects[0].position.x-155008.5).abs()<1e-8);
    }
    #[test]
    fn anchor_switch_alone_is_not_a_new_cable_and_count_changes_keep_sleeve_flags(){
        let mut form=ObjectForm::default();form.from_end=true;assert!(!form.has_pending_entry());
        assert_eq!(object_ui::parse_decimal("-12,5"),Some(-12.5));assert!(object_ui::parse_decimal("NaN").is_none());assert!(object_ui::parse_decimal("abc").is_none());
        form.attributes[2]="mantelbuis dubbel 3X".into();form.quantity=4;form.update_quantity_attribute();
        assert!(form.attributes[2].contains("mantelbuis")&&form.attributes[2].contains('4'));assert!(!form.attributes[2].contains("dubbel"));
    }
}

#[cfg(test)]
mod depth_audit_tests {
    use super::*;
    #[test]
    fn reopening_depth_entry_keeps_depth_visible_and_manual_override_breaks_surface_link(){
        let mut trench=VirtualTrench::new("PS12".into(),Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap();
        let form=ObjectForm{chainage:3.,use_depth:true,depth:0.85,..Default::default()};trench.objects.push(form.to_object(&trench).unwrap());
        let mut editor=Editor::new(0,&trench);editor.selected=Some(0);editor.object=ObjectForm::from_object(&trench,&trench.objects[0]);assert!(editor.object.use_depth);assert_eq!(editor.object.depth,0.85);
        editor.object.use_depth=false;editor.add_or_replace().unwrap();let point=&editor.draft.objects[0];assert_eq!(point.position.z,trench.objects[0].position.z);assert_eq!(point.metadata["surface_mode"],"manual");assert!(point.metadata.get("sleufbase_depth_meters").is_none());
        sleufbase_rust::virtual_map::apply_surface_height(&mut editor.draft,sleufbase_rust::virtual_map::MapPoint::Start,Some(8.)).unwrap();assert_eq!(editor.draft.objects[0].position.z,trench.objects[0].position.z);
    }
}

#[cfg(test)]
mod draft_snapshot_tests {
    use super::*;
    fn editor()->Editor {
        let mut trench=VirtualTrench::new("PS12".into(),
            Vertex{x:155000.,y:463000.,z:Some(2.)},
            Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap();
        trench.objects.push(ObjectForm{chainage:8.,name:"Bestaande kabel".into(),..Default::default()}.to_object(&trench).unwrap());
        Editor::new(0,&trench)
    }
    #[test]
    fn manual_copy_and_excel_snapshot_include_pending_new_and_edited_cables() {
        for edit_existing in [false,true] {
            let mut editor=editor();
            let original=serde_json::to_vec(&editor.original).unwrap();
            if edit_existing {assert!(editor.select_object(0));}
            editor.object.name="Ingevoerde waterleiding".into();
            editor.object.source="water".into();
            editor.object.chainage=3.;
            editor.object.use_depth=true;editor.object.depth=0.75;
            let snapshot=editor.completed_draft().unwrap();
            let expected=if edit_existing{1}else{2};
            assert_eq!(snapshot.objects.len(),expected);
            let copy=edit::manual_copy(&snapshot,"(handmatig)").unwrap();
            let tsv=edit::export_tabular(&snapshot).unwrap();
            assert_eq!(copy.objects[0].object_name,"Ingevoerde waterleiding");
            assert_eq!(copy.objects[0].position.z,Some(1.25));
            assert!(tsv.contains("Ingevoerde waterleiding\twater\t3.0000"));
            assert_eq!(editor.completed_draft().unwrap().objects.len(),expected);
            assert_eq!(serde_json::to_vec(&editor.original).unwrap(),original);
        }
    }
    #[test]
    fn excel_import_after_unchanged_selection_does_not_create_a_duplicate() {
        let mut editor=editor();assert!(editor.select_object(0));
        assert!(!editor.entry_changed());
        editor.finish_entry().unwrap();
        let mut next=editor.build().unwrap();
        edit::import_tabular(&mut next,"leiding\tNieuw water\twater\t3\t0\t1.2").unwrap();
        editor.draft=next;
        let saved=editor.completed_draft().unwrap();
        assert_eq!(saved.objects.len(),2);
        assert_eq!(saved.objects.iter().filter(|p|p.object_name=="Bestaande kabel").count(),1);
    }
    #[test]
    fn invalid_pending_entry_blocks_copy_and_keeps_the_draft() {
        let mut editor=editor();let before=serde_json::to_vec(&editor.draft).unwrap();
        editor.object.name="Nieuwe kabel".into();editor.object.angle=81.;
        assert!(editor.completed_draft().is_err());
        assert_eq!(serde_json::to_vec(&editor.draft).unwrap(),before);
        assert_eq!(editor.object.name,"Nieuwe kabel");
    }
    #[test]
    fn saving_a_new_cable_with_changed_start_height_updates_existing_linked_depths() {
        let mut editor=editor();
        editor.draft.objects[0].position.z=Some(1.4);
        editor.draft.objects[0].metadata=serde_json::json!({"surface_mode":"start","surface_z":2.,"sleufbase_depth_meters":0.6});
        editor.start.z=Some(5.);editor.mark_endpoint_height(true);
        editor.object.name="Nieuwe kabel".into();editor.object.chainage=3.;
        editor.object.use_depth=true;editor.object.depth=0.8;
        let saved=editor.completed_draft().unwrap();
        assert_eq!(saved.objects[0].position.z,Some(4.2));
        assert_eq!(saved.objects[1].position.z,Some(4.4));
        assert_eq!(saved.objects[1].metadata["surface_z"],5.);
    }
}

#[cfg(test)]
mod repeated_entry_tests {
    use super::*;
    fn trench()->VirtualTrench {
        VirtualTrench::new("PS12".into(),
            Vertex{x:155000.,y:463000.,z:Some(3.)},
            Vertex{x:155010.,y:463000.,z:Some(3.)},0.6).unwrap()
    }
    #[test]
    fn repeating_properties_waits_for_input_and_never_adds_an_unused_template() {
        let mut editor=Editor::new(0,&trench());
        editor.object.name="Waterleiding".into();editor.object.source="water".into();
        editor.object.role="leiding".into();editor.object.attributes=["PE".into(),"110".into(),"2 mantelbuis".into()];
        editor.object.diameter=110.;editor.object.quantity=2;editor.object.angle=25.;
        editor.object.use_depth=true;editor.object.depth=0.8;editor.object.chainage=4.;
        editor.add_or_replace().unwrap();
        editor.prepare_similar_entry().unwrap();
        assert_eq!((editor.object.name.as_str(),editor.object.diameter,editor.object.quantity,editor.object.angle),("Waterleiding",110.,2,25.));
        assert_eq!(editor.object.chainage,0.);
        assert!(!editor.entry_changed());
        // Merely changing the distance anchor is not a new cable either.
        editor.object.from_end=true;assert!(!editor.entry_changed());
        let saved=editor.completed_draft().unwrap();
        assert_eq!(saved.objects.len(),1);
        assert_eq!(saved.objects[0].position.x,155004.);
        assert_eq!(saved.objects[0].position.z,Some(2.2));
        editor.prepare_similar_entry().unwrap();
        editor.pick_object_position([155007.,463000.]).unwrap();
        assert!(editor.entry_changed());
        let saved=editor.completed_draft().unwrap();
        assert_eq!(saved.objects.len(),2);
        assert_eq!(saved.objects[1].position.x,155007.);
        assert_eq!(saved.objects[1].position.z,Some(2.2));
    }
    #[test]
    fn repeating_a_prepared_selection_keeps_that_type_instead_of_the_last_added_type() {
        let mut source=trench();
        source.objects.push(ObjectForm{name:"Bestaande LS".into(),source:"ls".into(),chainage:2.,diameter:50.,attributes:[String::new(),"50".into(),String::new()],..Default::default()}.to_object(&source).unwrap());
        let mut editor=Editor::new(0,&source);
        editor.object.name="Waterleiding".into();editor.object.source="water".into();
        editor.object.diameter=110.;editor.object.chainage=4.;editor.add_or_replace().unwrap();
        assert_eq!(editor.last_entry.as_ref().unwrap().source,"water");
        assert!(editor.select_object(0));editor.prepare_similar_entry().unwrap();
        assert_eq!(editor.object.name,"Bestaande LS");
        assert!(!editor.entry_changed());
        editor.prepare_similar_entry().unwrap();
        assert_eq!(editor.object.name,"Bestaande LS");
        assert_eq!(editor.object.source,"ls");
        assert_eq!(editor.object.diameter,50.);
        assert!(!editor.entry_changed());
        assert_eq!(editor.completed_draft().unwrap().objects.len(),2);
    }
    #[test]
    fn repeating_a_measured_object_does_not_copy_position_height_or_identity() {
        let mut source=trench();
        source.objects.push(TrenchPoint{
            role:"kabel".into(),object_name:"LS".into(),source_name:"ls".into(),
            attributes:["Al".into(),"50".into(),"".into()],
            position:Vertex{x:155002.,y:463000.3,z:Some(1.1)},
            metadata:serde_json::json!({"point_id":"measurement-17","surface_mode":"manual","template_reference_point":{"x":155001.,"y":463001.},"crossing_angle_degrees":-15.,"quantity":1,"display_rgb":[255,160,0]}),
        });
        let original=serde_json::to_vec(&source.objects[0]).unwrap();
        let mut editor=Editor::new(0,&source);
        assert!(editor.select_object(0));editor.prepare_similar_entry().unwrap();
        assert!(!editor.object.has_elevation);
        assert_eq!((editor.object.chainage,editor.object.offset,editor.object.elevation),(0.,0.,0.));
        assert_eq!(editor.object.rgb,Some([255,160,0]));
        editor.pick_object_position([155006.,463000.]).unwrap();
        let saved=editor.completed_draft().unwrap();
        assert_eq!(serde_json::to_vec(&saved.objects[0]).unwrap(),original);
        let added=&saved.objects[1];
        assert_eq!(added.position,Vertex{x:155006.,y:463000.,z:None});
        assert_eq!(added.attributes,["Al","50",""]);
        assert_eq!(added.metadata["crossing_angle_degrees"],-15.);
        assert!(added.metadata.get("point_id").is_none());
        assert!(added.metadata.get("template_reference_point").is_none());
        assert!(added.metadata.get("surface_mode").is_none());
        editor.object.angle=81.;
        let before=serde_json::to_vec(&editor.draft).unwrap();
        assert!(editor.prepare_similar_entry().is_err());
        assert_eq!(serde_json::to_vec(&editor.draft).unwrap(),before);
        assert_eq!(editor.object.angle,81.);
    }
}
