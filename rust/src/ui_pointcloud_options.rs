//! Keep the point-cloud type and material menus aligned with the trench editor.
use super::SleufBaseApp;
use sleufbase_rust::pointcloud_server::{EditorOptions, TypeChoice};

fn rule_color(rule: &sleufbase_rust::Rule) -> [u8; 3] {
    if rule.color_aci == 7 {
        [17, 24, 39]
    } else {
        sleufbase_rust::cad::aci_rgb(i32::from(rule.color_aci))
    }
}

impl SleufBaseApp {
    pub(super) fn pointcloud_editor_options(&self) -> Result<EditorOptions, String> {
        let settings = self.project_export_settings()?;
        let mut types: Vec<TypeChoice> = settings.object_rules.iter().map(|rule| {
            let label = rule.display_label().to_owned();
            let key = rule.keywords.first().map(|value| value.to_string())
                .unwrap_or_else(|| label.clone());
            TypeChoice { label, key, color: rule_color(rule) }
        }).collect();
        for value in sleufbase_rust::settings_compat::profile_extra_choices(
            &settings.kickthemap_profile_extra_choices, &settings.object_rules,
        ) {
            let color = sleufbase_rust::disciplines::matching_rule(value, &settings.object_rules)
                .map(rule_color).unwrap_or([17, 24, 39]);
            types.push(TypeChoice { label: value.to_owned(), key: value.to_owned(), color });
        }
        // Extra choices in older settings can be less constrained than rules.
        // Reject an unusable palette before drawing, instead of offering a
        // value which would later make every annotation save fail.
        if types.iter().any(|choice| [&choice.label, &choice.key].iter()
            .any(|value| value.len() > sleufbase_rust::pointcloud::MAX_ANNOTATION_TYPE_BYTES
                || value.chars().any(char::is_control))) {
            return Err("Een kabel-/leidingtype in de instellingen is langer dan 1024 UTF-8-bytes of bevat een ongeldig teken. Pas deze typekeuze aan voordat je de puntenwolkeditor opent.".into());
        }
        if settings.kickthemap_material_choices.iter()
            .any(|value| value.len() > 4096 || value.chars().any(char::is_control)) {
            return Err("Een materiaalkeuze in de instellingen is langer dan 4096 UTF-8-bytes of bevat een ongeldig teken. Pas deze materiaalkeuze aan voordat je de puntenwolkeditor opent.".into());
        }
        Ok(EditorOptions { types, materials: settings.kickthemap_material_choices })
    }
}
