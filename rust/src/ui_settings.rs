//! Transactional settings draft. No disk paths, accounts or active app state.
//! Original UI contracts: settings_navigation_patch.py and settings.py normalizers.
use eframe::egui;
use sleufbase_rust::{
    disciplines::Rule, project::AutosaveSettings, settings_compat::MigratedSettings,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    Maps,
    Dxf,
    Trenches,
    KickTheMap,
    Backups,
}
impl Category {
    const ALL: [Self; 5] = [
        Self::Maps,
        Self::Dxf,
        Self::Trenches,
        Self::KickTheMap,
        Self::Backups,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Backups => "Back-ups",
            Self::Maps => "Kaarten",
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
    rules: Vec<RuleRow>,
    materials: Vec<String>,
    extras: Vec<String>,
    mappings: Vec<(String, String)>,
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
        Self {
            draft: settings.clone(),
            autosave: autosave.clone(),
            category: Category::Maps,
            words: false,
            error: String::new(),
            viewer: None,
            autosave_count: None,
            project_scoped: false,
            restore_autosave_requested: false,
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
        if let Some(viewer) = &self.viewer {
            settings.viewer = sleufbase_rust::settings_compat::ViewerPreferences {
                background_source: viewer.background_source.clone(),
                use_background: viewer.use_background,
            };
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
        match self.candidate().and_then(|candidate| {
            save(&candidate.settings, &candidate.autosave)?;
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
fn map_fields(ui: &mut egui::Ui, state: &mut Dialog) {
    if let Some(viewer) = &mut state.viewer {
        use sleufbase_rust::project_view::BackgroundProvider as Background;
        super::theme::strong(ui,"Achtergrond op het kaartscherm");
        let display=|source:Background|match source {
            Background::PdokAerial=>"PDOK Luchtfoto NL actueel (8 cm, deels 5 cm)",Background::Cadastral=>"PDOK kadastrale kaart",
            Background::OpenStreetMap=>"OpenStreetMap",_=>"Onbekende achtergrond",
        };
        egui::ComboBox::from_id_salt("settings-viewer-background")
            .selected_text(display(Background::from_label(&viewer.background_source))).show_ui(ui, |ui| {
                for source in [Background::PdokAerial, Background::Cadastral, Background::OpenStreetMap] {
                    ui.selectable_value(&mut viewer.background_source, source.label().into(), display(source));
                }
            });
        ui.small("Na Opslaan wordt deze keuze toegepast en bewaard voor de volgende start en nieuwe projecten. Opgeslagen projecten behouden hun eigen kaartkeuze.");
        ui.checkbox(&mut viewer.use_background, "Achtergrondkaart tonen op het kaartscherm");
        ui.separator();
    }
    super::theme::strong(ui,"Achtergrond voor kaartuitvoer en sjabloonkaarten");
    let background = &mut state.draft.map_export.background;
    let label = |value: &str| match value {
        "pdok" => "PDOK Luchtfoto NL actueel (8 cm, deels 5 cm)", "osm" => "OpenStreetMap", "cadastral" => "PDOK kadastrale kaart",
        "white" => "Wit (zonder internet)", _ => "Onbekende achtergrond",
    };
    egui::ComboBox::from_id_salt("settings-export-background").selected_text(label(background))
        .show_ui(ui, |ui| {
            for value in ["pdok", "cadastral", "osm", "white"] {
                ui.selectable_value(background, value.into(), label(value));
            }
        });
    ui.add_space(12.);
    ui.checkbox(&mut state.draft.map_export.include_cad,
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
fn dxf_fields(ui: &mut egui::Ui, state: &mut Dialog) {
    let p = &mut state.draft.export_preferences;
    super::theme::strong(ui,"Weergave proefsleuf in kadastrale DXF-export");
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
    ui.checkbox(
        &mut p.include_tiffs_in_dxf_export,
        "GeoTIFF-afbeeldingen meenemen in DXF-export",
    );
    ui.small("De export zet hiervoor kopieën van de TIFFs in een map naast het DXF-bestand.");
    ui.horizontal(|ui| {
        ui.label("Afstand van proefsleufnaam tot hartlijn (meter)");
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
}
fn trench_fields(ui: &mut egui::Ui, state: &mut Dialog) {
    super::theme::strong(ui,"Proefsleuven & sjabloon");
    ui.small("Ingebouwd in de app en automatisch meegeleverd.");
    let p = &mut state.draft.export_preferences;
    ui.checkbox(
        &mut p.template_include_cross_sections,
        "Dwarsprofielen automatisch maken vanuit KickTheMap",
    );
    ui.checkbox(
        &mut p.template_generate_reverse_cross_section_sheet,
        "Ook een omgekeerde versie van het sjabloon exporteren",
    );
    ui.checkbox(
        &mut p.template_show_profile_direction,
        "Toon 0-punt RD in afstandsband en referentiepunt op sjabloonkaart",
    );
    ui.checkbox(
        &mut p.template_enable_maaiveld_edit_points,
        "Kaartpunten gebruiken voor maaiveldtekst en kleur",
    );
    ui.checkbox(
        &mut p.template_avoid_multileader_collisions,
        "Leidingverwijzingen binnen het profiel houden en overlap beperken",
    );
    ui.checkbox(
        &mut p.template_clip_cross_section_markers_to_profile,
        "Verwijzingscirkels afknippen op de profielgrenzen",
    );
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
    egui::ComboBox::from_label("Opdrachtgeverlogo op Blad1")
        .selected_text(&p.template_client_logo_preset)
        .show_ui(ui, |ui| {
            for (key, label) in [
                ("", "Geen"),
                ("brabant_water", "Brabant Water"),
                ("enexis", "Enexis"),
                ("wml", "WML"),
                ("hurkmans", "Hurkmans groep"),
                ("hanab", "Hanab"),
                ("bgm", "BGM"),
                ("apk", "APK"),
                ("custom", "Eigen bestand"),
            ] {
                ui.selectable_value(&mut p.template_client_logo_preset, key.into(), label);
            }
        });
    if p.template_client_logo_preset == "custom" {
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut p.template_client_logo_path);
            if ui.button("Kies bestand").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Afbeelding", &["png", "jpg", "jpeg"])
                    .pick_file()
                {
                    p.template_client_logo_path = path.to_string_lossy().into_owned();
                }
            }
        });
    }
    ui.separator();
    ui.checkbox(&mut state.draft.template_auto_fill_bgt_fysiek_voorkomen,
        "Maaiveld automatisch invullen vanuit BGT");
    ui.small("Vult midden, links en rechts vanuit BGT. Handmatige tekst blijft staan.");
    ui.checkbox(&mut state.draft.template_auto_export_discipline_excel,
        "Automatisch discipline-Excel naast sjabloon-DXF");
    ui.collapsing("Eigen DXF-sjabloon", |ui| {
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut p.cadastral_template_path);
            if ui.button("Kies bestand").clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("AutoCAD DXF", &["dxf"]).pick_file() {
                    p.cadastral_template_path = path.to_string_lossy().into_owned();
                }
            }
        });
    });
}
/// Save callback must durably store BOTH candidates before returning Ok.
/// Caller applies returned Saved only. Cancel and window X return no candidate.
pub(crate) fn show(
    ctx: &egui::Context,
    dialog: &mut Option<Dialog>,
    mut save: impl FnMut(&MigratedSettings, &AutosaveSettings) -> Result<(), String>,
) -> Option<Saved> {
    let state = dialog.as_mut()?;
    let mut open = true;
    let mut cancel = false;
    let mut requested = false;
    let viewport = ctx.screen_rect();
    let title = if state.words { "KickTheMap – woordenlijst" } else { "Instellingen" };
    egui::Window::new(title)
        .id(egui::Id::new("sleufbase-settings-dialog"))
        .default_pos(viewport.min + egui::vec2(12., 12.))
        .open(&mut open)
        .resizable(true)
        .default_width((viewport.width() - 40.).max(200.))
        .default_height((viewport.height() - 76.).max(160.))
        .max_width((viewport.width() - 32.).max(200.))
        .max_height((viewport.height() - 64.).max(160.))
        .show(ctx, |ui| {
            // Footer is outside the scroll area, preserving access at every category.
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Annuleren").clicked() {
                        cancel = true;
                    }
                    if ui.button("Opslaan").clicked() {
                        requested = true;
                    }
                    if state.words && ui.button("Terug naar instellingen").clicked() {
                        state.back();
                    }
                });
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
                            Category::Dxf => "Stel de proefsleufweergave, kleuren en gekoppelde rasters in.",
                            Category::Trenches => "Stel dwarsprofielen, maaiveld en het opdrachtgeverlogo in.",
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
                                rule_fields(ui, &mut state.rules);
                                return;
                            }
                            match state.category {
                                Category::Backups => {
                                    super::theme::strong(ui,"Autosave");
                                    ui.checkbox(
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
                                }
                                Category::Maps => map_fields(ui, state),
                                Category::Dxf => dxf_fields(ui, state),
                                Category::Trenches => trench_fields(ui, state),
                                Category::KickTheMap => {
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
                                }
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
                let labels = ["Kaarten", "DXF-export", "Proefsleuven", "KickTheMap", "Back-ups"];
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

}
