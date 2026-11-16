//! Original SleufBase shell; original-shell.json records the source evidence.
use super::*;
use sleufbase_rust::project_view::BackgroundProvider as Background;
use theme::{APP_BG, MUTED, SURFACE_ALT};

#[path = "ui_list_selection.rs"]
mod list_selection;

#[path = "ui_cable_list.rs"]
mod cable_list;

#[derive(Clone, PartialEq, Eq)]
enum TrenchRow {
    Raster(PathBuf),
    Virtual(u64, String),
}

#[derive(Clone, PartialEq, Eq)]
enum OverlayRow {
    Job(PathBuf, u64),
    Drawing(PathBuf),
}

pub(super) struct State {
    pub settings_scan: Option<sleufbase_rust::recovery::ScanWorker>,
    pub settings_generation: Option<u64>,
    focused_layer: Option<(egui::Id, bool)>,
    trench_selection: list_selection::Selection<TrenchRow>,
    overlay_selection: list_selection::Selection<OverlayRow>,
    pub login_open: bool,
    pub login_focus: bool,
    pub login_remember: bool,
    pub login_email: String,
    pub login_password: String,
    // Pending secrets are retained only until the native request finishes.
    pub login_pending_account: Option<sleufbase_rust::kickthemap_session::Account>,
    pub login_saved_attempted: bool,
    pub login_is_native: bool,
    pub installed: bool,
    pub extended: bool,
    pub help: super::help_ui::State,
    pub coords: String,
    pub(super) marxact_open: bool,
    marxact_generation:u64,
    marxact_index:usize,
    marxact_angle:f64,
    tiff_height: Option<f32>,
    klic_height: Option<f32>,
    last_sidebar_height: Option<f32>,
    last_cable_selection: Option<(u64, usize, usize)>,
    cable_list: cable_list::State,
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
            trench_selection: Default::default(),
            overlay_selection: Default::default(),
            login_open: false,login_focus:false,login_remember:true,
            login_email:String::new(),login_password:String::new(),
            login_pending_account:None,login_saved_attempted:false,login_is_native:false,
            installed: false,
            extended: false,
            help: Default::default(),
            coords: "RD: -".into(),
            marxact_open:false,marxact_generation:0,
            marxact_index:0,marxact_angle:0.,
            tiff_height: None,
            klic_height: None,
            last_sidebar_height: None,
            last_cable_selection: None,
            cable_list: Default::default(),
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
    ("Laad alle maaiveldgegevens", "terrain"),
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
        .stroke(Stroke::NONE)
        // Setting a stroke enables the frame in egui, even for Stroke::NONE.
        // Disable it last so ordinary rows have no background rectangle.
        .frame(false)
        .corner_radius(5)
        .min_size(egui::vec2(ui.available_width(), 22.))
        .truncate(),
    )
}

fn sidebar_list(ui: &mut egui::Ui, id: &'static str, height: f32, content: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical().id_salt(id)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .max_height(height).min_scrolled_height(height)
        .show(ui, |ui| {
            ui.set_min_size(egui::vec2(ui.available_width(), height));
            ui.spacing_mut().button_padding = egui::vec2(3., 1.);
            ui.spacing_mut().interact_size.y = 22.;
            ui.spacing_mut().item_spacing.y = 3.;
            content(ui);
        });
}

fn selection_count(ui: &mut egui::Ui, count: usize) {
    ui.small(format!("{count} geselecteerd"))
        .on_hover_text("Ctrl+klik: toevoegen of deselecteren. Shift+klik: een reeks. Ctrl+Shift+klik: reeks toevoegen.");
}

fn remove_selection_button(ui: &mut egui::Ui, count: usize, description: &str) -> bool {
    ui.add_enabled(count > 0, egui::Button::new("Verwijder selectie"))
        .on_hover_text(description)
        .clicked()
}

/// A fixed-size switch icon, independent of whether the system font has arrows.
fn cable_switch_button(ui: &mut egui::Ui, enabled: bool) -> egui::Response {
    let response = ui.add_enabled(enabled, egui::Button::new("")
        .min_size(egui::vec2(30., 28.)).corner_radius(6));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button,
        enabled && ui.is_enabled(), "Volgend gevonden object"));
    if ui.is_rect_visible(response.rect) {
        let color = if enabled && ui.is_enabled() {
            ui.style().interact(&response).fg_stroke.color
        } else { ui.visuals().weak_text_color() };
        let center = response.rect.center();
        let stroke = egui::Stroke::new(1.4_f32, color);
        ui.painter().arrow(center + egui::vec2(-7., -4.), egui::vec2(14., 0.), stroke);
        ui.painter().arrow(center + egui::vec2(7., 4.), egui::vec2(-14., 0.), stroke);
    }
    response.on_hover_text("Volgend gevonden object selecteren")
        .on_disabled_hover_text("Zoek meerdere objecten met een kaartklik of lijnselectie om ertussen te wisselen.")
}

