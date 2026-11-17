//! Editable MarXact source-name mappings, independent of the import dialog.
//! The original layer remains in attributes so a staged object can be remapped
//! repeatedly without losing survey names, heights, geometry or metadata.
use crate::{
    kickthemap_page_editor::PageConfig,
    marxact::normalize_name,
    python_values::trim,
    settings_compat::MigratedSettings,
    virtual_trench::{TrenchPoint, VirtualTrench},
};
use std::collections::BTreeMap;

pub const IGNORE: &str = "__IGNORE__";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub code: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameRow {
    pub key: String,
    pub source: String,
    pub count: usize,
    pub target: Option<String>,
    pub remembered: bool,
}

pub fn choices(settings: &MigratedSettings) -> Vec<Choice> {
    PageConfig::from_settings(settings).profile_options.into_iter()
        .map(|option| Choice { code: option.code, label: option.label }).collect()
}

pub fn source_name(point: &TrenchPoint) -> &str {
    let layer = trim(&point.attributes[1]);
    if !layer.is_empty() && layer != "0" { return layer; }
    let name = trim(&point.attributes[0]);
    if !name.is_empty() { name } else { trim(&point.source_name) }
}

fn display_name(value: &str) -> String {
    value.split(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
        .filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ")
}

fn option_code(value: &str, choices: &[Choice]) -> Option<String> {
    let key = normalize_name(value);
    if key == normalize_name(IGNORE) { return Some(IGNORE.into()); }
    choices.iter().find(|choice| normalize_name(&choice.code) == key)
        .or_else(|| choices.iter().find(|choice| normalize_name(&choice.label) == key))
        .map(|choice| choice.code.clone())
}

fn normalized(mappings: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for (source, target) in mappings {
        let key = normalize_name(source);
        let target = trim(target);
        if !key.is_empty() && !target.is_empty() {
            result.entry(key).or_insert_with(|| {
                if normalize_name(target) == normalize_name(IGNORE) { IGNORE.into() }
                else { target.to_owned() }
            });
        }
    }
    result
}

fn suggested(source: &str, settings: &MigratedSettings, choices: &[Choice]) -> Option<String> {
    // These exact original layer defaults precede generic keyword matching:
    // Gas_*_druk otherwise hits the sewer keyword "druk", and Laagspanning
    // hits "gs". A removed profile must not fall through to a wrong discipline.
    let label = match normalize_name(source).as_str() {
        "water" => Some("Waterleiding"),
        "datatransport" => Some("Datakabel"),
        "gas_lage_druk" => Some("Gasleiding LD"),
        "gas_hoge_druk" => Some("Gasleiding HD"),
        "laagspanning" => Some("Laagspanning"),
        "middenspanning" => Some("Middenspanning"),
        "riool_vrijerval" => Some("Riool vrijverval"),
        "riool_onder_over-_of_onderdruk" => Some("Riool druk"),
        _ => None,
    };
    if let Some(label) = label { return option_code(label, choices); }
    option_code(source, choices).or_else(|| {
        let rule = crate::matching_rule(source, &settings.object_rules)?;
        let code = rule.keywords.iter().map(|keyword| trim(keyword))
            .find(|keyword| !keyword.is_empty())?;
        option_code(code, choices)
    })
}

pub fn collect<'a>(
    objects: impl Iterator<Item = &'a TrenchPoint>,
    settings: &MigratedSettings,
    choices: &[Choice],
) -> Vec<NameRow> {
    let remembered = normalized(&settings.marxact_name_mappings);
    let mut rows = BTreeMap::<String, NameRow>::new();
    for point in objects {
        let source = display_name(source_name(point));
        let key = normalize_name(&source);
        if key.is_empty() { continue; }
        let row = rows.entry(key.clone()).or_insert_with(|| {
            let stored = remembered.get(&key);
            let target = match stored {
                Some(value) => option_code(value, choices),
                None => suggested(&source, settings, choices),
            };
            NameRow { key, source, count: 0, target, remembered: stored.is_some() }
        });
        row.count += 1;
    }
    rows.into_values().collect()
}

/// Apply only confirmed mappings. Filtering occurs after geometry was measured
/// from every original INSERT, so ignoring a utility cannot rotate the trench.
pub fn apply(trenches: &mut [VirtualTrench], mappings: &BTreeMap<String, String>) {
    let mappings = normalized(mappings);
    for trench in trenches {
        trench.objects.retain_mut(|point| {
            let Some(target) = mappings.get(&normalize_name(source_name(point))) else { return true; };
            if target == IGNORE { return false; }
            point.source_name.clone_from(target);
            true
        });
    }
}

