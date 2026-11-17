//! Draft edits commit only on Exporteren. Stable source paths and project
//! generation protect a dialog from reordered jobs or a replaced project.
use eframe::egui;
use sleufbase_rust::{
    geotiff::GeoTiffInfo,
    settings_compat::{MigratedSettings, ProfilePreferences},
    template_order::{self, Row, SortMode, State},
    template_project::{BackgroundKind, Options},
    Dataset,
};
/// Only settings-owned defaults belong in this snapshot. Drawing metadata,
/// source order, font folders and export-specific scale/DPI stay in the draft.
#[derive(Clone)]
pub(crate) struct ExportDefaults {
    pub overview: sleufbase_rust::cadastral_overview::Settings,
    profile_style: ProfilePreferences,
    logos: sleufbase_rust::template_branding::Logos,
    pub contact: sleufbase_rust::template_branding::ContactDetails,
    template: std::path::PathBuf,
    paired: bool,
    pub profiles: bool,
    background: BackgroundKind,
    include_cad: bool,
    label_gap: f64,
}
fn refresh_unchanged<T: Clone + PartialEq>(value: &mut T, opening: &T, current: &T) {
    if current != opening && value == opening { *value = current.clone(); }
}
impl ExportDefaults {
    pub fn from_settings(settings: &MigratedSettings) -> Result<Self, String> {
        let preferences = &settings.export_preferences;
        Ok(Self {
            overview: sleufbase_rust::cadastral_overview::Settings::from_settings(settings)?,
            profile_style: ProfilePreferences::from_settings(settings),
            logos: crate::ui_preferences::logos(preferences),
            contact: preferences.template_contact.clone(),
            template: preferences.cadastral_template_path.clone().into(),
            paired: preferences.template_generate_reverse_cross_section_sheet,
            profiles: preferences.template_include_cross_sections,
            background: BackgroundKind::from_setting(&settings.map_export.background)?,
            include_cad: settings.map_export.include_cad,
            label_gap: settings.dxf_label_gap,
        })
    }
    pub fn apply_project(&self, options: &mut Options) {
        options.overview_settings = self.overview.clone();
        options.profile_style = self.profile_style.clone();
        options.logos = self.logos.clone();
        options.contact = self.contact.clone();
        options.template = self.template.clone();
        options.paired = self.paired;
        options.background = self.background;
        options.include_cad = self.include_cad;
    }
    pub fn refresh_project(&self, opening: &Self, options: &mut Options) {
        // The overview has no separate controls in the template dialog.
        options.overview_settings = self.overview.clone();
        options.contact = self.contact.clone();
        refresh_unchanged(&mut options.template, &opening.template, &self.template);
        refresh_unchanged(&mut options.paired, &opening.paired, &self.paired);
        refresh_unchanged(&mut options.background, &opening.background, &self.background);
        refresh_unchanged(&mut options.include_cad, &opening.include_cad, &self.include_cad);
        refresh_unchanged(&mut options.logos.techbase, &opening.logos.techbase, &self.logos.techbase);
        refresh_unchanged(&mut options.logos.client, &opening.logos.client, &self.logos.client);
        macro_rules! profile {
            ($($field:ident),+ $(,)?) => {$(
                refresh_unchanged(&mut options.profile_style.$field,
                    &opening.profile_style.$field, &self.profile_style.$field);
            )+};
        }
        profile!(marker_diameter, show_reference, use_custom_maaiveld,
            auto_fill_bgt, avoid_collisions, clip_markers, diameter_prefix);
    }
    fn apply_order(&self, state: &mut State) {
        state.overview_settings = self.overview.clone();
        state.template = (!self.template.as_os_str().is_empty()).then(|| self.template.clone());
        state.logos = self.logos.clone();
        state.contact = self.contact.clone();
        state.profiles = self.profiles;
        state.paired = self.paired;
        state.map_background = Some(self.background);
        state.aerial = matches!(self.background, BackgroundKind::Pdok | BackgroundKind::PdokCadastral);
        state.include_cad = self.include_cad;
        state.marker_diameter = self.profile_style.marker_diameter;
        state.show_reference = self.profile_style.show_reference;
        state.use_custom_maaiveld = self.profile_style.use_custom_maaiveld;
        state.auto_bgt = self.profile_style.auto_fill_bgt;
        state.avoid_collisions = self.profile_style.avoid_collisions;
        state.clip_markers = self.profile_style.clip_markers;
        state.diameter_prefix = self.profile_style.diameter_prefix.clone();
        state.label_gap = self.label_gap;
    }
    fn refresh_order(&self, opening: &Self, state: &mut State) {
        state.overview_settings = self.overview.clone();
        state.contact = self.contact.clone();
        state.label_gap = self.label_gap;
        let template = |defaults: &Self| (!defaults.template.as_os_str().is_empty())
            .then(|| defaults.template.clone());
        refresh_unchanged(&mut state.template, &template(opening), &template(self));
        refresh_unchanged(&mut state.profiles, &opening.profiles, &self.profiles);
        refresh_unchanged(&mut state.paired, &opening.paired, &self.paired);
        let mut background = state.effective_background();
        refresh_unchanged(&mut background, &opening.background, &self.background);
        state.map_background = Some(background);
        state.aerial = matches!(background, BackgroundKind::Pdok | BackgroundKind::PdokCadastral);
        refresh_unchanged(&mut state.include_cad, &opening.include_cad, &self.include_cad);
        refresh_unchanged(&mut state.logos.techbase, &opening.logos.techbase, &self.logos.techbase);
        refresh_unchanged(&mut state.logos.client, &opening.logos.client, &self.logos.client);
        macro_rules! profile {
            ($($state:ident => $field:ident),+ $(,)?) => {$(
                refresh_unchanged(&mut state.$state,
                    &opening.profile_style.$field, &self.profile_style.$field);
            )+};
        }
        profile!(marker_diameter => marker_diameter, show_reference => show_reference,
            use_custom_maaiveld => use_custom_maaiveld, auto_bgt => auto_fill_bgt,
            avoid_collisions => avoid_collisions, clip_markers => clip_markers,
            diameter_prefix => diameter_prefix);
    }
}
/// Called just before export, while the original dialog baseline still exists.
pub(crate) fn refresh_state(state: &mut State, dialog: Option<&Dialog>, settings: &MigratedSettings) -> Result<(), String> {
    let current = ExportDefaults::from_settings(settings)?;
    state.overview_settings = current.overview.clone();
    state.contact = current.contact.clone();
    state.label_gap = current.label_gap;
    if let Some(opening) = dialog.and_then(|dialog| dialog.opening_defaults.as_ref()) {
        current.refresh_order(opening, state);
    }
    Ok(())
}
pub(crate) struct Dialog {
    pub state: State,
    pub error: String,
    selected: Vec<bool>,
    generation: u64,
    anchor: usize,
    number: String,
    variant: String,
    scale: String,
    sort_mode: Option<SortMode>,
    reveal_row: Option<usize>,
    opening_defaults: Option<ExportDefaults>,
}
pub(crate) fn logos(ui:&mut egui::Ui, logos:&mut sleufbase_rust::template_branding::Logos) {
    ui.collapsing("Logo's op de bladen",|ui| {
        for (label,path) in [("Techbase-logo",&mut logos.techbase),("Opdrachtgeverlogo",&mut logos.client)] {
            ui.horizontal(|ui| {
                ui.label(label);
                if ui.button("Kiezen…").clicked() {
                    if let Some(chosen)=rfd::FileDialog::new().add_filter("Afbeeldingen",&["png","jpg","jpeg"]).pick_file() {*path=Some(chosen);}
                }
                if path.is_some() && ui.small_button("Verwijderen").clicked() {*path=None;}
            });
            if let Some(path)=path {ui.small(path.display().to_string());}
        }
    });
}
pub(crate) fn server_mode(ui: &mut egui::Ui, mode: &mut sleufbase_rust::template_server::Mode) {
    use sleufbase_rust::template_server::Mode;
    ui.label("Kaartgegevens");
    ui.radio_value(mode, Mode::Fresh, "Online · actuele kaartgegevens ophalen");
    ui.radio_value(mode, Mode::Offline, "Offline · alleen al geladen gegevens");
    if *mode==Mode::Offline {ui.small("Offline export gebruikt geladen bronnen en een kaart zonder luchtfoto.");}
}
/// Shared persistent CAD support folders for both template export routes.
pub(crate) fn font_folders(ui: &mut egui::Ui, folders: &mut Vec<std::path::PathBuf>) {
    egui::CollapsingHeader::new("Extra CAD-lettertypen").show(ui, |ui| {
        ui.label("Voeg een map met SHX-, SHP-, LFF- of TrueType-lettertypen toe. Deze keuze wordt bij het project bewaard.");
        let mut remove = None;
        for (i, folder) in folders.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(folder.display().to_string());
                if ui.small_button("Verwijderen").clicked() { remove = Some(i); }
            });
        }
        if let Some(i) = remove { folders.remove(i); }
        if ui.add_enabled(folders.len() < 64, egui::Button::new("Lettertypemap toevoegen…")).clicked() {
            if let Some(folder) = rfd::FileDialog::new().set_title("Map met CAD-lettertypen").pick_folder() {
                if !folders.contains(&folder) { folders.push(folder); }
            }
        }
    });
}
/// Shared drawing-stamp fields for both template export routes.
pub(crate) fn title_fields(ui: &mut egui::Ui, fields: &mut sleufbase_rust::template_metadata::TitleFields) {
    egui::CollapsingHeader::new("Tekeningstempel")
        .id_salt("template-title-fields")
        .default_open(true)
        .show(ui, |ui| {
            ui.small("Deze gegevens komen op ieder gebruikt blad.");
            crate::ui::theme::text_fields(ui, "template-title-fields-grid", [
                ("Gezien", &mut fields.checked_by),
                ("Status", &mut fields.status),
                ("Versie", &mut fields.version),
                ("Referentie Tekeningen", &mut fields.reference_drawings),
                ("ProjectNr.BGM", &mut fields.project_number),
                ("TEK.NR.", &mut fields.drawing_number),
            ]);
        })
        .header_response
        .on_hover_text("Vul gezien, status, versie, referentietekening, projectnummer en tekeningnummer in.");
}
/// Keep these controls beside the outer form scroll area, never inside it.
pub(crate) fn order_columns(ui: &mut egui::Ui, height: f32) -> (egui::Ui, egui::Ui) {
    let width = ui.available_width().max(1.);
    let gap = 12_f32.min(width * 0.04);
    let preferred = if width >= 760. { 260. } else if width >= 620. { 230. }
        else if width >= 480. { 200. } else { width * 0.42 };
    let controls_width = preferred.min((width - gap) * 0.48).max(1.);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let controls_rect = egui::Rect::from_min_max(
        egui::pos2(rect.right() - controls_width, rect.top()), rect.max);
    let body_rect = egui::Rect::from_min_max(rect.min,
        egui::pos2(controls_rect.left() - gap, rect.bottom()));
    let mut make = |salt, rect: egui::Rect| {
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt(salt).max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::LEFT)));
        child.set_width(rect.width().max(1.));
        child.set_clip_rect(ui.clip_rect().intersect(rect));
        child
    };
    // The right pane owns both fixed movement buttons and its own scrolling
    // tools. Widths stay independent from the form and the length of a row.
    (make("export-order-body", body_rect), make("export-order-controls", controls_rect))
}
pub(crate) fn order_controls(ui: &mut egui::Ui, selection: &[bool], height: f32) -> Option<i32> {
    let count = selection.iter().filter(|selected| **selected).count();
    if height >= 112. { ui.strong("Volgorde"); }
    if height >= 150. { ui.small(format!("{count} geselecteerd")); }
    let button_height = if height >= 112. { 32. } else { 24. };
    let width = ui.available_width();
    let mut direction = None;
    for (label, offset, enabled) in [
        ("Omhoog", -1, selection.windows(2).any(|rows| !rows[0] && rows[1])),
        ("Omlaag", 1, selection.windows(2).any(|rows| rows[0] && !rows[1])),
    ] {
        if ui.add_enabled(enabled, egui::Button::new(label).min_size(egui::vec2(width, button_height)))
            .on_hover_text(format!("Verplaats de {count} geselecteerde regel(s) één plek."))
            .clicked()
        { direction = Some(offset); }
    }
    direction
}
/// Bulk controls scroll independently beneath the fixed movement buttons.
fn order_tools(ui: &mut egui::Ui, d: &mut Dialog) {
    let button = |ui: &mut egui::Ui, label: &str, enabled: bool| {
        let clicked = ui.add_enabled(enabled, egui::Button::new(label).wrap()
            .min_size(egui::vec2(ui.available_width(), 28.))).clicked();
        if clicked { ui.ctx().request_repaint(); }
        clicked
    };
    ui.small("Selecteer links een regel. Ctrl/Shift kiest meerdere regels.");
    ui.separator();
    ui.strong("Sorteren");
    let mut requested_sort = d.sort_mode;
    egui::ComboBox::from_id_salt("raster-template-sort")
        .width(ui.available_width())
        .wrap()
        .selected_text(requested_sort.map_or("Handmatig / opgeslagen", SortMode::label))
        .show_ui(ui, |ui| {
            for mode in SortMode::ALL {
                ui.selectable_value(&mut requested_sort, Some(mode), mode.label());
            }
        });
    let sort_again = button(ui, "Opnieuw sorteren", !d.state.rows.is_empty());
    if sort_again || requested_sort != d.sort_mode {
        ui.ctx().request_repaint();
        let mode = requested_sort.unwrap_or(SortMode::NumberAsc);
        let order = d.state.sort_rows(mode);
        template_order::reorder(&mut d.selected, &order);
        d.sort_mode = Some(mode);
        d.anchor = d.selected.iter().position(|selected| *selected).unwrap_or(0);
        d.reveal_row = d.selected.iter().position(|selected| *selected);
        d.sync_controls();
    }

    ui.separator();
    ui.strong("Selectie");
    let selected_rows = d.selected.iter().filter(|selected| **selected).count();
    for (text, included) in [("Selectie meenemen", true), ("Selectie uitsluiten", false)] {
        if button(ui, text, selected_rows > 0) {
            for (row, selected) in d.state.rows.iter_mut().zip(&d.selected) {
                if *selected && row.raster.is_some() { row.included = included; }
            }
        }
    }
    ui.separator();
    ui.strong("Nummering en schaal");
    ui.label("PS-nummer");
    ui.add_sized([ui.available_width(), ui.spacing().interact_size.y],
        egui::TextEdit::singleline(&mut d.number));
    ui.label("Variant");
    ui.add_sized([ui.available_width(), ui.spacing().interact_size.y],
        egui::TextEdit::singleline(&mut d.variant));
    ui.small("Leeg, AUTO, B/C/…");
    if button(ui, "Nummer/variant toepassen", selected_rows > 0) {
        d.error = d.apply_label().err().unwrap_or_default();
    }
    ui.label("Profielschaal");
    ui.add_sized([ui.available_width(), ui.spacing().interact_size.y],
        egui::TextEdit::singleline(&mut d.scale));
    if button(ui, "Schaal toepassen", selected_rows > 0) {
        d.error = d.apply_scale().err().unwrap_or_default();
    }

    ui.separator();
    ui.strong("Lege vakken");
    if button(ui, "Leeg vak invoegen", d.state.rows.len() < 5000) {
        let index = d.selected.iter().position(|selected| *selected)
            .unwrap_or(d.state.rows.len());
        d.state.rows.insert(index, Row::default());
        d.selected = vec![false; d.state.rows.len()];
        d.selected[index] = true;
        d.anchor = index;
        d.reveal_row = Some(index);
        d.sort_mode = None;
        d.sync_controls();
    }
    let empty_selected = d.state.rows.iter().zip(&d.selected)
        .any(|(row, selected)| *selected && row.raster.is_none());
    if button(ui, "Lege vakken verwijderen", empty_selected) {
        let mut index = 0;
        d.state.rows.retain(|row| {
            let keep = row.raster.is_some() || !d.selected[index];
            index += 1;
            keep
        });
        d.selected = vec![false; d.state.rows.len()];
        d.anchor = 0;
        d.reveal_row = None;
        d.sync_controls();
    }
}
impl Dialog {
    pub fn new(mut state: State, rasters: &[GeoTiffInfo], generation: u64) -> Self {
        let fresh=state.rows.is_empty();
        let error = state.sync(rasters).err().unwrap_or_default();
        if fresh && error.is_empty() {state.sort_rows(SortMode::NumberAsc);}
        let mut selected = vec![false; state.rows.len()];
        if let Some(s) = selected.first_mut() {
            *s = true;
        }
        let mut d = Self {
            state,
            error,
            selected,
            generation,
            anchor: 0,
            number: String::new(),
            variant: String::new(),
            scale: String::new(),
            sort_mode: fresh.then_some(SortMode::NumberAsc),
            reveal_row: None,
            opening_defaults: None,
        };
        d.sync_controls();
        d
    }
    /// Saved rows remain explicit; general settings are read anew on opening.
    pub fn with_settings(mut self, settings: &MigratedSettings) -> Result<Self, String> {
        let defaults = ExportDefaults::from_settings(settings)?;
        defaults.apply_order(&mut self.state);
        self.opening_defaults = Some(defaults);
        Ok(self)
    }
    fn sync_controls(&mut self) {
        let rows = self
            .state
            .rows
            .iter()
            .zip(&self.selected)
            .filter(|(r, s)| **s && r.raster.is_some())
            .map(|(r, _)| r)
            .collect::<Vec<_>>();
        fn common(values: impl Iterator<Item = String>) -> String {
            let mut values = values;
            let Some(first) = values.next() else {
                return String::new();
            };
            if values.any(|s| s != first) {
                "Gemengd".into()
            } else {
                first
            }
        }
        self.number = common(rows.iter().map(|r| r.number.clone()));
        self.variant = common(rows.iter().map(|r| r.variant.clone()));
        self.scale = common(
            rows.iter()
                .map(|r| r.scale.map_or_else(|| "Auto".into(), |s| format!("1:{s}"))),
        );
    }
    fn select(&mut self, index: usize, modifiers: egui::Modifiers) {
        if modifiers.shift {
            if !modifiers.command && !modifiers.ctrl {
                self.selected.fill(false);
            }
            for i in self.anchor.min(index)..=self.anchor.max(index) {
                if let Some(s) = self.selected.get_mut(i) {
                    *s = true;
                }
            }
        } else {
            if !modifiers.command && !modifiers.ctrl {
                self.selected.fill(false);
            }
            self.selected[index] = !self.selected[index];
            self.anchor = index;
        }
        self.sync_controls();
    }
    fn apply_label(&mut self) -> Result<(), String> {
        let n = if self.number.to_lowercase() == "gemengd" {
            None
        } else {
            Some(
                template_order::number(&self.number)
                    .ok_or("Vul een positief of nul PS-nummer in, bijvoorbeeld 5 of PS5")?,
            )
        };
        let v = if self.variant.to_lowercase() == "gemengd" {
            None
        } else {
            Some(template_order::variant(&serde_json::Value::String(
                self.variant.clone(),
            )))
        };
        for (r, s) in self.state.rows.iter_mut().zip(&self.selected) {
            if *s && r.raster.is_some() && r.included {
                if let Some(n) = &n {
                    r.number = n.clone();
                }
                if let Some(v) = &v {
                    r.variant = v.clone();
                }
            }
        }
        self.sync_controls();
        Ok(())
    }
    fn apply_scale(&mut self) -> Result<(), String> {
        let s = self.scale.trim();
        if s.eq_ignore_ascii_case("gemengd") {
            return Ok(());
        }
        let scale = sleufbase_rust::template_scale::parse(s)?;
        for (r, s) in self.state.rows.iter_mut().zip(&self.selected) {
            if *s && r.raster.is_some() {
                r.scale = scale;
            }
        }
        self.sync_controls();
        Ok(())
    }
}
pub(crate) fn show(
    ctx: &egui::Context,
    dialog: &mut Option<Dialog>,
    rasters: &[GeoTiffInfo],
    datasets: &[Dataset],
    generation: u64,
) -> Option<State> {
    let d = dialog.as_mut()?;
    if d.generation != generation {
        *dialog = None;
        return None;
    }
    let old = d
        .state
        .rows
        .iter()
        .map(|r| r.raster.clone())
        .collect::<Vec<_>>();
    let sync = d.state.sync(rasters);
    if let Err(e) = &sync {
        d.error = e.clone();
    }
    if old
        != d.state
            .rows
            .iter()
            .map(|r| r.raster.clone())
            .collect::<Vec<_>>()
    {
        d.selected = vec![false; d.state.rows.len()];
        for r in &mut d.state.rows {
            if r.raster.is_some() && !old.contains(&r.raster) {
                r.included = false;
            }
        }
        d.anchor = 0;
        d.reveal_row = None;
        d.sync_controls();
    }
     let mut open = true;
    let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut result = None;
    let contents = |ui: &mut egui::Ui| {
        ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
        ui.spacing_mut().icon_width=18.;
        // Match Resize's requested height even when the footer uses less space.
        ui.set_min_height(ui.available_height());
        let footer_height=if d.error.is_empty() {104.} else {128.}
            + if ui.available_width()<440. {28.} else {0.};
        let body_height=(ui.available_height()-footer_height).max(64.);
        let reveal_row=d.reveal_row.take();
        let (mut body_ui,mut controls_ui)=order_columns(ui,body_height);
        {
            let ui=&mut body_ui;
            egui::ScrollArea::vertical().id_salt("template-settings")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .max_height(body_height).auto_shrink([false,false]).show(ui,|ui|{
        ui.heading("1. Project en sjabloon");
        let field_width=(ui.available_width()-110.).max(80.);
        egui::Grid::new("template-identity").num_columns(2).min_col_width(90.).show(ui,|ui|{
            ui.label("Projectnaam");ui.add(egui::TextEdit::singleline(&mut d.state.project).desired_width(field_width));ui.end_row();
            ui.label("Tekenaar");ui.add(egui::TextEdit::singleline(&mut d.state.drawn_by).desired_width(field_width));ui.end_row();
        });
        title_fields(ui, &mut d.state.title_fields);

        ui.label(d.state.template.as_ref().map_or_else(||"Sjabloon: ingebouwd SleufBase-sjabloon".into(),|p|format!("Eigen sjabloon: {}",p.display())));
        ui.collapsing("Eigen DXF-sjabloon gebruiken (optioneel)",|ui|{
            ui.small("Voor de standaardexport is geen bestand nodig.");
            if ui.button("Eigen DXF-sjabloon kiezen…").clicked(){
                if let Some(p)=rfd::FileDialog::new().add_filter("DXF-sjabloon",&["dxf"]).pick_file(){d.state.template=Some(p);}
            }
            if d.state.template.is_some()&&ui.button("Ingebouwd sjabloon gebruiken").clicked(){d.state.template=None;}
        });


        ui.label("De exportdatum wordt automatisch op ieder gebruikt blad ingevuld (dd-mm-jjjj).");
        font_folders(ui, &mut d.state.font_directories);
        server_mode(ui, &mut d.state.server_mode);
        logos(ui, &mut d.state.logos);


        ui.separator();
        ui.heading("2. Proefsleuven en volgorde");
        ui.label("Kies hier een proefsleuf. Rechts staan de opties voor volgorde, selectie, nummering en lege vakken.");
        let labels=Some(d.state.preview_labels().0);

        let row_height=26.;
        let mut list=egui::ScrollArea::vertical()
            .id_salt("template-order")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .max_height(body_height.min(320.));
        if let Some(index)=reveal_row.filter(|index|*index<d.state.rows.len()) {
            list=list.vertical_scroll_offset(index as f32*(row_height+ui.spacing().item_spacing.y));
        }
        let list=list.show_rows(ui, row_height, d.state.rows.len(), |ui, range| {
                for i in range {
                    let r = &d.state.rows[i];
                    let name = r.raster.as_ref().map_or_else(
                        || "[Leeg vak]".into(),
                        |p| {
                            p.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        },
                    );
                    let ps =
                        labels
                            .as_ref()
                            .and_then(|l| l[i].as_deref())
                            .unwrap_or(if r.included {
                                "Ongeldig PS-nummer"
                            } else {
                                "Niet meenemen"
                            });
                    let text = if r.raster.is_none() {
                        format!("{:02}. {name}", i + 1)
                    } else {
                        format!(
                            "{:02}. [{ps}] [{}] {name}",
                            i + 1,
                            r.scale.map_or_else(|| "Auto".into(), |s| format!("1:{s}"))
                        )
                    };

                    if ui.add_sized([ui.available_width(),row_height],
                        egui::Button::new(text.as_str()).selected(d.selected[i]).frame(false).truncate())
                        .on_hover_text(text).clicked()
                    {
                        d.select(i, ui.input(|i| i.modifiers));
                    }
                }
            });
        if reveal_row.is_some() {
            // The virtual list and its containing form both need to reveal the
            // moved row; scrolling only one leaves it hidden in the other.
            ui.scroll_to_rect(list.inner_rect,Some(egui::Align::Min));
        }

        let selected = d
            .selected
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.then_some(i))
            .collect::<Vec<_>>();

        if selected.len() == 1 {
            let row = &mut d.state.rows[selected[0]];
            if row.raster.is_some() {
                ui.horizontal_wrapped(|ui| {
                    let current = if row.without_profile {
                        "Geen profiel".into()
                    } else {
                        row.dataset
                            .as_ref()
                            .and_then(|p| datasets.iter().find(|d| d.source_path == *p))
                            .map_or_else(|| "Automatisch via jobnummer".into(), |d| d.title.clone())
                    };
                    egui::ComboBox::from_id_salt("template-profile-job")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    !row.without_profile && row.dataset.is_none(),
                                    "Automatisch via jobnummer",
                                )
                                .clicked()
                            {
                                row.without_profile = false;
                                row.dataset = None;
                            }

                            if ui
                                .selectable_label(row.without_profile, "Geen profiel")
                                .clicked()
                            {
                                row.without_profile = true;
                                row.dataset = None;
                            }

                            for dataset in datasets {
                                if ui
                                    .selectable_label(
                                        row.dataset.as_ref() == Some(&dataset.source_path),
                                        format!(
                                            "{} — {}",
                                            dataset.title,
                                            dataset.source_path.display()
                                        ),
                                    )
                                    .clicked()
                                {
                                    row.dataset = Some(dataset.source_path.clone());
                                    row.without_profile = false;
                                }
                            }
                        });
                    ui.label("Adres");
                    let mut address = row.address.clone().unwrap_or_default();
                    if ui.text_edit_singleline(&mut address).changed() {
                        row.address = (!address.is_empty()).then_some(address);
                    }
                });

                ui.label("Opmerkingen");
                ui.add_sized([ui.available_width(), ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut row.comments));
            }
        }

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut d.state.profiles, "Dwarsprofielen");
            ui.checkbox(&mut d.state.paired, "Beide richtingen (Normaal / Reverse)");
            ui.add_enabled(
                !d.state.paired,
                egui::Checkbox::new(&mut d.state.reverse, "Alleen omgekeerde richting"),
            );
            ui.checkbox(&mut d.state.show_reference, "Referentiepunt");
            ui.checkbox(&mut d.state.avoid_collisions, "Labels uit elkaar plaatsen");
        });

        if d.state.server_mode==sleufbase_rust::template_server::Mode::Fresh {
            ui.horizontal_wrapped(|ui| {
                ui.label("Achtergrond");
                let mut background=d.state.effective_background();
                for (value,label) in [(BackgroundKind::Pdok,"Luchtfoto"),
                    (BackgroundKind::PdokCadastral,"PDOK Luchtfoto + percelen"),
                    (BackgroundKind::Osm,"OpenStreetMap"),
                    (BackgroundKind::OsmCadastral,"OpenStreetMap + percelen"),(BackgroundKind::Cadastral,"Kadaster"),
                    (BackgroundKind::Blank,"Wit")]
                {
                    if ui.radio_value(&mut background,value,label).changed() {
                        d.state.map_background=Some(background);
                        d.state.aerial=matches!(background,BackgroundKind::Pdok|BackgroundKind::PdokCadastral);
                    }
                }
            });
        } else {
            ui.small("Witte kaart · geladen gegevens")
                .on_hover_text("Je keuze voor de online achtergrond blijft bewaard.");
        }
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut d.state.include_cad,"Zichtbare CAD-lagen");
            ui.checkbox(&mut d.state.auto_bgt, "BGT-ondergrond automatisch invullen");
            ui.label("Markerdiameter (m)");
            ui.add(
                egui::DragValue::new(&mut d.state.marker_diameter)
                    .range(0.005..=1.)
                    .speed(0.005),
            );
        });

            });
        }
        {
            let ui=&mut controls_ui;
            if let Some(direction)=order_controls(ui,&d.selected,body_height) {
                    d.state.move_selected(&mut d.selected,direction>0);
                    d.sort_mode=None;
                    d.anchor=d.selected.iter().position(|selected|*selected).unwrap_or(0);
                    d.reveal_row=if direction<0 {
                        d.selected.iter().position(|selected|*selected)
                    } else {
                        d.selected.iter().rposition(|selected|*selected)
                    };
                    d.sync_controls();
                ui.ctx().request_repaint();
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("raster-template-order-tools")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .max_height(ui.available_height().max(1.))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    order_tools(ui, d);
                });
        }
        ui.separator();
        let included = d
            .state
            .rows
            .iter()
            .filter(|r| r.raster.is_some() && r.included)
            .count();
        let empty = d.state.rows.iter().filter(|r| r.raster.is_none()).count();
        ui.label(format!(
            "Meenemen: {included} | Lege vakken: {empty} | Bladen: {}",
            (included + empty).div_ceil(8)
        ));

        let title_error = d.state.title_fields.validate().err();
        if let Some(error) = &title_error {
            ui.add(egui::Label::new(egui::RichText::new(error).color(egui::Color32::RED)).truncate())
                .on_hover_text(error);
        } else if !d.error.is_empty() {
            ui.add(egui::Label::new(egui::RichText::new(&d.error).color(egui::Color32::RED)).truncate())
                .on_hover_text(&d.error);
        }

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    sync.is_ok() && included > 0 && title_error.is_none(),
                    egui::Button::new(egui::RichText::new("Exporteren…").color(egui::Color32::WHITE))
                        .fill(egui::Color32::from_rgb(234,88,12)),
                )
                .clicked()
            {
                match d.state.confirmed() {
                    Ok(state) => result = Some(state),
                    Err(e) => d.error = e,
                }
            }

            if ui.button("Annuleren").clicked() {
                cancel = true;
            }
        });
    };
    let screen=ctx.screen_rect();
    let maximum=egui::vec2((screen.width()-64.).max(280.),(screen.height()-88.).max(200.));
    egui::Window::new("DXF-sjabloon exporteren")
        .id(egui::Id::new("raster-template-compact-v3"))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER,egui::Vec2::ZERO)
        .default_size(egui::vec2(940.,560.).min(maximum))
        .min_size(egui::vec2(680.,320.).min(maximum))
        .max_size(maximum)
        .constrain_to(screen.shrink(16.))
        .collapsible(false)
        .vscroll(false)
        .resizable(true)
        .show(ctx, contents);

    if !open || cancel {
        *dialog = None;
        return None;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn raster() -> GeoTiffInfo {
        let transform = sleufbase_rust::geo::GeoTransform {
            a: 1.,
            b: 0.,
            c: 0.,
            d: 0.,
            e: -1.,
            f: 10.,
        };
        GeoTiffInfo {
            path: "PS01B.tiff".into(),
            width: 10,
            height: 10,
            transform,
            bounds: transform.bounds(10, 10).unwrap(),
            epsg: Some(28992),
            kickthemap_job_id: None,
            kickthemap_job_title: String::new(),
            kickthemap_job_sidecar: None,metadata:Default::default(),
        }
    }
    fn frame(
        ctx: &egui::Context,
        d: &mut Option<Dialog>,
        rasters: &[GeoTiffInfo],
        events: Vec<egui::Event>,
    ) -> (Option<State>, egui::FullOutput) {
        let mut result = None;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1024., 768.),
                )),
                events,
                ..Default::default()
            },
            |ctx| result = show(ctx, d, rasters, &[], 7),
        );
        (result, output)
    }
    #[test]
    fn export_footer_remains_reachable_with_many_sources_and_small_windows(){
        let rasters=(0..40).map(|i|{let mut r=raster();r.path=format!("PS{i:02}-lange-bestandsnaam.tiff").into();r}).collect::<Vec<_>>();
        for size in [egui::vec2(680.,420.),egui::vec2(1200.,760.),egui::vec2(1500.,930.)]{
            for dpi in [1.,1.25,2.]{
                let ctx=egui::Context::default();ctx.set_pixels_per_point(dpi);
                let mut d=Some(Dialog::new(State::default(),&rasters,7));
                let mut out=None;
                for _ in 0..4 {out=Some(ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,size)),..Default::default()},|ctx|{show(ctx,&mut d,&rasters,&[],7);}));}
                let out=out.unwrap();
                for label in ["Exporteren…","Annuleren"]{
                    let (rect,clip)=out.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label=>Some((t.galley.rect.translate(t.pos.to_vec2()),s.clip_rect)),_=>None}).expect(label);
                    assert!(rect.min.x>=0.&&rect.min.y>=0.&&rect.max.x<=size.x&&rect.max.y<=size.y,"{size:?}: {rect:?}");
                    assert!(clip.contains_rect(rect),"footer clipped: {label}");
                }
            }
        }
    }
    #[test]
    fn actual_window_exports_project_value_and_escape_discards_draft_edits() {
        let rasters = vec![raster()];
        let original = State {
            project: "Bestaand Ø".into(),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut d = Some(Dialog::new(original.clone(), &rasters, 7));
        d.as_mut().unwrap().state.project = "  Nieuw project Ø  ".into();
        frame(&ctx, &mut d, &rasters, vec![]);
        let (_, output) = frame(&ctx, &mut d, &rasters, vec![]);
        assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text.contains("automatisch op ieder gebruikt blad"))));
        let pos = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "Exporteren…" => {
                    Some(t.pos + t.galley.rect.size() / 2.)
                }
                _ => None,
            })
            .expect("visible export button");
        assert!(pos.y < 768.);
        let (exported, _) = frame(
            &ctx,
            &mut d,
            &rasters,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
        );
        assert_eq!(exported.unwrap().project, "Nieuw project Ø");
        let (cancelled, _) = frame(
            &ctx,
            &mut d,
            &rasters,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(cancelled.is_none());
        assert!(d.is_none());
        assert_eq!(original.project, "Bestaand Ø");
    }
    #[test]
    fn project_replacement_closes_and_appended_sources_start_excluded() {
        let mut rasters = vec![raster()];
        let ctx = egui::Context::default();
        let mut d = Some(Dialog::new(State::default(), &rasters, 7));
        let mut other = raster();
        other.path = "PS2.tiff".into();
        rasters.push(other);
        frame(&ctx, &mut d, &rasters, vec![]);
        assert!(!d.as_ref().unwrap().state.rows[1].included);
        assert!(show(&ctx, &mut d, &rasters, &[], 8).is_none());
        assert!(d.is_none());
    }
}
