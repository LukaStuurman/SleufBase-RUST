//! Named client logos. Imports stay temporary until the settings transaction
//! succeeds; saved image copies are immutable so old projects remain usable.
use crate::{export_control::Control, settings_compat::ExportPreferences};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, io::Write, path::{Path, PathBuf}};

pub const MAX_LOGOS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientLogo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub client_code: String,
    pub path: PathBuf,
}
impl ClientLogo {
    pub fn preset(&self) -> String { format!("saved:{}", self.id) }
    pub fn effective_code(&self) -> String {
        if self.client_code.trim().is_empty() { default_code(&self.name) }
        else { self.client_code.trim().to_owned() }
    }
}

pub fn validate_code(code: &str) -> Result<(), String> {
    if code.chars().count() > 24 || code.chars().any(|c| !(c.is_alphanumeric() || " .-_&/()".contains(c))) {
        return Err("Gebruik voor Afkorting op blad maximaal 24 letters, cijfers, spaties of .-_&/()".into());
    }
    Ok(())
}
pub fn default_code(name: &str) -> String {
    let value: String = crate::python_values::uppercase(name).chars()
        .filter(|c| c.is_alphanumeric() || " .-_&/()".contains(*c)).take(24).collect();
    if value.trim().is_empty() { "OPDRACHTGEVER".into() } else { value.trim().into() }
}
pub const BUILTIN_LOGOS: &[(&str, &str, &str, &str)] = &[
    ("brabant_water", "Brabant Water", "BW", "brabant_water.png"),
    ("enexis", "Enexis", "ENEXIS", "enexis.png"),
    ("wml", "WML", "WML", "wml.jpg"),
    ("hurkmans", "Hurkmans groep", "HUR", "hurkmans.png"),
    ("hanab", "Hanab", "HANAB", "hanab.jpg"),
    ("bgm", "BGM", "BGM", "bgm.png"),
    ("apk", "APK", "APK", "apk.png"),
];

pub fn validate(logos: &[ClientLogo]) -> Result<(), String> {
    if logos.len() > MAX_LOGOS {
        return Err(format!("Bewaar maximaal {MAX_LOGOS} opdrachtgeverslogo's"));
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for logo in logos {
        validate_code(logo.client_code.trim())?;
        let name = logo.name.trim();
        if logo.id.is_empty() || logo.id.len() > 64
            || !logo.id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            || !ids.insert(logo.id.as_str())
            || name.is_empty() || name.len() > 512 || name.chars().any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
            || !names.insert(crate::python_values::casefold(name))
            || logo.path.as_os_str().is_empty() || logo.path.as_os_str().len() > 32768
        {
            return Err("Geef ieder opdrachtgeverslogo een korte, unieke naam en een geldig afbeeldingsbestand".into());
        }
    }
    Ok(())
}

pub fn unique_name(base: &str, logos: &[ClientLogo]) -> String {
    let mut name = base.trim().to_owned();
    let mut suffix = 2;
    while logos.iter().any(|logo| crate::python_values::casefold(logo.name.trim())
        == crate::python_values::casefold(&name)) {
        name = format!("{} ({suffix})", base.trim());
        suffix += 1;
    }
    name
}

/// Dropping a cancelled draft also removes its temporary normalized image.
pub struct PendingLogo {
    pub logo: ClientLogo,
    _directory: tempfile::TempDir,
}
pub fn stage(source: &Path) -> Result<PendingLogo, String> {
    let directory = tempfile::Builder::new().prefix("logo-").tempdir()
        .map_err(|error| format!("Tijdelijke logomap maken mislukt: {error}"))?;
    let id = directory.path().file_name().and_then(|name| name.to_str())
        .ok_or("Ongeldige tijdelijke logonaam")?.to_owned();
    let path = directory.path().join("logo.png");
    crate::template_branding::prepare_logo(source, &path, &Control::default())
        .map_err(|error| format!("Logo laden mislukt: {error}"))?;
    let name = source.file_stem().map(|name| name.to_string_lossy()).unwrap_or_default()
        .chars().filter(|c| !c.is_control() && !matches!(c, '\u{2028}' | '\u{2029}')).take(120).collect::<String>();
    Ok(PendingLogo {
        logo: ClientLogo { id, client_code: default_code(&name), name: if name.trim().is_empty() { "Opdrachtgever".into() } else { name }, path },
        _directory: directory,
    })
}

/// Roll back only files created by this save attempt. Existing logo files and
/// arbitrary source paths are never deleted, including when a logo is removed
/// from the selector: older projects may still refer to those saved copies.
pub struct Publication { created: Vec<PathBuf>, committed: bool }
impl Publication {
    pub fn commit(mut self) { self.committed = true; }
}
impl Drop for Publication {
    fn drop(&mut self) {
        if !self.committed {
            for path in &self.created { let _ = fs::remove_file(path); }
        }
    }
}
pub fn publish_pending(
    root: &Path, preferences: &mut ExportPreferences, pending: &[PendingLogo],
) -> Result<Publication, String> {
    validate(&preferences.template_client_logos)?;
    let mut publication = Publication { created: Vec::new(), committed: false };
    for item in pending {
        let Some(logo) = preferences.template_client_logos.iter_mut()
            .find(|logo| logo.id == item.logo.id) else { continue; };
        let directory = root.join("client-logos");
        fs::create_dir_all(&directory).map_err(|error| format!("Logomap maken mislukt: {error}"))?;
        let destination = directory.join(format!("{}.png", item.logo.id));
        let mut input = fs::File::open(&item.logo.path)
            .map_err(|error| format!("Tijdelijk logo lezen mislukt: {error}"))?;
        let mut staged = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|error| format!("Logo opslaan mislukt: {error}"))?;
        std::io::copy(&mut input, &mut staged)
            .and_then(|_| staged.flush()).and_then(|_| staged.as_file().sync_all())
            .map_err(|error| format!("Logo opslaan mislukt: {error}"))?;
        staged.persist_noclobber(&destination)
            .map_err(|error| format!("Logo opslaan mislukt: {}", error.error))?;
        publication.created.push(destination.clone());
        logo.path = destination;
    }
    Ok(publication)
}

/// The catalog is global. A project keeps its selected logo, including older
/// custom paths, but cannot hide logos added later in another project.
pub fn apply_global_catalog(preferences: &mut ExportPreferences, global: &ExportPreferences) {
    let selected = preferences.template_client_logos.iter()
        .find(|logo| logo.preset() == preferences.template_client_logo_preset).cloned();
    if let Some(selected) = selected {
        if !global.template_client_logos.iter().any(|logo| logo.id == selected.id) {
            preferences.template_client_logo_preset = "custom".into();
            preferences.template_client_logo_path = selected.path.to_string_lossy().into_owned();
            preferences.template_client_logo_code = Some(selected.effective_code());
        }
    }
    preferences.template_client_logos = global.template_client_logos.clone();
}
