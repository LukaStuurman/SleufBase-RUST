//! Transactional settings draft with explicit DXF-template file actions.
//! Original UI contracts: settings_navigation_patch.py and settings.py normalizers.
use eframe::egui;
use sleufbase_rust::{
    disciplines::Rule, project::AutosaveSettings, settings_compat::MigratedSettings,
};
use std::{collections::BTreeMap, path::PathBuf, sync::mpsc, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    Maps,
    PointClouds,
    Dxf,
    Trenches,
    KickTheMap,
    Backups,
}
impl Category {
    const ALL: [Self; 6] = [
        Self::Maps,
        Self::PointClouds,
        Self::Dxf,
        Self::Trenches,
        Self::KickTheMap,
        Self::Backups,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Backups => "Back-ups",
            Self::Maps => "Kaarten",
            Self::PointClouds => "Puntenwolken",
            Self::Dxf => "DXF-export",
            Self::Trenches => "Proefsleuven",
            Self::KickTheMap => "KickTheMap",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ViewerDraft {
    pub background_source: String,
    pub use_background: bool,
}
pub(crate) struct Saved {
    /// Applied to the active project view only after both settings files saved.
    pub viewer: Option<ViewerDraft>,
    pub settings: MigratedSettings,
    pub autosave: AutosaveSettings,
}
struct RuleRow {
    words: String,
    layer: String,
    label: String,
    color: u16,
}
#[derive(Clone, Copy)]
enum TemplateAction {
    ExportOriginal,
    ImportCustom,
}
impl TemplateAction {
    fn progress(self) -> &'static str {
        match self {
            Self::ExportOriginal => "Ingebouwd sjabloon opslaan…",
            Self::ImportCustom => "Aangepast sjabloon controleren en laden…",
        }
    }
}
struct TemplateTask {
    action: TemplateAction,
    result: mpsc::Receiver<Result<PathBuf, String>>,
}
struct LogoTask {
    result: mpsc::Receiver<Result<sleufbase_rust::client_logos::PendingLogo, String>>,
}
struct TemplateNotice {
    text: &'static str,
    path: Option<PathBuf>,
}
pub(crate) struct Dialog {
    pub draft: MigratedSettings,
    pub autosave: AutosaveSettings,
    pub category: Category,
    pub words: bool,
    pub error: String,
    /// Caller supplies active view state; None leaves the viewer untouched.
    pub viewer: Option<ViewerDraft>,
    pub autosave_count: Option<usize>,
    pub project_scoped: bool,
    /// Caller takes this request and opens its existing autosave selector.
    pub restore_autosave_requested: bool,
    /// Explicit action independent of Save and the automatic 183-day toggle.
    pub manual_offline_download:Option<(u8,[i32;4],u8)>,
    pub offline_delete:Option<Option<u64>>,
    pub offline_delete_confirm:Option<Option<u64>>,
    pub offline_downloads:Vec<sleufbase_rust::offline_zoom::Download>,
    pub offline_catalog_notice:Option<String>,
    /// Current visible RD extent captured before opening Settings.
    pub offline_area:Option<[i32;4]>,
    pub manual_offline_busy:bool,
    pub manual_offline_status:Option<String>,
    rules: Vec<RuleRow>,
    materials: Vec<String>,
    extras: Vec<String>,
    mappings: Vec<(String, String)>,
    template_task: Option<TemplateTask>,
    template_notice: Option<TemplateNotice>,
    logo_task: Option<LogoTask>,
    pending_logos: Vec<sleufbase_rust::client_logos::PendingLogo>,
}
// Reuse the production Python-compatible normalizers, including Unicode
// casefold and whitespace behavior; do not create a competing UI interpretation.
fn choices(values: &[String]) -> Result<Vec<String>, String> {
    check_text_budget(values.iter().map(String::as_str))?;
    Ok(sleufbase_rust::settings_compat::from_legacy_json(
        &serde_json::json!({"kickthemap_material_choices":values}).to_string(),
    )?
    .kickthemap_material_choices)
}
fn mappings(values: &[(String, String)]) -> Result<BTreeMap<String, String>, String> {
    check_text_budget(values.iter().flat_map(|(s, t)| [s.as_str(), t.as_str()]))?;
    Ok(sleufbase_rust::settings_compat::normalize_name_mappings(
        values
            .iter()
            .map(|(source, target)| (source.as_str(), target.as_str())),
    ))
}
fn check_text_budget<'a>(values: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut bytes = 0usize;
    let mut count = 0usize;
    for value in values {
        bytes = bytes.saturating_add(value.len());
        count += 1;
        if value.len() > 32768 || bytes > 1024 * 1024 || count > 10000 {
            return Err(
                "Te veel tekst in instellingen; verkort de ingevoerde keuzes of naamkoppelingen"
                    .into(),
            );
        }
    }
    Ok(())
}

