//! Choices for the original SleufBase controls inside KickTheMap's own form.
//!
//! Source contract: LukaStuurman/SleufBase at 0257f1d6, functions
//! `_profile_options`, `_material_options`, `_injected_script`, and
//! `merge_profile_dropdown_options`. Drawing and saving remain provider actions.
use crate::{python_values::{casefold, trim}, settings_compat::MigratedSettings};
use serde::Serialize;
use std::collections::HashSet;

const BRIDGE: &str = include_str!("kickthemap_page_editor.js");

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProfileOption {
    pub label: String,
    pub code: String,
    pub keywords: String,
}

/// Only display choices enter the page. This type contains no account or
/// session data and does not replace the provider's original input fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PageConfig {
    pub profile_options: Vec<ProfileOption>,
    pub material_options: Vec<String>,
}

impl Default for PageConfig {
    fn default() -> Self { Self::from_settings(&MigratedSettings::default()) }
}

impl PageConfig {
    /// Pass the effective project settings, or the migrated application
    /// defaults when the project has no overrides.
    pub fn from_settings(settings: &MigratedSettings) -> Self {
        let mut profile_options = Vec::new();
        let mut codes = HashSet::new();
        for rule in &settings.object_rules {
            let Some(code) = rule.keywords.iter().map(|word| trim(word))
                .find(|word| !word.is_empty()) else { continue; };
            if !codes.insert(code.to_lowercase()) { continue; }
            let label = trim(&rule.label);
            profile_options.push(ProfileOption {
                label: if label.is_empty() { code } else { label }.to_owned(),
                code: code.to_owned(),
                keywords: rule.keywords.join(","),
            });
        }
        // The original extra-choice patch deduplicates against visible labels
        // and stored codes. Other keyword aliases still remain valid matches.
        let mut identities: HashSet<String> = profile_options.iter()
            .flat_map(|option| [&option.code, &option.label]).map(|value| casefold(value)).collect();
        for choice in &settings.kickthemap_profile_extra_choices {
            let choice = trim(choice);
            if !choice.is_empty() && identities.insert(casefold(choice)) {
                profile_options.push(ProfileOption {
                    label: choice.to_owned(), code: choice.to_owned(), keywords: choice.to_owned(),
                });
            }
        }
        let mut materials = HashSet::new();
        let material_options = settings.kickthemap_material_choices.iter().filter_map(|value| {
            let value = trim(value);
            (!value.is_empty() && materials.insert(casefold(value))).then(|| value.to_owned())
        }).collect();
        Self { profile_options, material_options }
    }

    /// Install on each document load and evaluate again when choices change.
    /// The script is idempotent, cleans up its prior observer/controls, and only
    /// edits an unambiguous properties form on a trusted top-level job page.
    /// Choice text is JSON data; it is never interpolated as executable code.
    pub fn initialization_script(&self) -> Result<String, String> {
        let config = serde_json::to_string(self)
            .map_err(|_| "KickTheMap-keuzelijsten konden niet worden voorbereid.".to_owned())?;
        if BRIDGE.len() + config.len() + 3 > 256 * 1024 {
            return Err("KickTheMap-keuzelijsten zijn te groot voor het browservenster.".into());
        }
        Ok(format!("{BRIDGE}({config});"))
    }
}
