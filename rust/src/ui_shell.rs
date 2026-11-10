//! Original SleufBase shell; original-shell.json records the source evidence.
use super::*;
use sleufbase_rust::project_view::BackgroundProvider as Background;
use theme::{APP_BG, INPUT_BG, MUTED, SURFACE_ALT};

pub(super) struct State {
    pub settings_scan: Option<sleufbase_rust::recovery::ScanWorker>,
    pub settings_generation: Option<u64>,
    focused_layer: Option<(egui::Id, bool)>,
    pub login_open: bool,
    pub login_focus: bool,
    pub login_remember: bool,
    pub login_email: String,
    pub login_password: String,
    pub installed: bool,
    pub extended: bool,
    pub coords: String,
    pub(super) marxact_open: bool,
    marxact_generation:u64,
    marxact_index:usize,
    marxact_angle:f64,
    tiff_height: Option<f32>,
    klic_height: Option<f32>,
    last_sidebar_height: Option<f32>,
}

#[cfg(test)]
#[path = "ui_shell_tests.rs"]
mod tests;
impl Default for State {
    fn default() -> Self {
        Self {
            settings_scan: None,
            settings_generation: None,
            focused_layer: None,
            login_open: false,login_focus:false,login_remember:true,login_email:String::new(),login_password:String::new(),
            installed: false,
            extended: false,
            coords: "RD: -".into(),
            marxact_open:false,marxact_generation:0,
            marxact_index:0,marxact_angle:0.,
            tiff_height: None,
            klic_height: None,
            last_sidebar_height: None,
        }
    }
}

// None entries are separators. Keep source order, including launcher hooks.
const ACTIONS: &[(&str, &str)] = &[
    ("Project laden...", "load"),
    ("Project opslaan", "save"),
    ("Project opslaan als...", "save_as"),
    ("", ""),
    ("Laad GeoTIFF(s)", "tiff"),
    ("Laad KLIC DXF", "dxf"),
    ("Zoom naar alles", "zoom"),
    ("Toon TIFF-adressen", "addresses"),
    ("", ""),
    ("Export kaarten", "maps"),
    ("Kadastrale export DXF", "cadastral"),
    ("Proefsleuven sjabloon DXF", "template"),
    ("", ""),
    ("Wis alle referentiepunten", "clear_refs"),
    ("Laad alle beginpunten", "starts"),
    ("", ""),
    ("Wis opmerkingen", "clear_comments"),
    ("Wis alles", "clear"),
    ("Discipline-overzicht naar Excel…", "excel"),
];
const KICKTHEMAP: &[(&str, &str)] = &[
    ("Jobs", "jobs"),
    ("Inloggen...", "login"),
    ("Exporteer objectpunten naar DXF...", "objects"),
    ("", ""),
    ("Uitloggen", "logout"),
];