impl Dialog {
    pub fn new(settings: &MigratedSettings, autosave: &AutosaveSettings) -> Self {
        let mut draft = settings.clone();
        for logo in &mut draft.export_preferences.template_client_logos {
            if logo.client_code.trim().is_empty() { logo.client_code = logo.effective_code(); }
        }
        Self {
            draft,
            autosave: autosave.clone(),
            category: Category::Maps,
            words: false,
            error: String::new(),
            viewer: None,
            autosave_count: None,
            project_scoped: false,
            restore_autosave_requested: false,
            manual_offline_download:None,offline_delete:None,offline_delete_confirm:None,
            offline_downloads:Vec::new(),offline_catalog_notice:None,offline_area:None,
            manual_offline_busy:false,
            manual_offline_status:None,
            template_task: None,
            template_notice: None,
            logo_task: None,
            pending_logos: Vec::new(),
            rules: settings
                .object_rules
                .iter()
                .map(|r| RuleRow {
                    words: r
                        .keywords
                        .iter()
                        .map(|s| s.as_ref())
                        .collect::<Vec<_>>()
                        .join(", "),
                    layer: r.target_layer.to_string(),
                    label: r.label.to_string(),
                    color: r.color_aci,
                })
                .collect(),
            materials: settings.kickthemap_material_choices.clone(),
            extras: settings.kickthemap_profile_extra_choices.clone(),
            mappings: settings
                .marxact_name_mappings
                .iter()
                .map(|(s, t)| (s.clone(), t.clone()))
                .collect(),
        }
    }
    fn start_template_task(&mut self, ctx: &egui::Context, action: TemplateAction, path: PathBuf) {
        if self.template_task.is_some() {
            return;
        }
        let (tx, result) = mpsc::sync_channel(1);
        let repaint = ctx.clone();
        self.error.clear();
        self.template_notice = None;
        match std::thread::Builder::new().name("sleufbase-template-settings".into()).spawn(move || {
            let result = match action {
                TemplateAction::ExportOriginal => sleufbase_rust::template_custom::export_original(&path)
                    .map(|()| path),
                TemplateAction::ImportCustom => sleufbase_rust::template_custom::import_custom(&path),
            };
            let _ = tx.send(result);
            repaint.request_repaint();
        }) {
            Ok(_) => self.template_task = Some(TemplateTask { action, result }),
            Err(error) => self.error = format!("Sjabloontaak starten mislukt: {error}"),
        }
    }
    fn poll_template_task(&mut self, ctx: &egui::Context) {
        let Some(task) = self.template_task.as_ref() else { return; };
        let action = task.action;
        let result = match task.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("Sjabloontaak is onverwacht gestopt".into()),
        };
        self.template_task = None;
        match result {
            Ok(path) => {
                self.error.clear();
                let text = match action {
                    TemplateAction::ExportOriginal => "Ingebouwd sjabloon opgeslagen.",
                    TemplateAction::ImportCustom => {
                        self.draft.export_preferences.cadastral_template_path = path.to_string_lossy().into_owned();
                        "Eigen sjabloon geladen. Klik op Opslaan om deze keuze te bewaren."
                    }
                };
                self.template_notice = Some(TemplateNotice { text, path: Some(path) });
            }
            Err(error) => self.error = format!("{} {error}", match action {
                TemplateAction::ExportOriginal => "Ingebouwd sjabloon opslaan mislukt:",
                TemplateAction::ImportCustom => "Aangepast sjabloon laden mislukt:",
            }),
        }
    }
    fn start_logo_task(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.logo_task.is_some() { return; }
        let (tx, result) = mpsc::sync_channel(1);
        let repaint = ctx.clone();
        self.error.clear();
        match std::thread::Builder::new().name("sleufbase-client-logo".into()).spawn(move || {
            let _ = tx.send(sleufbase_rust::client_logos::stage(&path));
            repaint.request_repaint();
        }) {
            Ok(_) => self.logo_task = Some(LogoTask { result }),
            Err(error) => self.error = format!("Logo laden starten mislukt: {error}"),
        }
    }
    fn poll_logo_task(&mut self, ctx: &egui::Context) {
        let Some(task) = self.logo_task.as_ref() else { return; };
        let result = match task.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("Logo laden is onverwacht gestopt".into()),
        };
        self.logo_task = None;
        match result {
            Ok(mut pending) => {
                let names = &self.draft.export_preferences.template_client_logos;
                if names.len() >= sleufbase_rust::client_logos::MAX_LOGOS {
                    self.error = "De lijst met opdrachtgeverslogo's is vol".into();
                    return;
                }
                pending.logo.name = sleufbase_rust::client_logos::unique_name(&pending.logo.name, names);
                self.draft.export_preferences.template_client_logos.push(pending.logo.clone());
                self.pending_logos.push(pending);
                self.error.clear();
            }
            Err(error) => self.error = error,
        }
    }
    pub fn open_words(&mut self) {
        self.words = true;
    }
    pub fn back(&mut self) {
        self.words = false;
    }
    fn candidate(&self) -> Result<Saved, String> {
        let mut settings = self.draft.clone();
        settings.object_rules = self
            .rules
            .iter()
            .map(|r| {
                Rule::new(
                    r.words
                        .split([',', ';', '\n'])
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect(),
                    r.layer.trim().into(),
                    r.color,
                    r.label.trim().into(),
                )
            })
            .collect();
        settings.kickthemap_material_choices = choices(&self.materials)?;
        settings.kickthemap_profile_extra_choices = choices(&self.extras)?;
        settings.marxact_name_mappings = mappings(&self.mappings)?;
        for logo in &mut settings.export_preferences.template_client_logos {
            logo.name = logo.name.trim().to_owned();
            logo.client_code = logo.client_code.trim().to_owned();
        }
        if let Some(viewer) = &self.viewer {
            // Keep global working-map preferences from the staged draft.
            settings.viewer.background_source = viewer.background_source.clone();
            settings.viewer.use_background = viewer.use_background;
            settings.viewer.explicit_background_choice = true;
        }
        if settings.viewer.offline_aerial_auto_backup
            && settings.viewer.offline_aerial_area.is_none(){
            // An explicit opt-in may use the current map extent on Save,
            // without requiring a separate manual download first.
            settings.viewer.offline_aerial_area=if settings.viewer.offline_aerial_whole_country {
                Some(sleufbase_rust::offline_zoom::NETHERLANDS_AREA)
            }else{self.offline_area};
        }
        settings.validate()?;
        Ok(Saved {
            viewer: self.viewer.clone(),
            settings,
            autosave: self.autosave.clone().normalize(),
        })
    }
    pub fn try_save(
        &mut self,
        save: &mut impl FnMut(&MigratedSettings, &AutosaveSettings) -> Result<(), String>,
    ) -> Option<Saved> {
        if self.template_task.is_some() || self.logo_task.is_some() {
            self.error = "Wacht tot het laden klaar is voordat je de instellingen opslaat".into();
            return None;
        }
        match self.candidate().and_then(|mut candidate| {
            let publication = sleufbase_rust::client_logos::publish_pending(
                &sleufbase_rust::project::app_data_directory(),
                &mut candidate.settings.export_preferences, &self.pending_logos,
            )?;
            save(&candidate.settings, &candidate.autosave)?;
            publication.commit();
            Ok(candidate)
        }) {
            Ok(saved) => {
                self.error.clear();
                Some(saved)
            }
            Err(error) => {
                self.error = error;
                None
            }
        }
    }
}
fn list_editor(ui: &mut egui::Ui, title: &str, values: &mut Vec<String>) {
    super::theme::strong(ui,title);
    let mut remove = None;
    for (index, value) in values.iter_mut().enumerate() {
        ui.push_id((title, index), |ui| {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(value);
                if ui.button("Verwijderen").clicked() {
                    remove = Some(index);
                }
            });
        });
    }
    if let Some(index) = remove {
        values.remove(index);
    }
    if ui.button(format!("{title} toevoegen")).clicked() {
        values.push(String::new());
    }
}
fn rule_fields(ui: &mut egui::Ui, rows: &mut Vec<RuleRow>) {
    ui.label("De eerste passende regel bepaalt laag, kleur en discipline. Scheid coderingen met komma of puntkomma.");
    let mut action = None;
    let count = rows.len();
    for (index, row) in rows.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    super::theme::strong(ui,format!("Regel {}", index + 1));
                    if ui
                        .add_enabled(index > 0, egui::Button::new("Omhoog"))
                        .clicked()
                    {
                        action = Some((index, -1));
                    }
                    if ui
                        .add_enabled(index + 1 < count, egui::Button::new("Omlaag"))
                        .clicked()
                    {
                        action = Some((index, 1));
                    }
                    if ui.button("Verwijderen").clicked() {
                        action = Some((index, 0));
                    }
                });
                ui.label("Woorden of codes");
                ui.text_edit_singleline(&mut row.words);
                ui.label("DXF-laagnaam");
                ui.text_edit_singleline(&mut row.layer);
                ui.label("Dwarsprofielnaam");
                ui.text_edit_singleline(&mut row.label);
                ui.horizontal(|ui| {
                    ui.label("Kleur");
                    ui.add(egui::DragValue::new(&mut row.color).range(1..=255));
                });
            });
        });
    }
    if let Some((index, direction)) = action {
        match direction {
            -1 => rows.swap(index, index - 1),
            1 => rows.swap(index, index + 1),
            _ => {
                rows.remove(index);
            }
        }
    }
    if ui.button("Regel toevoegen").clicked() {
        rows.push(RuleRow {
            words: String::new(),
            layer: "0".into(),
            label: String::new(),
            color: 7,
        });
    }
}
pub(super) fn control_style(ui: &mut egui::Ui) {
    let visuals=ui.visuals_mut();
    visuals.widgets.inactive.bg_stroke=egui::Stroke::new(1_f32,egui::Color32::from_rgb(175,190,210));
    visuals.widgets.inactive.corner_radius=6.into();
    for widget in [&mut visuals.widgets.hovered,&mut visuals.widgets.active] {
        widget.bg_stroke=egui::Stroke::new(1.5_f32,egui::Color32::from_rgb(83,99,122));
        widget.corner_radius=6.into();
    }
}
fn checkbox(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    ui.scope(|ui| {
        let style=ui.style_mut();
        style.spacing.icon_width=18.;style.spacing.icon_width_inner=12.;
        style.spacing.icon_spacing=9.;style.spacing.interact_size.y=30.;
        for visual in [&mut style.visuals.widgets.inactive,&mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active] {
            visual.bg_fill=egui::Color32::WHITE;
            visual.weak_bg_fill=egui::Color32::WHITE;
            visual.bg_stroke=egui::Stroke::new(1.5_f32,egui::Color32::from_rgb(83,99,122));
            visual.corner_radius=4.into();
            visual.fg_stroke=egui::Stroke::new(2_f32,egui::Color32::from_rgb(25,66,110));
        }
        ui.checkbox(value,label)
    }).inner
}
fn section(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1_f32,egui::Color32::from_rgb(208,218,231)))
        .corner_radius(10).inner_margin(16).show(ui,|ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y=10.;
            super::theme::strong(ui,title);
            ui.add_space(4.);
            content(ui);
        });
}
fn viewer_fields(ui: &mut egui::Ui, viewer: &mut ViewerDraft) {
    use sleufbase_rust::project_view::BackgroundProvider as Background;
    let display=|source:Background|source.label();
    egui::ComboBox::from_id_salt("settings-viewer-background")
        .selected_text(display(Background::from_label(&viewer.background_source))).show_ui(ui,|ui| {
            for source in [Background::PdokAerial,Background::PdokAerialCadastral,Background::OfflineAerial,Background::Cadastral,Background::OpenStreetMap,Background::OpenStreetMapCadastral] {
                if ui.selectable_label(Background::from_label(&viewer.background_source)==source,display(source)).clicked(){
                    viewer.background_source=source.label().into();
                }
            }
        });
    checkbox(ui,&mut viewer.use_background,"Achtergrondkaart tonen op het kaartscherm");
    ui.small("Na Opslaan wordt deze keuze toegepast en bewaard voor de volgende start en nieuwe projecten. Opgeslagen projecten behouden hun eigen kaartkeuze.");
}
fn pointcloud_fields(ui: &mut egui::Ui, state: &mut Dialog) {
    use sleufbase_rust::pointcloud::CloudCrs;
    section(ui, "Standaard coördinatenstelsel voor puntenwolken", |ui| {
        ui.label("Deze instelling wordt automatisch gebruikt voor nieuwe LAS/LAZ-imports, ook bij het downloaden van scans uit de KickTheMap-joblijst.");
        let current = &mut state.draft.pointcloud_default_crs;
        let label = |crs| match crs {
            CloudCrs::RdNew => "RD New (EPSG:28992) · standaard",
            CloudCrs::Wgs84 => "WGS 84 (EPSG:4326 / 4979)",
            CloudCrs::Other => "Ander / lokaal coördinatenstelsel",
            CloudCrs::Unknown => "Uit LAS/LAZ-metadata bepalen / onbekend",
        };
        egui::ComboBox::from_id_salt("pointcloud-default-crs")
            .selected_text(label(*current)).show_ui(ui, |ui| {
                for crs in [CloudCrs::RdNew, CloudCrs::Wgs84, CloudCrs::Other, CloudCrs::Unknown] {
                    ui.selectable_value(current, crs, label(crs));
                }
            });
        ui.small("RD New is de standaard. Coördinaten worden niet automatisch omgerekend. Als EPSG-metadata van een bron niet overeenkomt, verschijnt een foutmelding.");
        ui.small("Nieuwe puntenwolken worden met bronbestand en 3D-conversie in SleufBase AppData opgeslagen. Verwijderen uit de puntenwolkenlijst wist beide beheerde bestanden, niet het externe origineel.");
    });
}
fn working_map_fields(ui: &mut egui::Ui, state: &mut Dialog) {
    if let Some(viewer)=&mut state.viewer {
        super::theme::strong(ui,"Achtergrond op het kaartscherm");
        viewer_fields(ui,viewer);
        ui.separator();
    }
    let current_area=state.offline_area;
    const EXAMPLE_AREA:[i32;4]=[154500,462500,155500,463500];
    section(ui,"Offline luchtfoto en lokale kaart-back-up",|ui|{
        checkbox(ui,&mut state.draft.viewer.offline_aerial_auto_backup,
            "Offline luchtfoto automatisch bijwerken (elke 6 maanden)");
        ui.small("Automatisch bijwerken staat standaard uit. De ingebouwde overzichtsfoto blijft ook zonder download beschikbaar.");
        ui.horizontal_wrapped(|ui|{
            ui.label("Downloadgebied:");
            ui.radio_value(&mut state.draft.viewer.offline_aerial_whole_country,
                false,"Huidige kaartuitsnede");
            ui.radio_value(&mut state.draft.viewer.offline_aerial_whole_country,
                true,"Heel Nederland");
        });
        let whole=state.draft.viewer.offline_aerial_whole_country;
        let area=if whole {Some(sleufbase_rust::offline_zoom::NETHERLANDS_AREA)}
            else {current_area};
        let estimate_area=area.unwrap_or(EXAMPLE_AREA);
        ui.small(if whole {
            "Heel Nederland gebruikt een vaste RD-uitsnede. Op hoge detailniveaus is dit te groot; SleufBase verlaagt nooit zelf de gevraagde resolutie."
        } else {
            "De kaartuitsnede wordt vastgelegd zodra je Instellingen opent. Stel eerst het gewenste gebied in op de kaart."
        });
        ui.label("Kies het oorspronkelijke PDOK WMTS-zoomniveau");
        let zoom=state.draft.viewer.offline_aerial_zoom_level;
        egui::ComboBox::from_id_salt("offline-photo-zoom-level")
            .width(ui.available_width().min(490.))
            .selected_text(sleufbase_rust::offline_zoom::format_estimate(estimate_area,zoom))
            .show_ui(ui,|ui|{
                for level in sleufbase_rust::offline_zoom::MIN_ZOOM
                    ..=sleufbase_rust::offline_zoom::MAX_ZOOM {
                    ui.selectable_value(&mut state.draft.viewer.offline_aerial_zoom_level,
                        level,sleufbase_rust::offline_zoom::format_estimate(estimate_area,level));
                }
            });
        ui.label("Compressie van opgeslagen foto's");
        egui::ComboBox::from_id_salt("offline-photo-jpeg-quality")
            .selected_text(sleufbase_rust::offline_zoom::quality_label(state.draft.viewer.offline_aerial_quality))
            .show_ui(ui,|ui|{
                for quality in [100,95,85,70]{
                    ui.selectable_value(&mut state.draft.viewer.offline_aerial_quality,
                        quality,sleufbase_rust::offline_zoom::quality_label(quality));
                }
            });
        ui.small("Compressie verandert het aantal pixels niet: iedere tegel blijft 256 × 256 pixels. Kwaliteit 100 bewaart de originele PDOK-JPEG ongewijzigd.");
        let zoom=state.draft.viewer.offline_aerial_zoom_level;
        let estimate=area.and_then(|a|sleufbase_rust::offline_zoom::estimate(a,zoom));
        if let Some(area)=area {
            ui.small(format!("Gebied: {} · geschatte download {}. Er worden alleen de originele tegels van het gekozen zoomniveau opgehaald.",
                sleufbase_rust::offline_zoom::describe_area(area),
                estimate.map(|e|sleufbase_rust::offline_zoom::format_mb(e.expected_bytes)).unwrap_or_default()));
        }
        if let Some(size)=estimate.filter(|e|!e.allowed){
            ui.colored_label(egui::Color32::DARK_RED,
                format!("Dit gebied vraagt {} tegels, boven de limiet van {}. Kies een lager zoomniveau of kleinere uitsnede; de foto wordt niet automatisch verkleind.",
                    size.tiles,sleufbase_rust::offline_zoom::MAX_DOWNLOAD_TILES));
        }
        if zoom>=17 {
            ui.small("Niveau 17–19 kan meer pixels opleveren dan de bronfoto aan details bevat (PDOK: ongeveer 5–8 cm/pixel).");
        }
        if ui.add_enabled(!state.manual_offline_busy && estimate.is_some_and(|e|e.allowed),
            egui::Button::new("Offline luchtfoto downloaden voor offline gebruik")).clicked(){
            if let Some(area)=area {
                state.manual_offline_download=Some((zoom,area,state.draft.viewer.offline_aerial_quality));
            }
        }
        if let Some(status)=&state.manual_offline_status {ui.small(status);}
        ui.small("Snellere download met maximaal vier verbindingen. De hoogste resolutie blijft als originele JPEG beschikbaar; voor uitzoomen maakt SleufBase lokaal verkleinde weergavetegels zonder nieuwe downloads.");

        ui.separator();
        ui.strong("Gedownloade offline kaarten beheren");
        let mut requested=None;
        if state.offline_downloads.is_empty(){
            ui.small("Nog geen nieuwe WMTS-downloadgebieden geregistreerd. Via 'Alles verwijderen' kun je ook oudere offline-cachebestanden wissen.");
        }
        for download in &state.offline_downloads{
            ui.horizontal_wrapped(|ui|{
                ui.label(format!("Niveau {} · {} · {} · {}",
                    download.zoom,sleufbase_rust::offline_zoom::describe_area(download.area),
                    sleufbase_rust::offline_zoom::format_mb(download.bytes),
                    sleufbase_rust::offline_zoom::quality_label(download.quality)));
                if ui.add_enabled(!state.manual_offline_busy,
                    egui::Button::new("Verwijderen")).clicked(){
                    requested=Some(Some(download.id));
                }
            });
        }
        if ui.add_enabled(!state.manual_offline_busy,
            egui::Button::new("Alle gedownloade offline kaarten verwijderen"))
            .clicked(){requested=Some(None);}
        if let Some(id)=requested{state.offline_delete_confirm=Some(id);}
        if let Some(choice)=state.offline_delete_confirm{
            ui.colored_label(egui::Color32::DARK_RED,
                if choice.is_some(){"Dit offline kaartgebied verwijderen?"}
                else{"Alle offline kaarten en oude offline caches verwijderen?"});
            ui.horizontal(|ui|{
                if ui.button("Annuleren").clicked(){state.offline_delete_confirm=None;}
                if ui.add_enabled(!state.manual_offline_busy,
                    egui::Button::new("Ja, verwijderen")).clicked(){
                    state.offline_delete=Some(choice);
                    state.offline_delete_confirm=None;
                }
            });
        }
        if let Some(notice)=&state.offline_catalog_notice{ui.small(notice);}
    });
    ui.separator();
    checkbox(ui,&mut state.draft.viewer.show_trench_labels,"Naambordjes van proefsleuven tonen");
    ui.small("Toont namen bij TIFF-, KickTheMap-, MarXact- en virtuele proefsleuven. De bordjes zoeken vrije ruimte buiten de sleuven en TIFF-beelden; zonder ruimte worden ze verborgen.");
    ui.small("Deze keuze geldt alleen voor de werkkaart en wordt na Opslaan voor volgende sessies onthouden.");
}
fn map_fields(ui: &mut egui::Ui, state: &mut Dialog) {
    if ui.available_width()>=1000. {
        ui.columns(2,|columns| {
            section(&mut columns[0],"Kaartscherm",|ui| working_map_fields(ui,state));
            section(&mut columns[1],"Achtergrond voor kaartuitvoer en sjabloonkaarten",|ui| export_map_fields(ui,state));
        });
    } else {
        section(ui,"Kaartscherm",|ui| working_map_fields(ui,state));
        ui.add_space(12.);
        section(ui,"Achtergrond voor kaartuitvoer en sjabloonkaarten",|ui| export_map_fields(ui,state));
    }
}
fn export_map_fields(ui: &mut egui::Ui, state: &mut Dialog) {
    let background = &mut state.draft.map_export.background;
    let label = |value: &str| match value {
        "pdok" => "PDOK Luchtfoto NL actueel (8 cm, deels 5 cm)", "osm" => "OpenStreetMap", "cadastral" => "PDOK kadastrale kaart",
        "pdok_cadastral" => "PDOK Luchtfoto + percelen", "osm_cadastral" => "OpenStreetMap + percelen",
        "white" => "Wit (zonder internet)", _ => "Onbekende achtergrond",
    };
    egui::ComboBox::from_id_salt("settings-export-background").selected_text(label(background))
        .show_ui(ui, |ui| {
            for value in ["pdok", "pdok_cadastral", "cadastral", "osm", "osm_cadastral", "white"] {
                ui.selectable_value(background, value.into(), label(value));
            }
        });
    ui.add_space(12.);
    checkbox(ui, &mut state.draft.map_export.include_cad,
        "KLIC-overlays meenemen in kaartexport en sjabloonkaarten");
    ui.add_space(12.);
    super::theme::strong(ui,"A4-kaarten exporteren");
    ui.horizontal(|ui| {
        ui.label("Resolutie (DPI)");
        ui.add(egui::DragValue::new(&mut state.draft.map_export.dpi).range(30..=600));
    });
    ui.horizontal(|ui| {
        ui.label("Schaal 1:");
        ui.add(egui::DragValue::new(&mut state.draft.map_export.scale).range(1. ..=100_000.).speed(1.));
    });
    ui.small("Deze schaal en resolutie gelden voor Acties → Export kaarten.");
}
fn contact_fields(ui: &mut egui::Ui, contact: &mut sleufbase_rust::template_branding::ContactDetails) {
    ui.small("Deze gegevens komen in de Techbase-onderhoek op elk gebruikt sjabloonblad.");
    if ui.button("Standaard Techbase-gegevens herstellen").clicked() {
        *contact = Default::default();
    }
    super::theme::text_fields(ui, "settings-template-contact", [
        ("Adres", &mut contact.street),
        ("Postcode", &mut contact.postcode),
        ("Plaats", &mut contact.city),
        ("Telefoon", &mut contact.phone),
        ("E-mail", &mut contact.email),
        ("Website", &mut contact.website),
    ]);
    ui.small("Laat een veld leeg om die contactregel weg te laten. Klik op Opslaan om de wijzigingen toe te passen.");
}
fn template_fields(ui: &mut egui::Ui, state: &mut Dialog) {
    let current = state.draft.export_preferences.cadastral_template_path.trim();
    if current.is_empty() {
        ui.label(egui::RichText::new("Ingebouwd").strong());
        ui.small("Het originele SleufBase-sjabloon wordt gebruikt.");
    } else {
        ui.label(egui::RichText::new("Eigen sjabloon").strong());
        let file_name = std::path::Path::new(current).file_name()
            .map(|name| name.to_string_lossy()).unwrap_or_else(|| current.into());
        ui.add(egui::Label::new(file_name.as_ref()).truncate()).on_hover_text(current);
    }
    ui.small("Sla het ingebouwde sjabloon op om het in je CAD-programma te bewerken. Laad daarna de aangepaste DXF; deze wordt gecontroleerd en als eigen kopie bewaard.");
    ui.small("Bewaar als DXF voor AutoCAD 2007 of nieuwer en behoud de bestaande bladen, proefsleufvakken en vaste bloknamen.");
    ui.add_enabled_ui(state.template_task.is_none(), |ui| {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Ingebouwd sjabloon opslaan…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Ingebouwd DXF-sjabloon opslaan")
                    .add_filter("AutoCAD DXF", &["dxf"])
                    .set_file_name("SleufBase-ingebouwd-sjabloon.dxf")
                    .save_file()
                {
                    state.start_template_task(ui.ctx(), TemplateAction::ExportOriginal, path);
                }
            }
            if ui.button("Aangepast sjabloon laden…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Aangepast DXF-sjabloon laden")
                    .add_filter("AutoCAD DXF", &["dxf"])
                    .pick_file()
                {
                    state.start_template_task(ui.ctx(), TemplateAction::ImportCustom, path);
                }
            }
            if ui.add_enabled(!state.draft.export_preferences.cadastral_template_path.trim().is_empty(),
                egui::Button::new("Origineel herstellen")).clicked()
            {
                state.draft.export_preferences.cadastral_template_path.clear();
                state.error.clear();
                state.template_notice = Some(TemplateNotice {
                    text: "Origineel geselecteerd. Klik op Opslaan om deze keuze te bewaren.",
                    path: None,
                });
            }
        });
    });
    ui.small("Klik op Opslaan om je sjabloonkeuze voor volgende exports te bewaren. Annuleren behoudt de vorige keuze.");
}
fn client_logo_fields(ui: &mut egui::Ui, state: &mut Dialog) {
    ui.label("Voeg logo's van opdrachtgevers toe en geef ze een herkenbare naam. Bij de DXF-sjabloonexport kies je daarna een logo uit de lijst.");
    ui.small("PNG en JPEG worden als eigen kopie bewaard wanneer je op Opslaan klikt. Annuleren behoudt je vorige lijst.");
    let p = &mut state.draft.export_preferences;
    crate::ui_preferences::client_logo_default(ui, p);
    ui.add_space(8.);
    let mut remove = None;
    for (index, logo) in p.template_client_logos.iter_mut().enumerate() {
        ui.push_id(&logo.id, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Naam");
                let width = (ui.available_width() - 112.).clamp(100., 360.);
                ui.add(egui::TextEdit::singleline(&mut logo.name).desired_width(width))
                    .on_hover_text(logo.path.display().to_string());
                if ui.button("Verwijderen").clicked() { remove = Some(index); }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Afkorting op blad");
                ui.add(egui::TextEdit::singleline(&mut logo.client_code).desired_width(180.))
                    .on_hover_text("Tekst na PROJECTNR., bijvoorbeeld ENEXIS. Maximaal 24 tekens; leeg gebruikt de naam.");
            });
            ui.add_space(6.);
        });
    }
    if let Some(index) = remove {
        let removed = p.template_client_logos.remove(index);
        if p.template_client_logo_preset == removed.preset() {
            p.template_client_logo_preset.clear();
        }
        state.pending_logos.retain(|pending| pending.logo.id != removed.id);
    }
    let can_add = state.logo_task.is_none() && p.template_client_logos.len() < sleufbase_rust::client_logos::MAX_LOGOS;
    if ui.add_enabled(can_add, egui::Button::new("Logo toevoegen…")).clicked() {
        if let Some(path) = rfd::FileDialog::new().set_title("Opdrachtgeverlogo toevoegen")
            .add_filter("Logo (PNG of JPEG)", &["png", "jpg", "jpeg"]).pick_file() {
            state.start_logo_task(ui.ctx(), path);
        }
    }
    ui.small("Verwijderen haalt het logo uit de keuzelijst; eerder opgeslagen projecten houden hun afbeeldingsbestand.");
}
fn dxf_fields(ui: &mut egui::Ui, state: &mut Dialog) {
    section(ui,"DXF-sjabloon",|ui| template_fields(ui,state));
    ui.add_space(12.);
    section(ui,"Opdrachtgeverslogo's",|ui| client_logo_fields(ui,state));
    ui.add_space(12.);
    section(ui,"Weergave proefsleuf in kadastrale DXF-export",|ui| {
    let p = &mut state.draft.export_preferences;
    let mode_label = |mode: &str| match mode {
        "polygon" => "Witte vlakken per TIFF",
        "centerline" => "Detecteer proefsleuf als polyline (breedte 0.6 m)",
        "none" => "Geen", _ => "Onbekende weergave",
    };
    egui::ComboBox::from_label("Weergave proefsleuf")
        .selected_text(mode_label(&p.dxf_trench_export_mode))
        .show_ui(ui, |ui| {
            for (key, label) in [
                ("polygon", "Witte vlakken per TIFF"),
                ("centerline", "Detecteer proefsleuf als polyline (breedte 0.6 m)"),
                ("none", "Geen"),
            ] {
                ui.selectable_value(&mut p.dxf_trench_export_mode, key.into(), label);
            }
        });
    checkbox(ui,
        &mut p.include_tiffs_in_dxf_export,
        "GeoTIFF-afbeeldingen meenemen in DXF-export",
    );
    ui.small("Geen laat de hartlijnen en vlakken weg. Als je GeoTIFF-afbeeldingen meeneemt, blijft het PS-nummer of de proefsleufnaam naast iedere afbeelding staan.");
    ui.small("Geldt voor de kadastrale DXF-export en het overzicht in de sjabloonexport. De export zet gekoppelde TIFFs in een map naast het DXF-bestand. Detail- en profielvakken stel je apart in het exportvenster in.");
    });
    ui.add_space(12.);
    section(ui,"Labels en kleuren",|ui| {
    let p = &mut state.draft.export_preferences;
    ui.horizontal(|ui| {
        ui.label("Afstand van proefsleufnaam tot sleuf of TIFF (meter)");
        ui.add(egui::DragValue::new(&mut state.draft.dxf_label_gap).range(0.0..=100.).speed(0.1));
    });
    for (label, value) in [
        ("Kleur proefsleuf-hartlijn (0.6 m)", &mut p.dxf_centerline_color),
        ("Kleur PS-tekst in kadastrale DXF-export", &mut p.dxf_label_color),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.text_edit_singleline(value);
        });
    }
    });
    ui.add_space(12.);
    section(ui,"Contactgegevens op DXF-bladen",|ui| {
        contact_fields(ui, &mut state.draft.export_preferences.template_contact);
    });
}
fn trench_fields(ui: &mut egui::Ui, state: &mut Dialog) {
    super::theme::strong(ui,"Dwarsprofielen");
    let p = &mut state.draft.export_preferences;
    checkbox(ui,
        &mut p.template_include_cross_sections,
        "Dwarsprofielen automatisch maken vanuit KickTheMap",
    );
    checkbox(ui,
        &mut p.template_generate_reverse_cross_section_sheet,
        "Ook een omgekeerde versie van het sjabloon exporteren",
    );
    checkbox(ui,
        &mut p.template_show_profile_direction,
        "Toon 0-punt RD in afstandsband en referentiepunt op sjabloonkaart",
    );
    checkbox(ui,
        &mut p.template_enable_maaiveld_edit_points,
        "Kaartpunten gebruiken voor maaiveldtekst en kleur",
    );
    checkbox(ui,
        &mut p.template_avoid_multileader_collisions,
        "Leidingverwijzingen binnen het profiel houden en overlap beperken",
    );
    checkbox(ui,
        &mut p.template_clip_cross_section_markers_to_profile,
        "Verwijzingscirkels afknippen op de profielgrenzen",
    );
    ui.separator();
    super::theme::strong(ui,"Symbolen en opmaak");
    ui.horizontal(|ui| {
        ui.label("Diameter verwijzingssymbool dwarsprofiel (meter)");
        ui.add(
            egui::DragValue::new(&mut p.template_cross_section_marker_diameter)
                .range(0.005..=0.2)
                .speed(0.001),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Diametertekst (leeg = Ø)");
        ui.text_edit_singleline(&mut p.template_cross_section_diameter_text);
    });
    ui.small("Opdrachtgeverslogo's beheer je bij Instellingen > DXF-export.");
    ui.separator();
    super::theme::strong(ui,"Automatische uitvoer");
    checkbox(ui, &mut state.draft.template_auto_fill_bgt_fysiek_voorkomen,
        "Maaiveld automatisch invullen vanuit BGT");
    ui.small("Vult midden, links en rechts vanuit BGT. Handmatige tekst blijft staan.");
    checkbox(ui, &mut state.draft.template_auto_export_discipline_excel,
        "Automatisch discipline-Excel naast sjabloon-DXF");
}
/// Save callback must durably store BOTH candidates before returning Ok.
/// Caller applies returned Saved only. Cancel and window X return no candidate.
pub(crate) fn show(
    ctx: &egui::Context,
    dialog: &mut Option<Dialog>,
    mut save: impl FnMut(&MigratedSettings, &AutosaveSettings) -> Result<(), String>,
) -> Option<Saved> {
    let state = dialog.as_mut()?;
    state.poll_template_task(ctx);
    state.poll_logo_task(ctx);
    let mut open = true;
    let mut cancel = false;
    let mut requested = false;
    let viewport = ctx.screen_rect();
    let title = if state.words { "KickTheMap – woordenlijst" } else { "Instellingen" };
    let window_id = egui::Id::new("sleufbase-settings-dialog");
    // Older releases allowed this window to collapse. Keep those saved states
    // open as well, because the collapse control is no longer available.
    if let Some(mut collapse) = egui::collapsing_header::CollapsingState::load(ctx, window_id.with("collapsing")) {
        if !collapse.is_open() {
            collapse.set_open(true);
            collapse.store(ctx);
        }
    }
    egui::Window::new(title)
        .id(window_id)
        .fixed_pos(viewport.min + egui::vec2(12.,12.))
        .fixed_size(egui::vec2((viewport.width()-56.).max(180.),(viewport.height()-100.).max(140.)))
        // Position and size already reserve the frame/title margins. Automatic
        // Area constraints otherwise shift the first measured frames on hover.
        .constrain(false)
        .open(&mut open)
        .collapsible(false)
        .movable(false)
        .resizable(false)
        .show(ctx, |ui| {
            control_style(ui);
            ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
            // Footer is outside the scroll area, preserving access at every category.
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Annuleren").clicked() {
                        cancel = true;
                    }
                    if ui.add_enabled(state.template_task.is_none() && state.logo_task.is_none(),
                        egui::Button::new(egui::RichText::new("Opslaan").color(egui::Color32::WHITE))
                        .fill(super::ORANGE)).clicked() {
                        requested = true;
                    }
                    if state.words && ui.button("Terug naar instellingen").clicked() {
                        state.back();
                    }
                });
                if state.logo_task.is_some() {
                    ui.horizontal(|ui| { ui.spinner(); ui.label("Opdrachtgeverlogo controleren en laden…"); });
                }
                if let Some(task) = &state.template_task {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(task.action.progress());
                    });
                } else if let Some(notice) = &state.template_notice {
                    let response = ui.label(notice.text);
                    if let Some(path) = &notice.path {
                        response.on_hover_text(path.display().to_string());
                    }
                }
                if !state.error.is_empty() {
                    ui.colored_label(egui::Color32::DARK_RED, &state.error);
                }
                ui.separator();
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    if !state.words {
                        ui.horizontal_wrapped(|ui| {
                            for category in Category::ALL {
                                ui.selectable_value(
                                    &mut state.category,
                                    category,
                                    category.label(),
                                );
                            }
                        });
                        if state.project_scoped {
                            ui.small("Opslaan past deze instellingen toe op dit project en bewaart ze als standaard. Sla ook het project op om zijn instellingen te behouden. Back-ups gelden voor de hele app.");
                        }
                        ui.small(match state.category {
                            Category::Maps => "Kies de achtergrond voor het kaartscherm en voor je kaartuitvoer.",
                            Category::PointClouds => "Standaardcoördinaten, AppData-opslag en verwijderen van LAS/LAZ-puntenwolken.",
                            Category::Dxf => "Beheer je DXF-sjabloon, opdrachtgeverslogo’s, proefsleufweergave, kleuren en contactgegevens op de bladen.",
                            Category::Trenches => "Stel dwarsprofielen, maaiveld en automatische uitvoer in.",
                            Category::KickTheMap => "Beheer materiaalkeuzes, kabelnamen en de woordenlijst voor DXF-lagen.",
                            Category::Backups => "Stel automatisch opslaan in en herstel een eerdere versie.",
                        });
                        ui.separator();
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("settings_draft_content")
                        .max_height(ui.available_height())
                        .show(ui, |ui| {
                            if state.words {
                                section(ui,"Woorden, lagen en profielnamen",|ui| rule_fields(ui,&mut state.rules));
                                return;
                            }
                            match state.category {
                                Category::Backups => section(ui,"Automatisch opslaan en herstel",|ui| {
                                    super::theme::strong(ui,"Autosave");
                                    checkbox(ui,
                                        &mut state.autosave.enabled,
                                        "Automatisch opslaan",
                                    );
                                    ui.horizontal(|ui| {
                                        ui.label("Elke (minuten)");
                                        ui.add(
                                            egui::DragValue::new(
                                                &mut state.autosave.interval_minutes,
                                            )
                                            .range(1..=1440),
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Maximaal aantal autosaves");
                                        ui.add(
                                            egui::DragValue::new(&mut state.autosave.max_backups)
                                                .range(1..=200),
                                        );
                                    });
                                    if let Some(count) = state.autosave_count {
                                        ui.label(format!("Beschikbaar: {count}"));
                                    }
                                    if ui.button("Autosave laden…").clicked() {
                                        state.restore_autosave_requested = true;
                                    }
                                }),
                                Category::Maps => map_fields(ui, state),
                                Category::PointClouds => pointcloud_fields(ui, state),
                                Category::Dxf => dxf_fields(ui, state),
                                Category::Trenches => section(ui,"Proefsleuven en uitvoer",|ui| trench_fields(ui,state)),
                                Category::KickTheMap => section(ui,"Keuzes en naamkoppelingen",|ui| {
                                    super::theme::strong(ui,"KickTheMap woordenlijst");
                                    ui.label("Beheer woorden/codes, DXF-lagen, kleuren en dwarsprofielen in een ruime aparte weergave.");
                                    if ui.button("Woordenlijst beheren…").clicked() {
                                        state.open_words();
                                    }
                                    ui.separator();
                                    super::theme::strong(ui,"KickTheMap – kabels");
                                    list_editor(ui, "Materiaalkeuzes", &mut state.materials);
                                    ui.separator();
                                    list_editor(
                                        ui,
                                        "Extra Kabel/Leiding-keuzes",
                                        &mut state.extras,
                                    );
                                    ui.separator();
                                    super::theme::strong(ui,"MarXact namen → Kabel/Leiding");
                                    let mut remove = None;
                                    for (index, (source, target)) in
                                        state.mappings.iter_mut().enumerate()
                                    {
                                        ui.push_id(("mapping", index), |ui| {
                                            ui.horizontal(|ui| {
                                                ui.text_edit_singleline(source);
                                                ui.label("→");
                                                ui.text_edit_singleline(target);
                                                if ui.button("Verwijderen").clicked() {
                                                    remove = Some(index);
                                                }
                                            });
                                        });
                                    }
                                    if let Some(index) = remove {
                                        state.mappings.remove(index);
                                    }
                                    if ui.button("Naamkoppeling toevoegen").clicked() {
                                        state.mappings.push((String::new(), String::new()));
                                    }
                                }),
                            }
                        });
                });
            });
        });
    if !open || cancel {
        *dialog = None;
        return None;
    }
    if requested {
        if let Some(saved) = state.try_save(&mut save) {
            *dialog = None;
            return Some(saved);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewer_draft_is_returned_only_after_success_and_validated_before_persistence() {
        let mut dialog = Dialog::new(&MigratedSettings::default(), &AutosaveSettings::default());
        let viewer = ViewerDraft {background_source: "OpenStreetMap".into(), use_background: false};
        dialog.viewer = Some(viewer.clone());
        dialog.draft.map_export.background = "cadastral".into();
        assert!(dialog.try_save(&mut |_, _| Err("disk failure".into())).is_none());
        assert_eq!(dialog.viewer.as_ref(), Some(&viewer));
        let saved = dialog.try_save(&mut |settings, _| {
            assert_eq!(settings.map_export.background, "cadastral");
            Ok(())
        }).unwrap();
        assert_eq!(saved.viewer, Some(viewer));
        dialog.viewer.as_mut().unwrap().background_source = "removed-or-unknown".into();
        assert!(dialog.try_save(&mut |_, _| panic!("invalid viewer draft persisted")).is_none());
    }
    #[test]
    fn map_and_words_routes_render_original_controls_without_saving() {
        let ctx = egui::Context::default();
        let mut dialog = Some(Dialog::new(&MigratedSettings::default(), &AutosaveSettings::default()));
        dialog.as_mut().unwrap().viewer = Some(ViewerDraft {
            background_source: "OpenStreetMap".into(), use_background: true,
        });
        for words in [false, true] {
            dialog.as_mut().unwrap().words = words;
            let mut text = String::new();
            for _ in 0..3 {
                let output = ctx.run(egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 800.))),
                    ..Default::default()
                }, |ctx| { assert!(show(ctx, &mut dialog, |_, _| panic!("render must not save")).is_none()); });
                text = output.shapes.iter().filter_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape { Some(text.galley.job.text.as_str()) } else { None }
                }).collect::<Vec<_>>().join("\n");
            }
            if words {
                assert!(text.contains("KickTheMap – woordenlijst"));
                assert!(text.contains("Terug naar instellingen"));
                assert!(text.contains("Woorden of codes"));
                assert!(!text.contains("Achtergrond op het kaartscherm"));
            } else {
                let labels = ["Kaarten", "Puntenwolken", "DXF-export", "Proefsleuven", "KickTheMap", "Back-ups"];
                let positions = labels.map(|label| text.find(label).expect("original category rendered"));
                assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
                assert!(text.contains("Achtergrond op het kaartscherm"));
                assert!(text.contains("Achtergrond voor kaartuitvoer en sjabloonkaarten"));
                assert!(text.contains("OpenStreetMap"));
            }
        }
    }
    #[test]
    fn failed_save_retains_draft_and_success_returns_candidate_only() {
        let active = MigratedSettings::default();
        let backup = AutosaveSettings::default();
        let mut dialog = Dialog::new(&active, &backup);
        dialog.draft.dxf_label_gap = 8.;
        dialog.autosave.interval_minutes = 7;
        let mut calls = 0;
        assert!(dialog
            .try_save(&mut |_, _| {
                calls += 1;
                Err("injected disk failure".into())
            })
            .is_none());
        assert_eq!(calls, 1);
        assert_eq!(dialog.error, "injected disk failure");
        assert_eq!(active.dxf_label_gap, 3.5);
        assert_eq!(backup.interval_minutes, 10);
        let saved = dialog
            .try_save(&mut |candidate, autosave| {
                assert_eq!(candidate.dxf_label_gap, 8.);
                assert_eq!(autosave.interval_minutes, 7);
                Ok(())
            })
            .unwrap();
        assert_eq!(saved.settings.dxf_label_gap, 8.);
        assert_eq!(saved.autosave.interval_minutes, 7);
        assert_eq!(active.dxf_label_gap, 3.5);
    }
    #[test]
    fn category_and_values_survive_words_view_and_cancel() {
        let active = MigratedSettings::default();
        let mut dialog = Dialog::new(&active, &AutosaveSettings::default());
        dialog.category = Category::KickTheMap;
        dialog.materials.push("PVC".into());
        dialog.open_words();
        dialog.rules[0].words = "custom*".into();
        dialog.back();
        assert_eq!(dialog.category, Category::KickTheMap);
        assert_eq!(dialog.materials, ["PVC"]);
        assert!(!dialog.words);
        drop(dialog);
        assert_eq!(active.object_rules[0].keywords[0], "water");
        assert!(active.kickthemap_material_choices.is_empty());
    }
    #[test]
    fn choice_and_mapping_normalization_matches_original_first_wins_casefold() {
        let values = vec![
            " PVC ".into(),
            "pvc".into(),
            "".into(),
            "Straße".into(),
            "STRASSE".into(),
            "PE".into(),
        ];
        assert_eq!(choices(&values).unwrap(), ["PVC", "Straße", "PE"]);
        let map = mappings(&[
            ("  WATER\t LEIDING ".into(), " Water ".into()),
            ("water leiding".into(), "Other".into()),
            ("".into(), "skip".into()),
        ])
        .unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map["WATER LEIDING"], "Water");
    }
    #[test]
    fn oversized_editor_input_is_rejected_without_persistence_or_draft_loss() {
        for mapping in [false, true] {
            let active = MigratedSettings::default();
            let mut state = Dialog::new(&active, &AutosaveSettings::default());
            if mapping {
                state.mappings.push(("source".into(), "x".repeat(32769)));
            } else {
                state.materials.push("x".repeat(32769));
            }
            assert!(state
                .try_save(&mut |_, _| panic!("oversized draft persisted"))
                .is_none());
            assert!(!state.error.is_empty());
            assert_eq!(state.materials.len() + state.mappings.len(), 1);
            assert!(active.kickthemap_material_choices.is_empty());
            assert!(active.marxact_name_mappings.is_empty());
        }
    }
    #[test]
    fn compact_window_keeps_footer_visible_for_every_category_and_words() {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(680., 420.));
        let mut dialog = Some(Dialog::new(
            &MigratedSettings::default(),
            &AutosaveSettings::default(),
        ));
        // Force enough content to require scrolling in both list and words views.
        dialog.as_mut().unwrap().materials = vec!["PVC".into(); 80];
        for category in Category::ALL {
            dialog.as_mut().unwrap().category = category;
            for words in [false, true] {
                if words {
                    dialog.as_mut().unwrap().open_words();
                }
                let mut output = None;
                for _ in 0..3 {
                    output = Some(ctx.run(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ctx| {
                            assert!(
                                show(ctx, &mut dialog, |_, _| panic!("render must not save"))
                                    .is_none()
                            );
                        },
                    ));
                }
                let output = output.unwrap();
                let mut footer = Vec::new();
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape {
                        if matches!(text.galley.job.text.as_str(), "Opslaan" | "Annuleren") {
                            let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                            assert!(
                                screen.contains_rect(rect),
                                "footer outside viewport: {rect:?}"
                            );
                            assert!(
                                shape.clip_rect.contains_rect(rect),
                                "footer clipped by scroll: {rect:?}"
                            );
                            footer.push(rect);
                        }
                    }
                }
                assert_eq!(
                    footer.len(),
                    2,
                    "footer missing for {category:?}, words={words}"
                );
                if words {
                    dialog.as_mut().unwrap().back();
                    assert_eq!(dialog.as_ref().unwrap().category, category);
                }
            }
        }
    }
    #[test]
    fn invalid_rules_never_call_persistence() {
        let mut dialog = Dialog::new(&MigratedSettings::default(), &AutosaveSettings::default());
        dialog.rules[0].layer = "bad\nlayer".into();
        assert!(dialog
            .try_save(&mut |_, _| panic!("invalid draft persisted"))
            .is_none());
        assert!(!dialog.error.is_empty());
    }
    #[test]
    fn actual_footer_clicks_cancel_or_apply_only_after_successful_save() {
        for (label, fail) in [("Annuleren", false), ("Opslaan", true), ("Opslaan", false)] {
            let ctx = egui::Context::default();
            let mut dialog = Some(Dialog::new(
                &MigratedSettings::default(),
                &AutosaveSettings::default(),
            ));
            dialog.as_mut().unwrap().draft.dxf_label_gap = 9.;
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(680., 420.));
            let mut calls = 0;
            let mut saved = None;
            let mut button = None;
            let mut writer = |_: &MigratedSettings, _: &AutosaveSettings| {
                calls += 1;
                if fail {
                    Err("disk failure".into())
                } else {
                    Ok(())
                }
            };
            for _ in 0..3 {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ctx| {
                        saved = show(ctx, &mut dialog, &mut writer);
                    },
                );
                button = output.shapes.iter().find_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape {
                        (text.galley.job.text == label).then(|| {
                            egui::Rect::from_min_size(text.pos, text.galley.size()).center()
                        })
                    } else {
                        None
                    }
                });
            }
            let position = button.expect("footer button is rendered");
            for pressed in [true, false] {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        events: vec![
                            egui::Event::PointerMoved(position),
                            egui::Event::PointerButton {
                                pos: position,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            },
                        ],
                        ..Default::default()
                    },
                    |ctx| {
                        if let Some(value) = show(ctx, &mut dialog, &mut writer) {
                            saved = Some(value);
                        }
                    },
                );
            }
            if label == "Annuleren" {
                assert_eq!(calls, 0);
                assert!(dialog.is_none() && saved.is_none());
            } else if fail {
                assert_eq!(calls, 1);
                assert_eq!(dialog.as_ref().unwrap().error, "disk failure");
                assert!(saved.is_none());
            } else {
                assert_eq!(calls, 1);
                assert!(dialog.is_none());
                assert_eq!(saved.unwrap().settings.dxf_label_gap, 9.);
            }
        }
    }
    #[test]
    fn maps_tab_exposes_offline_export_and_resolution_at_normal_window_sizes() {
        for size in [egui::vec2(1200.,760.), egui::vec2(1500.,930.), egui::vec2(1920.,1080.)] {
            let ctx = egui::Context::default();
            let mut dialog = Some(Dialog::new(&MigratedSettings::default(), &AutosaveSettings::default()));
            dialog.as_mut().unwrap().project_scoped = true;
            let frame = |dialog: &mut Option<Dialog>, events| ctx.run(egui::RawInput {
                screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,size)), events, ..Default::default()
            }, |ctx| {assert!(show(ctx,dialog,|_,_| panic!("render must not save")).is_none());});
            frame(&mut dialog,vec![]);
            let output=frame(&mut dialog,vec![]);
            let position=|out:&egui::FullOutput,label:&str| out.shapes.iter().find_map(|s|
                if let egui::Shape::Text(t)=&s.shape {(t.galley.text()==label).then(|| t.galley.rect.translate(t.pos.to_vec2()).center())}else{None})
                .unwrap_or_else(||panic!("Missing {label} at {size:?}"));
            for label in ["Resolutie (DPI)","Schaal 1:","Opslaan","Annuleren"] {
                let p=position(&output,label);
                assert!(p.x>0. && p.x<size.x && p.y>0. && p.y<size.y);
            }
            let click=|dialog:&mut Option<Dialog>,pos| {for pressed in [true,false] {
                frame(dialog,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{
                    pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);
            }};
            click(&mut dialog,position(&output,"PDOK Luchtfoto NL actueel (8 cm, deels 5 cm)"));
            let popup=frame(&mut dialog,vec![]);
            click(&mut dialog,position(&popup,"Wit (zonder internet)"));
            assert_eq!(dialog.as_ref().unwrap().draft.map_export.background,"white");
            let saved=dialog.as_mut().unwrap().try_save(&mut |settings,_| {
                assert_eq!(settings.map_export.background,"white"); Ok(())
            }).unwrap();
            assert_eq!(saved.settings.map_export.background,"white");
        }
    }

    #[test]
    fn settings_geometry_does_not_grow_when_only_the_mouse_moves() {
        for (size,dpi) in [egui::vec2(680.,420.),egui::vec2(1200.,760.),egui::vec2(1500.,930.)]
            .into_iter().flat_map(|size| [1.,1.25,2.].map(move|dpi|(size,dpi))) {
            for (category,words) in Category::ALL.into_iter().map(|c|(c,false))
                .chain(std::iter::once((Category::KickTheMap,true))) {
                let ctx=egui::Context::default();
                super::super::theme::install(&ctx);
                ctx.set_pixels_per_point(dpi);
                let mut dialog=Some(Dialog::new(&MigratedSettings::default(),&AutosaveSettings::default()));
                dialog.as_mut().unwrap().category=category;
                dialog.as_mut().unwrap().words=words;
                dialog.as_mut().unwrap().viewer=Some(ViewerDraft{background_source:"OpenStreetMap".into(),use_background:true});
                dialog.as_mut().unwrap().project_scoped=true;
                let frame=|dialog:&mut Option<Dialog>,events| {ctx.run(egui::RawInput{
                    screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,size)),events,..Default::default()
                },|ctx| {show(ctx,dialog,|_,_|panic!("hover saved settings"));})};
                for _ in 0..2 {frame(&mut dialog,vec![]);}
                let rect=|| egui::AreaState::load(&ctx,egui::Id::new("sleufbase-settings-dialog")).unwrap().rect();
                let before=rect();
                for i in 0..16 {
                    frame(&mut dialog,vec![egui::Event::PointerMoved(egui::pos2(20.+i as f32*31.,50.+(i%7) as f32*43.))]);
                    assert_eq!(rect(),before,"hover changed {category:?} at {size:?}");
                }
                assert!(before.left()>=0.&&before.top()>=0.&&before.right()<=size.x&&before.bottom()<=size.y,
                    "settings outside viewport: {before:?} / {size:?}");
            }
        }
    }
    #[test]
    fn checkbox_icons_have_visible_outlines_and_both_states_toggle() {
        let ctx=egui::Context::default();let mut checked=false;let mut target=None;
        let frame=|value:&mut bool,events,target:&mut Option<egui::Rect>|ctx.run(egui::RawInput{
            screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(500.,200.))),events,..Default::default()
        },|ctx|{egui::CentralPanel::default().show(ctx,|ui|{*target=Some(checkbox(ui,value,"Zichtbare keuze").rect);});});
        frame(&mut checked,vec![],&mut target);
        for expected in [true,false] {
            let pos=target.unwrap().center();
            for pressed in [true,false] {
                frame(&mut checked,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{
                    pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}],&mut target);
            }
            assert_eq!(checked,expected);
            let out=frame(&mut checked,vec![],&mut target);
            assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r)
                if r.rect.width()>=17.&&r.rect.width()<=20.&&r.stroke.width>=1.
                    &&r.stroke.color.r()<150&&r.stroke.color.b()<180)),"checkbox outline missing");
        }
    }

}