/// Preserve unrelated remembered mappings while replacing case/whitespace
/// variants of the sources confirmed in this import.
pub fn merge(base: &mut BTreeMap<String, String>, additions: &BTreeMap<String, String>) {
    let mut result = normalized(base);
    result.extend(normalized(additions));
    *base = result;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Vertex;

    fn point(layer: &str, leader: &str, mapped: &str) -> TrenchPoint {
        TrenchPoint {
            metadata: serde_json::json!({"display_rgb": [12, 34, 56], "survey_id": "synthetic-1"}),
            role: "object".into(), object_name: leader.into(), source_name: mapped.into(),
            position: Vertex { x: 2., y: 1., z: Some(3.12) },
            attributes: [leader.into(), layer.into(), "marxact_point".into()],
        }
    }

    #[test]
    fn layer_identity_and_python_casefold_group_all_sources_before_remapping() {
        let mut settings = MigratedSettings::default();
        settings.marxact_name_mappings.insert("STRASSE KABEL".into(), "Waterleiding".into());
        let objects = [point(" Straße\u{1c} kabel ", "ignored leader", "old code"),
            point("strasse\tKABEL", "another leader", "different old code"),
            point("0", "  Unbekend meetobject  ", "ls")];
        assert_eq!(source_name(&objects[0]), "Straße\u{1c} kabel");
        assert_eq!(source_name(&objects[2]), "Unbekend meetobject");
        let rows = collect(objects.iter(), &settings, &choices(&settings));
        let grouped = rows.iter().find(|row| row.key == "strasse kabel").unwrap();
        assert_eq!(grouped.count, 2);
        assert_eq!(grouped.target.as_deref(), Some("water"));
        assert!(grouped.remembered);
        let unknown = rows.iter().find(|row| row.key == "unbekend meetobject").unwrap();
        assert_eq!(unknown.target, None);
        assert!(!unknown.remembered);
        assert_eq!(source_name(&point("0", "", "fallback")), "fallback");
    }

    #[test]
    fn explicit_marxact_defaults_precede_ambiguous_substrings_and_require_available_profiles() {
        let mut settings = MigratedSettings::default();
        let sources = [("Water", "water"), ("Datatransport", "data"),
            ("Gas_lage_druk", "ld"), ("Gas_hoge_druk", "hd"),
            ("Laagspanning", "ls"), ("Middenspanning", "ms"),
            ("Riool_vrijerval", "verval"), ("Riool_onder_over-_of_onderdruk", "rio")];
        let objects = sources.iter().map(|(source, _)| point(source, "leader", source)).collect::<Vec<_>>();
        let rows = collect(objects.iter(), &settings, &choices(&settings));
        for (source, expected) in sources {
            let row = rows.iter().find(|row| row.key == normalize_name(source)).unwrap();
            assert_eq!(row.target.as_deref(), Some(expected), "{source}");
        }
        settings.object_rules.retain(|rule| rule.label != "Gasleiding LD");
        let rows = collect(objects.iter(), &settings, &choices(&settings));
        assert_eq!(rows.iter().find(|row| row.key == "gas_lage_druk").unwrap().target, None,
            "a missing gas profile must not silently become pressure sewer");
    }

    #[test]
    fn remembered_choices_ignore_and_removed_targets_override_suggestions() {
        let mut settings = MigratedSettings::default();
        settings.kickthemap_profile_extra_choices = vec!["Speciale kabel".into()];
        settings.marxact_name_mappings = BTreeMap::from([
            ("Water".into(), "Removed profile".into()),
            ("Laagspanning".into(), " __ignore__ ".into()),
            ("Gas_hoge_druk".into(), "Speciale kabel".into()),
        ]);
        let objects = [point("Water", "", "water"), point("Laagspanning", "", "ls"),
            point("Gas_hoge_druk", "", "hd"), point("Speciale kabel", "", "old")];
        let rows = collect(objects.iter(), &settings, &choices(&settings));
        assert_eq!(rows.iter().find(|r| r.key == "water").unwrap().target, None);
        assert_eq!(rows.iter().find(|r| r.key == "laagspanning").unwrap().target.as_deref(), Some(IGNORE));
        assert_eq!(rows.iter().find(|r| r.key == "gas_hoge_druk").unwrap().target.as_deref(), Some("Speciale kabel"));
        assert!(rows.iter().find(|r| r.key == "water").unwrap().remembered);
        assert!(!rows.iter().find(|r| r.key == "speciale kabel").unwrap().remembered);
        assert_eq!(rows.iter().find(|r| r.key == "speciale kabel").unwrap().target.as_deref(), Some("Speciale kabel"));
    }

    #[test]
    fn applying_ignore_preserves_axis_boundary_heights_and_retained_object_metadata() {
        let start = Vertex { x: 0., y: 1., z: Some(4.) };
        let end = Vertex { x: 5., y: 1., z: Some(4.2) };
        let mut trench = VirtualTrench::new("Synthetic".into(), start, end, 2.).unwrap();
        trench.boundary_3d = vec![start, end, Vertex { x: 5., y: 2., z: Some(4.1) }];
        trench.alignment = Some(crate::marxact_alignment::MarXactAlignment::new(start, end, 2.));
        trench.metadata = serde_json::json!({"survey": "keep"});
        trench.objects = vec![point("Laagspanning", "original label", "previous"),
            point("0", "Unknown object", "not classified")];
        let mut expected = serde_json::to_value(&trench).unwrap();
        expected["objects"].as_array_mut().unwrap().truncate(1);
        expected["objects"][0]["source_name"] = serde_json::json!("ls");
        let mut trenches = [trench];
        apply(&mut trenches, &BTreeMap::from([
            (" LAAGSPANNING ".into(), "ls".into()), ("unknown object".into(), IGNORE.into()),
        ]));
        assert_eq!(serde_json::to_value(&trenches[0]).unwrap(), expected);
        // A later correction still uses the original layer, not the last code.
        apply(&mut trenches, &BTreeMap::from([("Laagspanning".into(), "ms".into())]));
        assert_eq!(trenches[0].objects[0].source_name, "ms");
        assert_eq!(trenches[0].objects[0].object_name, "original label");
    }

    #[test]
    fn merge_replaces_normalized_sources_and_preserves_unrelated_memory() {
        let mut base = BTreeMap::from([(" Straße\t KABEL ".into(), "old".into()),
            ("Water".into(), "water".into())]);
        merge(&mut base, &BTreeMap::from([("STRASSE KABEL".into(), " ls ".into()),
            (" Unknown ".into(), "__ignore__".into())]));
        assert_eq!(base, BTreeMap::from([("strasse kabel".into(), "ls".into()),
            ("water".into(), "water".into()), ("unknown".into(), IGNORE.into())]));
    }
}