/// Three cable strokes crossed by a two-point selection line, drawn without
/// relying on a system-font glyph or bitmap scaling.
fn cable_line_button(ui: &mut egui::Ui, enabled: bool, active: bool, busy: bool) -> egui::Response {
    let selected = active || busy;
    let response = ui.add_enabled(enabled, egui::Button::new("")
        .selected(selected).min_size(egui::vec2(30., 28.)).corner_radius(6));
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button,
        enabled && ui.is_enabled(), selected, "Kabels met een lijn selecteren"));
    if ui.is_rect_visible(response.rect) {
        let color = if !enabled || !ui.is_enabled() { ui.visuals().weak_text_color() }
            else if selected { Color32::WHITE }
            else { ui.style().interact(&response).fg_stroke.color };
        let accent = if !enabled || !ui.is_enabled() { color }
            else if selected { Color32::WHITE } else { ORANGE };
        let center = response.rect.center();
        for y in [-5., 0., 5.] {
            ui.painter().line_segment([center + egui::vec2(-9., y), center + egui::vec2(9., y)],
                Stroke::new(1.2_f32, color));
        }
        let ends = [center + egui::vec2(-6., 8.), center + egui::vec2(6., -8.)];
        ui.painter().line_segment(ends, Stroke::new(1.8_f32, accent));
        for endpoint in ends { ui.painter().circle_filled(endpoint, 1.7, accent); }
    }
    response.on_hover_text(if busy {
        "Lijnselectie annuleren. De gekruiste kabels worden verzameld."
    } else if active {
        "Klik twee punten op de kaart. Alle kabels die de lijn kruist verschijnen in Lijst. Esc of deze knop annuleert."
    } else {
        "Kabels met een lijn selecteren: klik twee punten op de kaart. Alle gekruiste kabels verschijnen in Lijst."
    }).on_disabled_hover_text("Open de kaart met een zichtbare KLIC-tekening. Sla een open maaiveldtekst eerst op of sluit het tekstvenster.")
}