fn menu(ui: &mut egui::Ui, label: &str, items: &[(&str, &'static str)]) -> Option<&'static str> {
    // Return an action after closing the popup; file dialogs never borrow its UI.
    let mut action = None;
    ui.menu_button(label, |ui| {
        ui.set_min_width(190.);
        ui.style_mut().override_font_id = Some(egui::FontId::proportional(12.));
        ui.spacing_mut().item_spacing.y = 3.;
        ui.style_mut().visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(255, 224, 199);
        ui.style_mut().visuals.widgets.hovered.bg_fill = Color32::from_rgb(255, 224, 199);
        for &(label, id) in items {
            if label.is_empty() {
                ui.add_space(4.);
                ui.separator();
                ui.add_space(4.);
            } else if ui
                .add_sized(
                    [ui.available_width(), 30.],
                    egui::Button::new(label)
                        .frame(true)
                        .corner_radius(8)
                        .wrap_mode(egui::TextWrapMode::Extend),
                )
                .clicked()
            {
                action = Some(id);
                ui.close_menu();
            }
        }
    });
    action
}

fn separator(ui: &mut egui::Ui, height: &mut f32, min: f32, max: f32, id: &'static str) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.), egui::Sense::hover());
    let response = ui.interact(rect, ui.id().with(id), egui::Sense::drag());
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if response.dragged() {
        *height = (*height + response.drag_delta().y).clamp(min, max.max(min));
    }
}

fn tiff_row(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(if selected {
            Color32::WHITE
        } else {
            INK
        }))
        .selected(selected)
        .frame(false)
        .stroke(Stroke::NONE)
        .corner_radius(0)
        .min_size(egui::vec2(ui.available_width(), 18.))
        .truncate(),
    )
}

impl SleufBaseApp {
    pub(super) fn show_original_menu(&mut self, ctx: &egui::Context) {
        let mut action = None;
        egui::TopBottomPanel::top("sleufbase-menu")
            .exact_height(30.)
            .frame(egui::Frame::new().fill(APP_BG).inner_margin(egui::Margin {
                left: 18,
                right: 18,
                top: 2,
                bottom: 2,
            }))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.x = 6.;
                ui.spacing_mut().button_padding = egui::vec2(10., 4.);
                ui.spacing_mut().interact_size.y = 26.;
                ui.style_mut().override_font_id =
                    Some(egui::FontId::proportional(theme::FONT_SIZE));
                ui.horizontal(|ui| {
                    if ui.button("Locatie").clicked() {
                        self.open_location();
                    }
                    action = action.or(menu(ui, "Acties", ACTIONS));
                    action = action.or(menu(ui, "KickTheMap", KICKTHEMAP));
                    action = action.or(menu(ui, "AI", &[("Maaiveld via Gemma", "ai")]));
                    let settings = ui.button("Instellingen");
                    if settings.clicked() {
                        self.open_settings();
                    }
                    settings
                        .on_hover_text("Rechtermuisknop voor aanvullende hulpmiddelen")
                        .context_menu(|ui| {
                            if ui.button("Aanvullende hulpmiddelen").clicked() {
                                self.shell.extended = true;
                                ui.close_menu();
                            }
                        });
                    action = action.or(menu(ui, "marXact", &[("marXact import", "marxact"),("Oriëntatie aanpassen...", "marxact_orientation")]));
                });
            });
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::U)
        }) {
            self.shell.extended = !self.shell.extended;
        }
        if let Some(action) = action {
            self.run_shell_action(action, ctx);
        }
    }

    fn run_shell_action(&mut self, action: &str, ctx: &egui::Context) {
        match action {
            "load" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("SleufBase-project", &["json", "klicproject"])
                    .pick_file()
                {
                    self.request_project_action(ProjectAction::OpenInput(path), ctx);
                }
            }
            "save" => {
                self.save_project_file(false);
            }
            "save_as" => {
                self.save_project_file(true);
            }
            "tiff" => {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("GeoTIFF", &["tif", "tiff"])
                    .pick_files()
                {
                    self.load_geotiffs(files, ctx);
                }
            }
            "dxf" => {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("KLIC DXF", &["dxf"])
                    .pick_files()
                {
                    self.load_overlays(files);
                }
            }
            "zoom" => {
                self.navigation.focus = None;
                self.navigation.fit_bounds = None;
                self.navigation.viewport = None;
                self.zoom = 1.;
                self.pan = egui::Vec2::ZERO;
            }
            "addresses" => self.open_tiff_addresses(),
            "maps" => {
                if let Some(folder)=rfd::FileDialog::new().set_title("Kies exportmap voor kaarten").pick_folder(){
                    self.start_maps_export(folder);
                }
            }
            "cadastral" => self.export_cadastral_from_actions(),
            "template" => self.open_template_export(),
            "clear_refs" => self.clear_raster_references(),
            "starts" => self.load_all_shell_start_points(),
            "clear_comments" => self.clear_map_comments(),
            "clear" => self.request_project_action(ProjectAction::ClearLayers, ctx),
            "excel" => self.save_excel(),
            "jobs" => {
                self.open_kickthemap_workspace();
            }
            "login" => {
                self.open_kickthemap_login();
            },
            "objects" => self.save_dxf(),
            "logout" => self.logout_live_jobs(),
            "ai" => self.open_ai_maaiveld(),
            "marxact_orientation" => self.open_marxact_orientation(),
            "marxact" => {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("marXact DXF", &["dxf"])
                    .pick_files()
                {
                    self.import_marxact(files);
                }
            }
            _ => unreachable!("unknown shell action"),
        }
    }

    fn marxact_indices(&self)->Vec<usize>{
        self.virtual_trenches.iter().enumerate()
            .filter_map(|(i,t)|(t.alignment.is_some()||!t.boundary_3d.is_empty()).then_some(i))
            .collect()
    }
    pub(super) fn open_marxact_orientation(&mut self){
        let indices=self.marxact_indices();
        let Some(&index)=indices.first() else{
            self.message="Er zijn geen geladen MarXact-proefsleuven.".into();return;
        };
        self.shell.marxact_open=true;
        self.shell.marxact_generation=self.project_generation;
        self.shell.marxact_index=index;
        self.shell.marxact_angle=self.virtual_trenches[index].alignment
            .as_ref().map_or(0.,|a|a.rotation_degrees);
    }
    pub(super) fn show_marxact_orientation(&mut self,ctx:&egui::Context){
        if !self.shell.marxact_open{return;}
        if self.shell.marxact_generation!=self.project_generation{
            self.shell.marxact_open=false;return;
        }
        let indices=self.marxact_indices();
        if indices.is_empty(){self.shell.marxact_open=false;return;}
        if !indices.contains(&self.shell.marxact_index){
            self.shell.marxact_index=indices[0];
            self.shell.marxact_angle=self.virtual_trenches[indices[0]].alignment
                .as_ref().map_or(0.,|a|a.rotation_degrees);
        }
        let mut open=true;
        let mut operation=None;
        egui::Window::new("MarXact kabel-/leidingoriëntatie")
            .open(&mut open).default_width(650.).default_height(310.)
            .min_width(590.).min_height(290.)
            .resizable(false).show(ctx,|ui|{
                ui.heading("MarXact kabel-/leidingoriëntatie");
                ui.label("De automatische richting volgt de gemeten 3D-POLYLINE-randen. De rotatiecorrectie draait de gekozen sleuf ten opzichte van de automatische uitlijning.");
                ui.horizontal(|ui|{
                    ui.label("Proefsleuf");
                    let selected=self.shell.marxact_index;
                    egui::ComboBox::from_id_salt("marxact-orientation-source")
                        .width(390.).selected_text(&self.virtual_trenches[selected].label)
                        .show_ui(ui,|ui|{
                            for &index in &indices{
                                if ui.selectable_label(index==self.shell.marxact_index,
                                    &self.virtual_trenches[index].label).clicked(){
                                    self.shell.marxact_index=index;
                                    self.shell.marxact_angle=self.virtual_trenches[index].alignment
                                        .as_ref().map_or(0.,|a|a.rotation_degrees);
                                }
                            }
                        });
                });
                ui.horizontal(|ui|{
                    ui.label("Rotatiecorrectie");
                    if ui.add(egui::Slider::new(&mut self.shell.marxact_angle,-90. ..=90.)
                        .suffix("°")).changed(){
                        operation=Some("apply");
                    }
                    if ui.add(egui::DragValue::new(&mut self.shell.marxact_angle)
                        .range(-90. ..=90.).speed(0.1).suffix("°")).changed(){
                        operation=Some("apply");
                    }
                });
                ui.small(format!("{:+.1}° ten opzichte van automatisch",self.shell.marxact_angle));
                ui.horizontal(|ui|{
                    for (label,delta) in [("−1°",-1.),("−0,1°",-0.1),("+0,1°",0.1),("+1°",1.)]{
                        if ui.button(label).clicked(){
                            self.shell.marxact_angle=(self.shell.marxact_angle+delta).clamp(-90.,90.);
                            operation=Some("apply");
                        }
                    }
                    if ui.button("Correctie 0°").clicked(){
                        self.shell.marxact_angle=0.;operation=Some("apply");
                    }
                });
                ui.horizontal(|ui|{
                    if ui.button("Automatisch opnieuw bepalen").clicked(){operation=Some("recalculate");}
                    if ui.button("Sluiten").clicked(){operation=Some("close");}
                });
            });
        self.shell.marxact_open=open;
        match operation{
            Some("apply")|Some("recalculate")=>{
                let index=self.shell.marxact_index;
                let mut candidate=self.virtual_trenches[index].clone();
                let result=if operation==Some("recalculate"){
                    sleufbase_rust::marxact_alignment::recalculate(&mut candidate)
                }else{
                    sleufbase_rust::marxact_alignment::apply_rotation(
                        &mut candidate,self.shell.marxact_angle)
                };
                match result{
                    Ok(())=>{
                        self.virtual_trenches[index]=candidate;
                        if operation==Some("recalculate"){self.shell.marxact_angle=0.;}
                        self.trench_markers.clear();
                        self.refresh_virtual_references();
                        self.dirty=true;
                        self.message="MarXact-oriëntatie bijgewerkt.".into();
                    }
                    Err(e)=>self.message=format!("MarXact-oriëntatie: {e}"),
                }
            }
            Some("close")=>self.shell.marxact_open=false,
            _=>{}
        }
    }
    pub(super) fn show_original_sidebar(&mut self, ctx: &egui::Context) {
        self.shell.focused_layer = None;
        let width = (ctx.screen_rect().width().max(1200.) * 0.255).clamp(360., 400.);
        egui::SidePanel::left("original-sidebar").default_width(width).min_width(340.)
            .frame(egui::Frame::new().fill(APP_BG).inner_margin(egui::Margin { left: 18, right: 14, top: 18, bottom: 18 }))
            .show(ctx, |ui| {
                theme::title(ui, "Weergave");
                theme::card(18, 12, 8).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(74.);
                    let field_width=(ui.available_width()-44.).max(120.);
                    ui.spacing_mut().slider_width=field_width;
                    let previous = self.project_view.clone();
                    ui.spacing_mut().item_spacing.x=12.;
                    ui.horizontal(|ui| {
                        ui.add_sized([32.,28.],egui::Label::new("Kaart").halign(egui::Align::Min));
                        ui.allocate_ui_with_layout(egui::vec2(field_width,28.),egui::Layout::left_to_right(egui::Align::Center),|ui|{
                            ui.set_max_width(field_width);
                            let display = |source:Background| match source {
                                Background::PdokAerial=>"PDOK Luchtfoto NL actueel (8 cm, deels 5 cm)",
                                Background::Cadastral=>"PDOK kadastrale kaart",Background::OpenStreetMap=>"OpenStreetMap",_=>"Onbekende achtergrond",
                            };
                            egui::ComboBox::from_id_salt("original-background").width(field_width).truncate().selected_text(display(self.project_view.provider())).show_ui(ui,|ui|{
                                for source in [Background::PdokAerial,Background::Cadastral,Background::OpenStreetMap] {
                                    if ui.selectable_label(self.project_view.use_background&&self.project_view.background_source==source.label(),display(source)).clicked(){
                                        self.project_view.use_background=true;self.project_view.background_source=source.label().into();
                                    }
                                }
                            });
                        });
                    });
                    ui.horizontal(|ui|{
                        ui.add_sized([32.,28.],egui::Label::new("TIFF").halign(egui::Align::Min));
                        ui.add(egui::Slider::new(&mut self.project_view.opacity_percent,0. ..=100.).show_value(false));
                    });
                    if previous != self.project_view {
                        self.dirty = true;
                        if previous.use_background != self.project_view.use_background || previous.background_source != self.project_view.background_source { self.request_selected_background(); }
                    }
                });
                ui.add_space(6.);
                let available = ui.available_height();
                let mut tiff = self.shell.tiff_height.unwrap_or((available.max(620.) * 0.30).clamp(220., 280.));
                let mut klic = self.shell.klic_height.unwrap_or((available.max(620.) * 0.16).clamp(120., 150.));
                if let Some(previous)=self.shell.last_sidebar_height {
                    let delta=available-previous;
                    tiff+=delta*2./8.; klic+=delta/8.;
                }
                self.shell.last_sidebar_height=Some(available);
                tiff = tiff.min((available - 230.).max(140.));
                klic = klic.min((available - tiff - 110.).max(100.));
                self.show_shell_tiffs(ui, tiff);
                separator(ui, &mut tiff, 140., available - klic - 110., "tiff-sash");
                self.show_shell_overlays(ui, klic);
                separator(ui, &mut klic, 100., available - tiff - 110., "klic-sash");
                self.shell.tiff_height = Some(tiff); self.shell.klic_height = Some(klic);
                theme::title(ui, "Kabelinformatie");
                let height = (ui.available_height() - 30.).max(40.);
                theme::card(18, 14, 14).show(ui, |ui| {
                    ui.set_min_size(egui::vec2(ui.available_width(),height));
                    egui::ScrollArea::vertical().id_salt("cable-information").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible).max_height(height).min_scrolled_height(height).show(ui, |ui| {
                        if let Some(selection) = &self.dxf_selection {
                            for (key, value) in &selection.details {
                                theme::strong(ui,cad_preview(key));
                                let preview=cad_preview(value);
                                ui.add(egui::Label::new(preview).wrap());
                                if preview.len()<value.len() {
                                    if ui.small_button("Volledige tekst kopiëren").clicked(){ui.ctx().copy_text(value.clone());}
                                }
                            }
                        } else { ui.label("Klik in de kaart op een kabel om KLIC-informatie en laagnamen te tonen.\n\nGebruik de rechtermuisknop op een proefsleuf in de lijst om draaien en opmerkingen te beheren.\n\nTip: zoom verder in voor een preciezere selectie als kabels dicht op elkaar liggen."); }
                    });
                });
            });
    }

    fn show_shell_tiffs(&mut self, ui: &mut egui::Ui, height: f32) {
        theme::title(ui, "GeoTIFF Proefsleuven");
        let mut contexts = Vec::new();
        let mut zoom_target = None;
        theme::card(18, 14, 14).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let list_height = (height - 90.).max(28.);
            let mut selected = None;
            egui::Frame::new().fill(INPUT_BG).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_salt("original-tiffs")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .max_height(list_height)
                    .show(ui, |ui| {
                        ui.set_min_size(egui::vec2(ui.available_width(), list_height));
                        ui.spacing_mut().button_padding = egui::Vec2::ZERO;
                        ui.spacing_mut().interact_size.y = 18.;
                        ui.spacing_mut().item_spacing.y = 0.;
                        for (i, raster) in self.geotiffs.iter().enumerate() {
                            let label = raster
                                .path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            let response = tiff_row(
                                ui,
                                self.raster_points.selected.as_ref() == Some(&raster.path),
                                label.as_ref(),
                            );
                            if response.clicked() || response.secondary_clicked() {
                                response.request_focus();
                                selected = Some((false, i));
                            }
                            if response.has_focus() && self.raster_points.selected.as_ref() == Some(&raster.path) {
                                self.shell.focused_layer = Some((response.id, false));
                            }
                            if response.double_clicked() { zoom_target=Some(raster.bounds); }
                            contexts.push((false,i,response.clone()));
                            if let Some(error) = self.raster_errors.get(i).and_then(|e| e.as_ref())
                            {
                                response.on_hover_text(error);
                            }
                        }
                        for (i, trench) in self.virtual_trenches.iter().enumerate() {
                            let response =
                                tiff_row(ui, self.virtual_map.selected == Some(i), &trench.label);
                            if response.clicked() || response.secondary_clicked() {
                                response.request_focus();
                                selected = Some((true, i));
                            }
                            if response.has_focus() && self.virtual_map.selected == Some(i) {
                                self.shell.focused_layer = Some((response.id, false));
                            }
                            if response.double_clicked() {
                                let mut bounds=None;
                                for (x,y) in trench.measured_boundary() {
                                    bounds=union_bounds(bounds,Some(Bounds{min_x:x,min_y:y,max_x:x,max_y:y}));
                                }
                                zoom_target=bounds;
                            }
                            contexts.push((true,i,response));
                        }
                    });
            });
            if let Some((virtual_trench, i)) = selected {
                let old_virtual = self.virtual_map.selected;
                let old_raster = self.raster_points.selected.clone();
                self.cancel_virtual_map();
                self.stop_raster_rotation();
                self.raster_points.mode = None;
                self.raster_points.rows.clear();
                self.virtual_map.selected = virtual_trench.then_some(i);
                self.raster_points.selected = if virtual_trench {
                    None
                } else {
                    Some(self.geotiffs[i].path.clone())
                };
                self.update_view_selection(old_virtual, old_raster.as_deref());
            }
            ui.horizontal(|ui| {
                if ui.button("Virtuele proefsleuf").clicked() {
                    self.start_virtual_creation();
                }
                if ui.button("Verwijder").clicked() {
                    self.remove_shell_tiff();
                }
            });
        });
        if let Some(bounds)=zoom_target {
            self.navigation.fit_bounds=Some(bounds);
            self.navigation.viewport=None;
            self.zoom=1.;self.pan=egui::Vec2::ZERO;
            self.dirty=true;
        }
        // The menu belongs to each row. Making the entire card clickable after
        // its children would intercept ordinary selection and footer clicks.
        for (is_virtual,index,response) in contexts {
            response.context_menu(|ui| {
                if is_virtual {
                    if self.virtual_trenches.get(index).is_some(){
                        self.show_virtual_tiff_context(ui,index);
                    }
                } else if let Some(path)=self.geotiffs.get(index).map(|r|r.path.clone()) {
                    // Right-click selects the row actually clicked, not a
                    // previously selected layer (or another duplicate path).
                    let old_virtual=self.virtual_map.selected;
                    let old_raster=self.raster_points.selected.clone();
                    self.virtual_map.selected=None;
                    self.raster_points.selected=Some(path.clone());
                    if old_virtual.is_some()||old_raster.as_ref()!=Some(&path){
                        self.update_view_selection(old_virtual,old_raster.as_deref());
                    }
                    if ui.button("Maak virtuele kopie...").clicked(){
                        self.make_virtual_from_raster(index);
                        ui.close_menu();
                    }
                    ui.separator();
                    self.show_physical_rotation_menu(ui);
                    ui.separator();
                    if ui.button("Opmerking bewerken...").clicked(){
                        self.open_tiff_comment(&path);ui.close_menu();
                    }
                    if ui.button("Opmerking wissen").clicked(){
                        self.clear_tiff_comment(&path);ui.close_menu();
                    }
                }
            });
        }
    }

    fn show_shell_overlays(&mut self, ui: &mut egui::Ui, height: f32) {
        theme::title(ui, "KLIC Overlays");
        let mut remove_job=None;
        theme::card(18, 14, 14).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let list_height = (height - 90.).max(24.);
            egui::ScrollArea::vertical()
                .id_salt("original-overlays")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .max_height(list_height)
                .show(ui, |ui| {
                    ui.set_min_size(egui::vec2(ui.available_width(), list_height));
                    // JobFeatures objects share this panel with DXF/KLIC overlays.
                    // A checkbox hides only the map layer; the source job stays
                    // loaded for profiles, exports and subsequent sessions.
                    for (index,job) in self.datasets.iter().enumerate() {
                        ui.push_id(("job-object-overlay", &job.source_path), |ui| {
                            ui.horizontal(|ui| {
                                let mut visible=!self.project_view.hidden_object_layers.contains(&job.source_path);
                                if ui.checkbox(&mut visible,"").on_hover_text("Toon objectpunten en objectlijnen op de kaart").changed() {
                                    self.project_view.hidden_object_layers.retain(|p|p!=&job.source_path);
                                    if !visible {self.project_view.hidden_object_layers.push(job.source_path.clone());}
                                    self.dirty=true;
                                }
                                let response=ui.selectable_label(
                                    self.selected==Some(index)&&self.project_view.selected_dxf.is_none(),
                                    format!("Job: {} ({} objecten)",job.title,job.features.len())
                                ).on_hover_text(format!("KickTheMap-objecten uit {}",job.source_path.display()));
                                response.context_menu(|ui|{
                                    if ui.button("Verwijder objectlaag uit project").clicked(){
                                        remove_job=Some(index);
                                        ui.close_menu();
                                    }
                                });
                                if response.clicked(){
                                    self.selected=Some(index);
                                    self.project_view.selected_dxf=None;
                                    self.dxf_selection=None;
                                    self.dirty=true;
                                }
                            });
                        });
                    }
                    for overlay in &mut self.overlays {
                        ui.horizontal(|ui| {
                            self.dirty |= ui.checkbox(&mut overlay.visible, "").changed();
                            let label = overlay
                                .path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            let response = ui
                                .selectable_label(
                                    self.project_view.selected_dxf.as_ref() == Some(&overlay.path),
                                    label,
                                );
                            if response.has_focus() && self.project_view.selected_dxf.as_ref() == Some(&overlay.path) {
                                self.shell.focused_layer = Some((response.id, true));
                            }
                            if response.clicked() {
                                response.request_focus();
                                self.project_view.selected_dxf = Some(overlay.path.clone());
                                self.dxf_selection = None;
                                self.dirty = true;
                            }
                        });
                    }
                });
            if self.project_view.selected_dxf.is_none() && self.selected.is_some_and(|i|i<self.datasets.len()){
                if ui.button("Verwijder geselecteerde objectlaag").clicked(){
                    remove_job=self.selected;
                }
            }else if ui.add_enabled(self.project_view.selected_dxf.is_some(),
                egui::Button::new("Verwijder geselecteerde KLIC")).clicked() {
                self.remove_shell_overlay();
            }
        });
        if let Some(index)=remove_job{self.remove_job_overlay(index);}
    }

    pub(super) fn handle_layer_shortcuts(&mut self, ctx: &egui::Context) {
        let Some((id, overlay)) = self.shell.focused_layer else { return; };
        // List shortcuts are local to the focused selected row. Text editors,
        // menus, floating dialogs and a click in the map must retain their keys.
        let blocked = ctx.memory(|m| m.focused() != Some(id) || m.any_popup_open()
            || m.areas().visible_layer_ids().iter().any(|layer|
                matches!(layer.order, egui::Order::Middle | egui::Order::Foreground)));
        if blocked || self.trench_editor.is_some() || ctx.input(|i| i.modifiers != egui::Modifiers::NONE) { return; }
        // Project files can contain duplicate paths. A path selection cannot
        // identify one such row, so never delete several layers with one key.
        if overlay {
            if self.project_view.selected_dxf.as_ref().is_some_and(|path|
                self.overlays.iter().filter(|o| &o.path == path).count() != 1) { return; }
        } else if self.virtual_map.selected.is_none() {
            if self.raster_points.selected.as_ref().is_some_and(|path|
                self.geotiffs.iter().filter(|r| &r.path == path).count() != 1) { return; }
        }
        let remove = ctx.input_mut(|input|
            input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
            || input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace));
        if remove {
            if overlay { self.remove_shell_overlay(); } else { self.remove_shell_tiff(); }
            ctx.memory_mut(|m| m.surrender_focus(id));
            self.shell.focused_layer = None;
        }
    }

    pub(super) fn remove_job_overlay(&mut self,index:usize){
        if self.project_sources_owned()||self.file_intent.is_some(){
            self.message="Rond eerst de lopende projectbewerking af.".into();
            return;
        }
        if index>=self.datasets.len(){return;}
        let removed=self.datasets.remove(index);
        if !self.datasets.iter().any(|dataset|dataset.source_path==removed.source_path){
            self.project_view.hidden_object_layers.retain(|path|path!=&removed.source_path);
        }
        self.selected=self.selected.and_then(|selected|match selected.cmp(&index){
            std::cmp::Ordering::Less=>Some(selected),
            std::cmp::Ordering::Equal=>None,
            std::cmp::Ordering::Greater=>Some(selected-1),
        });
        self.dirty=true;
        self.message=format!("Objectlaag {} uit het project verwijderd.",removed.title);
    }

    fn remove_shell_overlay(&mut self) {
        if let Some(path) = self.project_view.selected_dxf.take() {
            self.overlays.retain(|o| o.path != path);
            self.dxf_selection = None;
            self.dirty = true;
        }
    }

    fn remove_shell_tiff(&mut self) {
        if self.trench_editor.is_some() {
            self.message = "Sluit eerst de proefsleufbewerking.".into();
            return;
        }
        self.cancel_virtual_map();
        self.stop_raster_rotation();
        self.raster_points.mode = None;
        if let Some(i) = self
            .virtual_map
            .selected
            .take()
            .filter(|&i| i < self.virtual_trenches.len())
        {
            self.virtual_trenches.remove(i);
            self.trench_markers.clear();
            self.refresh_virtual_references();
            self.dirty = true;
        } else if let Some(path) = self.raster_points.selected.take() {
            if let Some(i) = self.geotiffs.iter().position(|r| r.path == path) {
                self.geotiffs.remove(i);
                if i < self.raster_textures.len() {
                    self.raster_textures.remove(i);
                }
                self.navigation.raster_crops.clear();
                if let Some(loader) = &mut self.viewport_loader {
                    loader.update(&[]);
                }
                if self.geotiffs.is_empty() {
                    if let Some(loader) = &mut self.raster_loader {
                        loader.replace(Vec::new(), Default::default());
                    }
                }
                self.schedule_raster_previews();
                self.dirty = true;
            }
        }
        self.raster_points.rows.clear();
        self.project_view.selected_tiff = None;
    }

    pub(super) fn show_original_map(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(APP_BG).inner_margin(egui::Margin {
                left: 8,
                right: 18,
                top: 18,
                bottom: 18,
            }))
            .show(ctx, |ui| {
                theme::title(ui, "Kaart");
                theme::card(22, 12, 12).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.colored_label(MUTED, "GeoTIFF en KLIC");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let profile_enabled = self
                                .project_settings
                                .as_ref()
                                .unwrap_or(&self.migrated_settings)
                                .export_preferences
                                .template_show_profile_direction;
                            if ui
                                .add_enabled(
                                    profile_enabled,
                                    egui::Button::new(
                                        if self.raster_points.mode
                                            == Some(raster_point_ui::Mode::Start)
                                        {
                                            "Beginpunt sluiten"
                                        } else {
                                            "Beginpunt"
                                        },
                                    ),
                                )
                                .clicked()
                            {
                                self.toggle_shell_point(false);
                            }
                            if ui
                                .add_enabled(
                                    profile_enabled,
                                    egui::Button::new(
                                        if self.raster_points.mode
                                            == Some(raster_point_ui::Mode::Reference)
                                            || self.virtual_map.reference_active()
                                        {
                                            "Referentie sluiten"
                                        } else {
                                            "Referentiepunt"
                                        },
                                    ),
                                )
                                .clicked()
                            {
                                self.toggle_shell_point(true);
                            }
                            let enabled = self
                                .project_settings
                                .as_ref()
                                .unwrap_or(&self.migrated_settings)
                                .export_preferences
                                .template_enable_maaiveld_edit_points;
                            if ui
                                .add_enabled(
                                    enabled,
                                    egui::Button::new(if self.maaiveld_ui.active {
                                        "Maaiveld sluiten"
                                    } else {
                                        "Maaiveld invullen"
                                    }),
                                )
                                .clicked()
                            {
                                self.toggle_maaiveld_mode();
                            }
                        });
                    });
                    ui.add_space(4.);
                    let map_height = (ui.available_height() - 46.).max(40.);
                    theme::card(18, 0, 0).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), map_height),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| self.show_shell_canvas(ui),
                        );
                    });
                    ui.add_space(4.);
                    theme::card(16, 12, 6).fill(SURFACE_ALT).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.set_min_height(20.);
                        ui.spacing_mut().interact_size.y = 20.;
                        ui.horizontal(|ui| {
                            let status = if self.message.is_empty() {
                                "Laad GeoTIFF-bestanden en KLIC DXF-bestanden om te starten."
                            } else {
                                &self.message
                            };
                            let coords = ui.fonts(|fonts| {
                                fonts.layout_no_wrap(
                                    self.shell.coords.clone(),
                                    egui::FontId::proportional(theme::FONT_SIZE),
                                    DARK,
                                )
                            });
                            let width = (ui.available_width() - coords.size().x - 12.).max(0.);
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 20.),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_width(width);
                                    ui.add(egui::Label::new(status).truncate());
                                },
                            );
                            ui.colored_label(DARK, &self.shell.coords);
                        });
                    });
                });
            });
    }

    fn toggle_shell_point(&mut self, reference: bool) {
        self.close_maaiveld_mode();
        if reference {
            if let Some(i) = self.virtual_map.selected {
                if self.virtual_map.reference_active() {
                    self.cancel_virtual_map();
                } else {
                    self.start_virtual_reference(i);
                }
                return;
            }
        }
        if self.raster_points.selected.is_none() {
            self.raster_points.selected = self.geotiffs.first().map(|r| r.path.clone());
        }
        let Some(path) = self.raster_points.selected.clone() else {
            self.message = "Selecteer eerst een proefsleuf.".into();
            return;
        };
        let mode = if reference {
            raster_point_ui::Mode::Reference
        } else {
            raster_point_ui::Mode::Start
        };
        self.cancel_virtual_map();
        self.stop_raster_rotation();
        if self.raster_points.mode == Some(mode) {
            self.raster_points.mode = None;
        } else if reference {
            self.raster_points.mode = Some(mode);
            self.message =
                "Klik op de kaart om het referentiepunt te plaatsen of te verplaatsen.".into();
        } else {
            self.start_point_task(Some(path));
        }
    }
}
