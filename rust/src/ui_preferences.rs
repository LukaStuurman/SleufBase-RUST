//! Native controls for the original non-secret export settings. Changes are
//! saved by the existing settings dialog's atomic save action.
use sleufbase_rust::{settings_compat::{ExportPreferences,MigratedSettings},template_branding::Logos};
use std::path::{Path,PathBuf};
use eframe::egui;

pub(super) fn fields(ui:&mut egui::Ui,preferences:&mut ExportPreferences){
    ui.collapsing("DXF- en sjabloonvoorkeuren",|ui|{
        egui::ComboBox::from_label("Proefsleuven in kadastraal overzicht").selected_text(match preferences.dxf_trench_export_mode.as_str(){"centerline"=>"Hartlijn","none"=>"Geen",_=>"Vlak"}).show_ui(ui,|ui|{for(key,label)in[("centerline","Hartlijn"),("polygon","Vlak"),("none","Geen")]{ui.selectable_value(&mut preferences.dxf_trench_export_mode,key.into(),label);}});
        ui.checkbox(&mut preferences.include_tiffs_in_dxf_export,"TIFF-afbeeldingen in kadastrale DXF opnemen");
        for(label,value)in[("Hartlijnkleur",&mut preferences.dxf_centerline_color),("Labelkleur",&mut preferences.dxf_label_color)]{ui.horizontal(|ui|{ui.label(label);ui.text_edit_singleline(value);});}
        ui.horizontal(|ui|{ui.label("Standaard DXF-sjabloon");ui.text_edit_singleline(&mut preferences.cadastral_template_path);if ui.button("Kiezen…").clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("AutoCAD DXF",&["dxf"]).pick_file(){preferences.cadastral_template_path=path.to_string_lossy().into_owned();}}});
        ui.checkbox(&mut preferences.template_include_cross_sections,"Dwarsprofielen opnemen");
        ui.checkbox(&mut preferences.template_show_profile_direction,"Profielrichting en referentie tonen");
        ui.checkbox(&mut preferences.template_enable_maaiveld_edit_points,"Bewerkte maaiveldpunten gebruiken");
        ui.checkbox(&mut preferences.template_avoid_multileader_collisions,"Botsingen tussen profiellabels vermijden");
        ui.checkbox(&mut preferences.template_clip_cross_section_markers_to_profile,"Objectmarkers binnen profiel houden");
        ui.horizontal(|ui|{ui.label("Markerdiameter (m)");ui.add(egui::DragValue::new(&mut preferences.template_cross_section_marker_diameter).range(0.005..=0.2).speed(0.001));});
        ui.horizontal(|ui|{ui.label("Diametertekst (leeg = Ø)");ui.text_edit_singleline(&mut preferences.template_cross_section_diameter_text);});
        client_logo_default(ui, preferences);
        ui.small("Opdrachtgeverslogo's toevoegen en beheren: Instellingen > DXF-export.");
    });
}
fn assets()->Option<PathBuf>{
    let mut roots=Vec::new();if let Ok(exe)=std::env::current_exe(){if let Some(parent)=exe.parent(){roots.push(parent.join("assets"));}}
    if let Ok(cwd)=std::env::current_dir(){roots.push(cwd.join("assets"));}
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets"));
    roots.into_iter().find(|p|p.join("template_logos").is_dir())
}
#[derive(Clone)]
pub(crate) struct LogoChoice {
    pub key: String,
    pub label: String,
    pub path: Option<PathBuf>,
    pub client_code: Option<String>,
}
fn logo_directory() -> PathBuf {
    assets().unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets"))
        .join("template_logos")
}
pub(crate) fn logo_choices(preferences: &ExportPreferences) -> Vec<LogoChoice> {
    let directory = logo_directory();
    let mut choices = vec![LogoChoice { key: String::new(), label: "Geen".into(), path: None, client_code: Some(String::new()) }];
    for &(key, label, code, file) in sleufbase_rust::client_logos::BUILTIN_LOGOS {
        choices.push(LogoChoice {
            key: key.into(), label: label.into(),
            path: Some(directory.join(file)), client_code: Some(code.into()),
        });
    }
    for logo in &preferences.template_client_logos {
        choices.push(LogoChoice { key: logo.preset(), label: format!("{} (eigen)", logo.name.trim()), path: Some(logo.path.clone()), client_code: Some(logo.effective_code()) });
    }
    if !preferences.template_client_logo_path.trim().is_empty() {
        let path = PathBuf::from(&preferences.template_client_logo_path);
        let client_code = choices.iter().find(|choice| choice.path.as_ref() == Some(&path))
            .and_then(|choice| choice.client_code.clone()).or_else(|| preferences.template_client_logo_code.clone());
        choices.push(LogoChoice { key: "custom".into(), label: "Eerder gekozen eigen logo".into(),
            path: Some(path), client_code });
    }
    choices
}
pub(crate) fn client_logo_default(ui: &mut egui::Ui, preferences: &mut ExportPreferences) {
    let choices = logo_choices(preferences);
    let selected = choices.iter().find(|choice| choice.key == preferences.template_client_logo_preset)
        .map_or("Onbekend logo", |choice| choice.label.as_str());
    ui.label("Standaard opdrachtgeverlogo");
    egui::ComboBox::from_id_salt("settings-client-logo")
        .width(ui.available_width().clamp(80., 300.))
        .truncate().selected_text(selected).show_ui(ui, |ui| {
            ui.set_width(ui.available_width().min(300.));
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            for choice in &choices {
                ui.selectable_value(&mut preferences.template_client_logo_preset, choice.key.clone(), &choice.label);
            }
        });
}
pub(crate) fn client_logo_selector(ui: &mut egui::Ui, path: &mut Option<PathBuf>, choices: &[LogoChoice]) {
    ui.label("Opdrachtgeverlogo");
    let selected = choices.iter().find(|choice| choice.path.as_ref() == path.as_ref())
        .map(|choice| choice.label.clone()).unwrap_or_else(|| {
            path.as_ref().and_then(|path| path.file_name())
                .map_or_else(|| "Geen".into(), |name| format!("Eigen logo: {}", name.to_string_lossy()))
        });
    egui::ComboBox::from_id_salt("template-client-logo")
        .width(ui.available_width().clamp(80., 300.))
        .truncate().selected_text(&selected).show_ui(ui, |ui| {
            ui.set_width(ui.available_width().min(300.));
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            // A previously saved custom path remains selectable when it is not
            // part of the current catalog. Opening the dialog never loses it.
            if path.is_some() && !choices.iter().any(|choice| choice.path.as_ref() == path.as_ref()) {
                let current = path.clone();
                ui.selectable_value(path, current, &selected);
            }
            for (index, choice) in choices.iter().enumerate() {
                if !choices[..index].iter().any(|previous| previous.path == choice.path) {
                    ui.selectable_value(path, choice.path.clone(), &choice.label);
                }
            }
        });
    ui.small("Logo toevoegen of beheren: Instellingen > DXF-export > Opdrachtgeverslogo's.");
}
pub(crate) fn sync_client_title(
    fields: &mut sleufbase_rust::template_metadata::TitleFields,
    path: &Option<PathBuf>, choices: &[LogoChoice],
) {
    if let Some(choice) = choices.iter().find(|choice| choice.path == *path) {
        if let Some(code) = &choice.client_code { fields.project_client = Some(code.clone()); }
    }
}
pub(super) fn logos(preferences:&ExportPreferences)->Logos{
    let directory=logo_directory();
    let techbase=Some(directory.join("techbase_full.png")).filter(|p|p.is_file());
    let client=logo_choices(preferences).into_iter()
        .find(|choice|choice.key==preferences.template_client_logo_preset)
        .and_then(|choice|choice.path);
    Logos{techbase,client}
}
pub(super) fn apply_order_defaults(state:&mut sleufbase_rust::template_order::State,settings:&MigratedSettings){
    state.overview_settings=sleufbase_rust::cadastral_overview::Settings::from_settings(settings).expect("validated project export settings");
    state.contact=settings.export_preferences.template_contact.clone();
    let p=&settings.export_preferences;state.template=(!p.cadastral_template_path.is_empty()).then(||p.cadastral_template_path.clone().into());
    state.profiles=p.template_include_cross_sections;
    state.map_background=Some(sleufbase_rust::template_project::BackgroundKind::from_setting(&settings.map_export.background).expect("validated project map background"));
    state.include_cad=settings.map_export.include_cad;
    // The preference adds a second sheet; it never reverses the base sheet.
    state.paired=p.template_generate_reverse_cross_section_sheet;
    state.show_reference=p.template_show_profile_direction;state.avoid_collisions=p.template_avoid_multileader_collisions;state.marker_diameter=p.template_cross_section_marker_diameter;
    state.use_custom_maaiveld=p.template_enable_maaiveld_edit_points;state.clip_markers=p.template_clip_cross_section_markers_to_profile;state.diameter_prefix=p.template_cross_section_diameter_text.clone();
    state.auto_bgt=settings.template_auto_fill_bgt_fysiek_voorkomen;state.label_gap=settings.dxf_label_gap;
    let next_logos=logos(p);
    if state.logos.client!=next_logos.client { state.title_fields.project_client=None; }
    state.logos=next_logos;
    sync_client_title(&mut state.title_fields, &state.logos.client, &logo_choices(p));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn order_dialog_defaults_include_profile_options_and_custom_logo(){
        let mut settings=MigratedSettings::default();
        let p=&mut settings.export_preferences;
        p.cadastral_template_path="chosen.dxf".into();p.template_client_logo_preset="custom".into();p.template_client_logo_path="chosen.png".into();
        p.template_include_cross_sections=false;p.template_generate_reverse_cross_section_sheet=true;
        p.template_cross_section_marker_diameter=0.06;p.template_clip_cross_section_markers_to_profile=false;
        p.template_cross_section_diameter_text="diameter".into();settings.template_auto_fill_bgt_fysiek_voorkomen=false;
        let mut state=sleufbase_rust::template_order::State::default();apply_order_defaults(&mut state,&settings);
        assert_eq!(state.template,Some(PathBuf::from("chosen.dxf")));
        assert_eq!(state.logos.client,Some(PathBuf::from("chosen.png")));
        assert!(!state.profiles);assert!(!state.reverse);assert!(state.paired);assert_eq!(state.marker_diameter,0.06);
        assert!(!state.clip_markers);assert!(!state.auto_bgt);assert_eq!(state.diameter_prefix,"diameter");
    }
}