impl SleufBaseApp {
    pub(super) fn open_cable_candidate_list(&mut self) {
        self.shell.cable_list.open_and_reveal();
        self.shell.last_cable_selection = None;
    }
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
                    if ui.button("Info").on_hover_text("Uitleg per onderdeel en over SleufBase (F1)").clicked() {
                        self.shell.help.open();
                    }
                });
            });
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
            self.shell.help.open();
        }
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
            "terrain" => self.preload_all_maaiveld(),
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
        let mut cable_selection = None;
        let width = (ctx.screen_rect().width().max(1200.) * 0.255).clamp(360., 400.);
        egui::SidePanel::left("original-sidebar").default_width(width).min_width(340.)
            .frame(egui::Frame::new().fill(APP_BG).inner_margin(egui::Margin { left: 18, right: 14, top: 18, bottom: 18 }))
            .show(ctx, |ui| {
                let available = ui.available_height();
                let mut tiff = self.shell.tiff_height.unwrap_or((available.max(620.) * 0.30).clamp(220., 280.));
                let mut klic = self.shell.klic_height.unwrap_or((available.max(620.) * 0.30).clamp(190., 225.));
                if let Some(previous)=self.shell.last_sidebar_height {
                    let delta=available-previous;
                    tiff+=delta*2./8.; klic+=delta/8.;
                }
                self.shell.last_sidebar_height=Some(available);
                tiff = tiff.min((available - 310.).max(140.));
                klic = klic.min((available - tiff - 110.).max(150.));
                self.show_shell_tiffs(ui, tiff);
                // Selecting another TIFF can start its point-preparation worker,
                // which temporarily owns every source. Keep the other lists intact.
                if self.export_intent.is_some() { return; }
                separator(ui, &mut tiff, 140., available - klic - 110., "tiff-sash");
                self.show_shell_overlays(ui, klic);
                separator(ui, &mut klic, 150., available - tiff - 110., "klic-sash");
                self.shell.tiff_height = Some(tiff); self.shell.klic_height = Some(klic);
                let cable_matches = cable_list::visible_matches(&self.overlays, &self.found_layers);
                let list_label = format!("Lijst ({})", cable_matches.len());
                let list_width = egui::WidgetText::from(list_label.clone()).into_galley(ui,
                    Some(egui::TextWrapMode::Extend), f32::INFINITY,
                    egui::FontSelection::Style(egui::TextStyle::Button)).size().x
                    + 2. * ui.spacing().button_padding.x;
                let list_width = list_width.ceil().max(70.);
                let controls_width = 30. + 6. + list_width + 6. + 30.;
                let available_width = ui.available_width();
                let named = egui::FontFamily::Name("SleufBase title".into());
                let family = if ui.fonts(|fonts| fonts.families().contains(&named)) {
                    named
                } else { egui::FontFamily::Proportional };
                let title = egui::RichText::new("Kabelinformatie")
                    .font(egui::FontId::new(theme::FONT_SIZE, family));
                let title_width = egui::WidgetText::from(title).into_galley(ui,
                    Some(egui::TextWrapMode::Extend), f32::INFINITY,
                    egui::FontSelection::Style(egui::TextStyle::Button)).size().x + 4.;
                let line_active = self.cable_line.active();
                let line_busy = self.cable_line.busy();
                let line_enabled = line_active || line_busy || (!self.browser_ui.web_visible
                    && !self.maaiveld_ui.has_draft() && self.overlays.iter().any(|overlay| overlay.visible));
                let (mut swap, mut toggle_list, mut toggle_line) = (false, false, false);
                let mut controls = |ui: &mut egui::Ui| {
                    ui.spacing_mut().item_spacing.x = 6.;
                    swap = cable_switch_button(ui, cable_matches.len() > 1).clicked();
                    toggle_list = ui.add_enabled_ui(!self.browser_ui.web_visible, |ui|
                        ui.add_sized([list_width, 28.], egui::Button::new(&list_label)
                            .selected(self.shell.cable_list.open).corner_radius(6)
                            .wrap_mode(egui::TextWrapMode::Extend)))
                        .inner.on_hover_text("Open of sluit de objectlijst rechts naast de kaart")
                        .on_disabled_hover_text("Ga terug naar de kaart om de objectlijst te openen.").clicked();
                    toggle_line = cable_line_button(ui, line_enabled, line_active, line_busy).clicked();
                };
                if available_width < title_width + controls_width + 8. {
                    // Keep both rows left-aligned when the complete header does not fit.
                    theme::title(ui, "Kabelinformatie");
                    ui.horizontal(|ui| {
                        ui.add_space(4.);
                        controls(ui);
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.;
                        // Use the same natural-width title as the other panes. A
                        // stretched add_sized label centers its widget in the spare space.
                        theme::title(ui, "Kabelinformatie");
                        controls(ui);
                    });
                }
                if swap {
                    let current = self.dxf_selection.as_ref().and_then(|selection| cable_matches.iter()
                        .position(|&(overlay, feature)| selection.overlay == overlay && selection.feature == feature));
                    let next = current.map_or(0, |index| (index + 1) % cable_matches.len());
                    cable_selection = cable_matches.get(next).copied();
                }
                if toggle_list {
                    self.shell.cable_list.toggle();
                    ctx.request_repaint();
                }
                if toggle_line {
                    self.toggle_cable_line_selection();
                    ctx.request_repaint();
                }
                // Commit a switch before painting the details; the right-hand
                // list and map halo subsequently read this same selection.
                if let Some((overlay, feature)) = cable_selection.take() {
                    self.select_dxf(overlay, feature);
                    self.shell.last_cable_selection = None;
                    ctx.request_repaint();
                }
                let selection_key = self.dxf_selection.as_ref()
                    .map(|selection| (self.project_generation, selection.overlay, selection.feature));
                let reset_cable_scroll = self.shell.last_cable_selection != selection_key;
                self.shell.last_cable_selection = selection_key;
                let height = (ui.available_height() - 30.).max(40.);
                theme::card(18, 14, 14).show(ui, |ui| {
                    let width = ui.available_width();
                    ui.set_width(width);
                    ui.set_min_height(height);
                    // Fill the card instead of letting the scrollbar follow the
                    // width of whichever detail value happens to be longest.
                    let scroll = egui::ScrollArea::vertical().id_salt("cable-information")
                        .auto_shrink([false, false]).max_width(width)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                        .max_height(height).min_scrolled_height(height);
                    let scroll = if reset_cable_scroll {
                        // Reset inertia/animation too; an old scroll target must
                        // not move the newly selected object's first fields away.
                        let id = ui.make_persistent_id(egui::Id::new("cable-information"));
                        egui::scroll_area::State::default().store(ui.ctx(), id);
                        scroll.vertical_scroll_offset(0.)
                    } else { scroll };
                    scroll.show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if let Some(selection) = &self.dxf_selection {
                            let priority = |key: &str| {
                                if key.eq_ignore_ascii_case("XDATA InfraCAD") { 0 }
                                else if key == "Laag" { 1 }
                                else { 2 }
                            };
                            for group in 0..3 {
                                for (key, value) in selection.details.iter().filter(|(key, _)| priority(key) == group) {
                                    theme::strong(ui,cad_preview(key));
                                    let preview=cad_preview(value);
                                    ui.add(egui::Label::new(preview).wrap());
                                    if preview.len()<value.len() {
                                        if ui.small_button("Volledige tekst kopiëren").clicked(){ui.ctx().copy_text(value.clone());}
                                    }
                                }
                            }
                        } else { ui.label("Klik in de kaart op een kabel om KLIC-informatie en laagnamen te tonen.\n\nGebruik de rechtermuisknop op een proefsleuf in de lijst om draaien en opmerkingen te beheren.\n\nTip: zoom verder in voor een preciezere selectie als kabels dicht op elkaar liggen."); }
                    });
                });
            });
    }

    fn trench_rows(&self) -> Vec<TrenchRow> {
        self.geotiffs.iter().map(|r| TrenchRow::Raster(r.path.clone()))
            .chain(self.virtual_trenches.iter().map(|t| TrenchRow::Virtual(t.job_id, t.label.clone())))
            .collect()
    }

    fn overlay_rows(&self) -> Vec<OverlayRow> {
        self.datasets.iter().map(|d| OverlayRow::Job(d.source_path.clone(), d.job_id))
            .chain(self.overlays.iter().map(|o| OverlayRow::Drawing(o.path.clone())))
            .collect()
    }

    fn sync_trench_selection(&mut self) {
        let rows = self.trench_rows();
        let preferred = self.shell.trench_selection.is_current(self.project_generation, &rows)
            .then(|| self.shell.trench_selection.active()).flatten();
        let active = if let Some(index) = self.virtual_map.selected.filter(|&i| i < self.virtual_trenches.len()) {
            Some(self.geotiffs.len() + index)
        } else if let Some(path) = self.raster_points.selected.as_ref() {
            preferred.filter(|&i| self.geotiffs.get(i).is_some_and(|r| &r.path == path))
                .or_else(|| {
                    let mut matches = self.geotiffs.iter().enumerate().filter(|(_, r)| &r.path == path);
                    let first = matches.next().map(|(i, _)| i);
                    if matches.next().is_none() { first } else { None }
                })
        } else { None };
        self.shell.trench_selection.sync(self.project_generation, rows, active);
    }

    fn sync_overlay_selection(&mut self) {
        let rows = self.overlay_rows();
        let preferred = self.shell.overlay_selection.is_current(self.project_generation, &rows)
            .then(|| self.shell.overlay_selection.active()).flatten();
        let active = if let Some(path) = self.project_view.selected_dxf.as_ref() {
            preferred.filter(|&i| i >= self.datasets.len()
                && self.overlays.get(i - self.datasets.len()).is_some_and(|o| &o.path == path))
                .or_else(|| {
                    let mut matches = self.overlays.iter().enumerate().filter(|(_, o)| &o.path == path);
                    let first = matches.next().map(|(i, _)| self.datasets.len() + i);
                    if matches.next().is_none() { first } else { None }
                })
        } else { self.selected.filter(|&i| i < self.datasets.len()) };
        self.shell.overlay_selection.sync(self.project_generation, rows, active);
    }

    fn apply_trench_list_selection(&mut self) {
        let old_virtual = self.virtual_map.selected;
        let old_raster = self.raster_points.selected.clone();
        let old_point_mode = self.raster_points.mode;
        let active = self.shell.trench_selection.active();
        let selected_virtual = active.and_then(|i| i.checked_sub(self.geotiffs.len()))
            .filter(|&i| i < self.virtual_trenches.len());
        let selected_raster = active.and_then(|i| self.geotiffs.get(i)).map(|r| r.path.clone());
        // Selecting an already active row must keep its prepared point markers.
        if selected_virtual == old_virtual && selected_raster == old_raster { return; }
        self.cancel_virtual_map();
        self.stop_raster_rotation();
        self.raster_points.mode = None;
        self.raster_points.rows.clear();
        self.virtual_map.selected = selected_virtual;
        self.raster_points.selected = selected_raster;
        self.update_view_selection(old_virtual, old_raster.as_deref());
        self.resume_point_mode_after_selection(old_point_mode);
        self.maaiveld_selection_changed();
    }

    fn apply_overlay_list_selection(&mut self) {
        let old = self.project_view.selected_dxf.clone();
        match self.shell.overlay_selection.active() {
            Some(i) if i < self.datasets.len() => {
                self.selected = Some(i);
                self.project_view.selected_dxf = None;
            }
            Some(i) => {
                self.project_view.selected_dxf = self.overlays.get(i - self.datasets.len()).map(|o| o.path.clone());
            }
            None => {
                self.selected = None;
                self.project_view.selected_dxf = None;
            }
        }
        self.dxf_selection = None;
        self.found_layers.clear();
        self.dirty |= old != self.project_view.selected_dxf;
    }

    fn show_shell_tiffs(&mut self, ui: &mut egui::Ui, height: f32) {
        self.sync_trench_selection();
        theme::title(ui, "Proefsleuven");
        let mut contexts = Vec::new();
        let mut zoom_target = None;
        let mut remove = false;
        let mut visibility = Vec::new();
        theme::card(18, 14, 14).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let list_height = (height - 110.).max(28.);
            let mut clicked = None;
            sidebar_list(ui, "original-tiffs", list_height, |ui| {
                        for (i, raster) in self.geotiffs.iter().enumerate() {
                            let label = raster.path.file_name().unwrap_or_default().to_string_lossy();
                            let response = ui.push_id(("raster-row", i), |ui| {
                                ui.horizontal(|ui| {
                                    let mut visible = layer_visibility_ui::raster_is_visible(raster);
                                    if ui.checkbox(&mut visible, "").on_hover_text("Toon deze proefsleuf op de werkkaart").changed() {
                                        visibility.push((false, i, visible));
                                    }
                                    tiff_row(ui, self.shell.trench_selection.contains(i), label.as_ref())
                                }).inner
                            }).inner;
                            if response.clicked() || response.secondary_clicked() {
                                response.request_focus();
                                clicked = Some((i, response.secondary_clicked(), ui.input(|i| i.modifiers)));
                            }
                            if response.has_focus() {
                                self.shell.focused_layer = Some((response.id, false));
                            }
                            if response.double_clicked() { zoom_target = Some(raster.bounds); }
                            contexts.push((i, response.clone()));
                            if let Some(error) = self.raster_errors.get(i).and_then(|e| e.as_ref()) {
                                response.on_hover_text(error);
                            }
                        }
                        for (i, trench) in self.virtual_trenches.iter().enumerate() {
                            let row = self.geotiffs.len() + i;
                            let response = ui.push_id(("virtual-row", i), |ui| {
                                ui.horizontal(|ui| {
                                    let mut visible = layer_visibility_ui::trench_is_visible(trench);
                                    if ui.checkbox(&mut visible, "").on_hover_text("Toon deze proefsleuf op de werkkaart").changed() {
                                        visibility.push((true, i, visible));
                                    }
                                    tiff_row(ui, self.shell.trench_selection.contains(row), &trench.label)
                                }).inner
                            }).inner;
                            if response.clicked() || response.secondary_clicked() {
                                response.request_focus();
                                clicked = Some((row, response.secondary_clicked(), ui.input(|i| i.modifiers)));
                            }
                            if response.has_focus() {
                                self.shell.focused_layer = Some((response.id, false));
                            }
                            if response.double_clicked() {
                                let mut bounds = None;
                                for (x, y) in trench.measured_boundary() {
                                    bounds = union_bounds(bounds, Some(Bounds {min_x:x, min_y:y, max_x:x, max_y:y}));
                                }
                                zoom_target = bounds;
                            }
                            contexts.push((row, response));
                        }
            });
            if let Some((row, secondary, modifiers)) = clicked {
                if self.maaiveld_ui.has_draft() {
                    self.message = "Sla de maaiveldtekst op of sluit het invulvenster voordat je een andere proefsleuf kiest.".into();
                } else {
                    if secondary { self.shell.trench_selection.context(row); }
                    else { self.shell.trench_selection.click(row, modifiers.ctrl || modifiers.command, modifiers.shift); }
                    self.apply_trench_list_selection();
                }
            }
            selection_count(ui, self.shell.trench_selection.len());
            ui.horizontal_wrapped(|ui| {
                if ui.button("Virtuele proefsleuf").clicked() { self.start_virtual_creation(); }
                remove |= remove_selection_button(ui, self.shell.trench_selection.len(),
                    "Verwijder alle geselecteerde proefsleuven uit dit project");
            });
        });
        for (virtual_trench, index, visible) in visibility {
            if virtual_trench { self.set_trench_visibility(index, visible); }
            else { self.set_raster_visibility(index, visible); }
        }
        if let Some(bounds) = zoom_target {
            self.navigation.fit_bounds = Some(bounds);
            self.navigation.viewport = None;
            self.zoom = 1.; self.pan = egui::Vec2::ZERO;
            self.dirty = true;
        }
        // Deletion is deferred until all row menus have used the original indices.
        for (row, response) in contexts {
            response.context_menu(|ui| {
                if row < self.geotiffs.len() {
                    self.show_raster_trench_context(ui, row);
                } else if row - self.geotiffs.len() < self.virtual_trenches.len() {
                    self.show_virtual_tiff_context(ui, row - self.geotiffs.len());
                }
                ui.separator();
                if ui.button(format!("Verwijder selectie ({})", self.shell.trench_selection.len())).clicked() {
                    remove = true;
                    ui.close_menu();
                }
            });
        }
        if remove { self.remove_shell_tiff(); }
    }

    fn show_shell_overlays(&mut self, ui: &mut egui::Ui, height: f32) {
        self.sync_overlay_selection();
        theme::title(ui, "Overlays");
        let mut remove = false;
        let mut contexts = Vec::new();
        theme::card(18, 14, 14).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let list_height = (height - 110.).max(24.);
            let mut clicked = None;
            sidebar_list(ui, "original-overlays", list_height, |ui| {
                    for (index, job) in self.datasets.iter().enumerate() {
                        ui.push_id(("job-object-overlay", index), |ui| {
                            ui.horizontal(|ui| {
                                let mut visible = !self.project_view.hidden_object_layers.contains(&job.source_path);
                                if ui.checkbox(&mut visible, "").on_hover_text("Toon objectpunten en objectlijnen op de kaart").changed() {
                                    self.project_view.hidden_object_layers.retain(|p| p != &job.source_path);
                                    if !visible { self.project_view.hidden_object_layers.push(job.source_path.clone()); }
                                    self.dirty = true;
                                }
                                let response = tiff_row(ui, self.shell.overlay_selection.contains(index),
                                    &format!("Job: {} ({} objecten)", job.title, job.features.len()))
                                    .on_hover_text(format!("KickTheMap-objecten uit {}", job.source_path.display()));
                                if response.clicked() || response.secondary_clicked() {
                                    response.request_focus();
                                    clicked = Some((index, response.secondary_clicked(), ui.input(|i| i.modifiers)));
                                }
                                if response.has_focus() { self.shell.focused_layer = Some((response.id, true)); }
                                contexts.push(response);
                            });
                        });
                    }
                    for (index, overlay) in self.overlays.iter_mut().enumerate() {
                        let row = self.datasets.len() + index;
                        ui.push_id(("drawing-overlay", index), |ui| {
                            ui.horizontal(|ui| {
                                self.dirty |= ui.checkbox(&mut overlay.visible, "").changed();
                                let label = overlay.path.file_name().unwrap_or_default().to_string_lossy();
                                let response = tiff_row(ui, self.shell.overlay_selection.contains(row), label.as_ref());
                                if response.clicked() || response.secondary_clicked() {
                                    response.request_focus();
                                    clicked = Some((row, response.secondary_clicked(), ui.input(|i| i.modifiers)));
                                }
                                if response.has_focus() { self.shell.focused_layer = Some((response.id, true)); }
                                contexts.push(response);
                            });
                        });
                    }
                });
            if let Some((row, secondary, modifiers)) = clicked {
                if secondary { self.shell.overlay_selection.context(row); }
                else { self.shell.overlay_selection.click(row, modifiers.ctrl || modifiers.command, modifiers.shift); }
                self.apply_overlay_list_selection();
            }
            selection_count(ui, self.shell.overlay_selection.len());
            ui.horizontal_wrapped(|ui| {
                // Wrap whole controls together; never split the short labels.
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                remove |= remove_selection_button(ui, self.shell.overlay_selection.len(),
                    "Verwijder alle geselecteerde overlays uit dit project");
                let visible_count = {
                    let hidden: std::collections::HashSet<_> = self.project_view.hidden_object_layers.iter().collect();
                    self.datasets.iter().filter(|job| !hidden.contains(&job.source_path)).count()
                };
                let count = self.datasets.len();
                let mixed = visible_count > 0 && visible_count < count;
                let mut all_visible = count > 0 && visible_count == count;
                let description = if mixed {
                    "Een deel van de jobpunten is zichtbaar. Klik om alle jobpunten en objectlijnen te tonen."
                } else if all_visible {
                    "Verberg alle jobpunten en objectlijnen op de werkkaart."
                } else {
                    "Toon alle jobpunten en objectlijnen op de werkkaart."
                };
                if ui.add_enabled(count > 0, egui::Checkbox::new(&mut all_visible, "Jobpunten")
                    .indeterminate(mixed)).on_hover_text(description)
                    .on_disabled_hover_text("Laad een KickTheMap-job om objectpunten te tonen.").changed()
                {
                    let mut job_paths: std::collections::HashSet<_> = self.datasets.iter().map(|job| &job.source_path).collect();
                    if all_visible {
                        self.project_view.hidden_object_layers.retain(|path| !job_paths.contains(path));
                    } else {
                        for path in &self.project_view.hidden_object_layers { job_paths.remove(path); }
                        for job in &self.datasets {
                            if job_paths.remove(&job.source_path) {
                                self.project_view.hidden_object_layers.push(job.source_path.clone());
                            }
                        }
                    }
                    self.dirty = true;
                }
            });
        });
        for response in contexts {
            response.context_menu(|ui| {
                if ui.button(format!("Verwijder selectie ({})", self.shell.overlay_selection.len())).clicked() {
                    remove = true;
                    ui.close_menu();
                }
            });
        }
        if remove { self.remove_shell_overlay(); }
    }

    pub(super) fn handle_layer_shortcuts(&mut self, ctx: &egui::Context) {
        let Some((id, overlay)) = self.shell.focused_layer else { return; };
        // Keep Delete/Backspace local to the focused list; dialogs and editors
        // retain their own keys, even while a multiple selection remains visible.
        let blocked = ctx.memory(|m| m.focused() != Some(id) || m.any_popup_open()
            || m.areas().visible_layer_ids().iter().any(|layer|
                matches!(layer.order, egui::Order::Middle | egui::Order::Foreground)));
        if blocked || self.trench_editor.is_some() || ctx.input(|i| i.modifiers != egui::Modifiers::NONE) { return; }
        let remove = ctx.input_mut(|input|
            input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
            || input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace));
        if remove {
            if overlay { self.remove_shell_overlay(); } else { self.remove_shell_tiff(); }
            ctx.memory_mut(|m| m.surrender_focus(id));
            self.shell.focused_layer = None;
        }
    }

    fn can_remove_shell_layers(&mut self) -> bool {
        if self.project_sources_owned() || self.file_intent.is_some() {
            self.message = "Rond eerst de lopende projectbewerking af.".into();
            return false;
        }
        if self.trench_editor.is_some() {
            self.message = "Sluit eerst de proefsleufbewerking.".into();
            return false;
        }
        true
    }

    #[cfg(test)]
    pub(super) fn remove_job_overlay(&mut self, index: usize) {
        if !self.can_remove_shell_layers() || index >= self.datasets.len() { return; }
        let removed = self.datasets.remove(index);
        if !self.datasets.iter().any(|dataset| dataset.source_path == removed.source_path) {
            self.project_view.hidden_object_layers.retain(|path| path != &removed.source_path);
        }
        self.selected = self.selected.and_then(|selected| match selected.cmp(&index) {
            std::cmp::Ordering::Less => Some(selected),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(selected - 1),
        });
        self.shell.overlay_selection = Default::default();
        self.close_maaiveld_mode();
        self.dirty = true;
        self.message = format!("Objectlaag {} uit het project verwijderd.", removed.title);
    }

    fn remove_shell_overlay(&mut self) {
        if !self.can_remove_shell_layers() { return; }
        self.sync_overlay_selection();
        let selected = self.shell.overlay_selection.indices();
        if selected.is_empty() { return; }
        let dataset_count = self.datasets.len();
        let mut removed_paths = Vec::new();
        // Exact row indices also distinguish duplicate file paths. Work from
        // the bottom upwards so deleting one source never retargets the next.
        for &row in selected.iter().rev() {
            if row >= dataset_count {
                self.overlays.remove(row - dataset_count);
            } else {
                removed_paths.push(self.datasets.remove(row).source_path);
            }
        }
        for path in removed_paths {
            if !self.datasets.iter().any(|dataset| dataset.source_path == path) {
                self.project_view.hidden_object_layers.retain(|p| p != &path);
            }
        }
        self.selected = None;
        self.project_view.selected_dxf = None;
        self.dxf_selection = None;
        self.found_layers.clear();
        self.shell.overlay_selection = Default::default();
        self.shell.focused_layer = None;
        self.close_maaiveld_mode();
        self.dirty = true;
        self.message = format!("{} overlay(s) uit het project verwijderd.", selected.len());
    }

    fn remove_shell_tiff(&mut self) {
        if !self.can_remove_shell_layers() { return; }
        self.sync_trench_selection();
        let selected = self.shell.trench_selection.indices();
        if selected.is_empty() { return; }
        let raster_count = self.geotiffs.len();
        let remove_virtual = selected.iter().any(|&i| i >= raster_count);
        let remove_raster = selected.iter().any(|&i| i < raster_count);
        self.cancel_virtual_map();
        self.stop_raster_rotation();
        self.raster_points.mode = None;
        self.close_maaiveld_mode();
        if remove_virtual {
            // Cancel every index-based draft and pending height reply before
            // compacting the trench vector, then resume surviving height work.
            self.virtual_map = Default::default();
            if let Some(worker) = &mut self.pending_ahn { worker.cancel(); }
            self.shell.marxact_open = false;
        }
        for &row in selected.iter().rev() {
            if row >= raster_count {
                self.virtual_trenches.remove(row - raster_count);
            } else {
                self.geotiffs.remove(row);
                if row < self.raster_textures.len() { self.raster_textures.remove(row); }
                if row < self.raster_errors.len() { self.raster_errors.remove(row); }
            }
        }
        if remove_virtual {
            self.trench_markers.clear();
            self.refresh_virtual_references();
            self.resume_pending_ahn();
        }
        if remove_raster {
            self.navigation.raster_crops.clear();
            if let Some(loader) = &mut self.viewport_loader { loader.update(&[]); }
            if self.geotiffs.is_empty() {
                if let Some(loader) = &mut self.raster_loader { loader.replace(Vec::new(), Default::default()); }
            }
            self.schedule_raster_previews();
            self.tiff_comment_editor = None;
            self.tiff_addresses.reset();
        }
        self.virtual_map.selected = None;
        self.raster_points.selected = None;
        self.raster_points.rows.clear();
        self.project_view.selected_tiff = None;
        self.shell.trench_selection = Default::default();
        self.shell.focused_layer = None;
        self.dirty = true;
        self.message = if selected.len() == 1 { "Proefsleuf uit het project verwijderd.".into() }
            else { format!("{} proefsleuven uit het project verwijderd.", selected.len()) };
    }

    fn show_shell_map_display_controls(&mut self, ui: &mut egui::Ui) {
        let previous = self.project_view.clone();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.;
            let field_width = (ui.available_width() - 40.).clamp(100., 220.);
            ui.horizontal(|ui| {
                ui.add_sized([32., 28.], egui::Label::new("Kaart").halign(egui::Align::Min));
                egui::ComboBox::from_id_salt("original-background").width(field_width).truncate()
                    .selected_text(self.project_view.provider().label()).show_ui(ui, |ui| {
                        for source in [Background::PdokAerial, Background::PdokAerialCadastral,
                            Background::Cadastral, Background::OpenStreetMap, Background::OpenStreetMapCadastral]
                        {
                            if ui.selectable_label(self.project_view.use_background
                                && self.project_view.provider() == source, source.label()).clicked()
                            {
                                self.project_view.use_background = true;
                                self.project_view.background_source = source.label().into();
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.add_sized([32., 28.], egui::Label::new("TIFF").halign(egui::Align::Min));
                ui.spacing_mut().slider_width = 130.;
                ui.add(egui::Slider::new(&mut self.project_view.opacity_percent, 0. ..=100.).show_value(false));
            });
        });
        if previous != self.project_view {
            self.dirty = true;
            if previous.use_background != self.project_view.use_background
                || previous.background_source != self.project_view.background_source
            {
                self.request_selected_background();
            }
        }
    }

    pub(super) fn show_original_map(&mut self, ctx: &egui::Context) {
        if self.shell.cable_list.open && !self.browser_ui.web_visible {
            let candidates = cable_list::visible_matches(&self.overlays, &self.found_layers);
            let selected = self.dxf_selection.as_ref().map(|selection| (selection.overlay, selection.feature));
            if let Some((overlay, feature)) = cable_list::show(ctx, &mut self.shell.cable_list,
                self.project_generation, &self.overlays, &candidates, selected)
            {
                self.select_dxf(overlay, feature);
                self.shell.last_cable_selection = None;
                ctx.request_repaint();
            }
        }
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
                    let controls_inline = ui.available_width() >= 940.;
                    if !controls_inline {
                        self.show_shell_map_display_controls(ui);
                        ui.add_space(2.);
                    }
                    ui.horizontal(|ui| {
                        if controls_inline {
                            ui.allocate_ui_with_layout(egui::vec2(440., 28.),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| self.show_shell_map_display_controls(ui));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true), |ui| {
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
                            if ui.add(egui::Button::new("Proefsleufnaam")
                                .selected(self.migrated_settings.viewer.show_trench_labels).corner_radius(6))
                                .on_hover_text("Namen van proefsleuven op de werkkaart tonen of verbergen")
                                .clicked()
                            {
                                self.toggle_shell_trench_names();
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

    fn toggle_shell_trench_names(&mut self) {
        let mut settings = self.migrated_settings.clone();
        settings.viewer.show_trench_labels = !settings.viewer.show_trench_labels;
        match settings_store::save_pair(&project::app_data_directory(), &settings, &self.backup_settings) {
            Ok(()) => {
                self.migrated_settings = settings;
                if let Some(dialog) = &mut self.settings_dialog {
                    dialog.draft.viewer.show_trench_labels = self.migrated_settings.viewer.show_trench_labels;
                }
                self.message = if self.migrated_settings.viewer.show_trench_labels {
                    "Proefsleufnamen op de werkkaart ingeschakeld."
                } else {
                    "Proefsleufnamen op de werkkaart uitgeschakeld."
                }.into();
            }
            Err(error) => self.message = format!("Weergave van proefsleufnamen niet opgeslagen: {error}"),
        }
    }

    fn toggle_shell_point(&mut self, reference: bool) {
        if self.maaiveld_ui.has_draft() {
            self.message = "Sla de maaiveldtekst op of sluit het invulvenster voordat je een ander gereedschap kiest.".into();
            return;
        }
        if self.virtual_map.selected.and_then(|i| self.virtual_trenches.get(i))
            .is_some_and(|trench| !layer_visibility_ui::trench_is_visible(trench))
            || self.raster_points.selected.as_ref().is_some_and(|path|
                self.geotiffs.iter().find(|r| &r.path == path)
                    .is_some_and(|raster| !layer_visibility_ui::raster_is_visible(raster)))
        {
            self.message = "Maak deze proefsleuf eerst zichtbaar.".into();
            return;
        }
        self.close_maaiveld_mode();
        if !reference && self.virtual_map.selected.is_some() {
            self.resume_point_mode_after_selection(Some(raster_point_ui::Mode::Start));
            return;
        }
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
            self.raster_points.selected = self.geotiffs.iter()
                .find(|r| layer_visibility_ui::raster_is_visible(r)).map(|r| r.path.clone());
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
