//! Recoverable publication of the two Rust-only preference files.
use serde::{Deserialize, Serialize};
use sleufbase_rust::{
    project::{write_autosave_settings, AutosaveSettings},
    settings_compat::{save_rust_settings, MigratedSettings},
};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const FILES: [&str; 2] = ["settings.json", "autosave-settings.json"];
const JOURNAL: &str = ".settings-journal.json";
const PREFIX: &str = ".settings-stage-";
const OWNER: &str = "owner.json";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    kind: String,
    version: u8,
    stage: String,
    existed: [bool; 2],
}
#[derive(Clone, Copy)]
enum Event {
    Publish(usize),
    Restore(usize),
}

struct Lock {
    _file: File,
    #[cfg(not(windows))]
    path: PathBuf,
}
#[cfg(not(windows))]
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
fn lock(root: &Path) -> Result<Lock, String> {
    let path = root.join(".settings.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.create(true).share_mode(0);
    }
    #[cfg(not(windows))]
    options.create_new(true);
    let file = options
        .open(&path)
        .map_err(|e| format!("Instellingen zijn in gebruik: {e}"))?;
    Ok(Lock {
        _file: file,
        #[cfg(not(windows))]
        path,
    })
}
fn regular(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(true),
        Ok(_) => Err(format!("Onveilig instellingenpad: {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}
fn replace(root: &Path, index: usize, bytes: &[u8]) -> Result<(), String> {
    regular(&root.join(FILES[index]))?;
    let mut temp = tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    temp.write_all(bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    temp.persist(root.join(FILES[index]))
        .map_err(|e| e.error.to_string())?;
    Ok(())
}
fn journal_exists(root: &Path) -> Result<bool, String> {
    regular(&root.join(JOURNAL))
}

/// Validate and serialize away from root before creating a lock or touching old bytes.
pub(crate) fn save_pair(
    root: &Path,
    settings: &MigratedSettings,
    autosave: &AutosaveSettings,
) -> Result<(), String> {
    save_with(root, settings, autosave, &mut |_| Ok(()))
}
fn save_with(
    root: &Path,
    settings: &MigratedSettings,
    autosave: &AutosaveSettings,
    hook: &mut impl FnMut(Event) -> Result<(), String>,
) -> Result<(), String> {
    let validated = tempfile::tempdir().map_err(|e| e.to_string())?;
    save_rust_settings(&validated.path().join(FILES[0]), settings)?;
    write_autosave_settings(&validated.path().join(FILES[1]), autosave)?;
    let new_bytes =
        FILES.map(|name| fs::read(validated.path().join(name)).map_err(|e| e.to_string()));
    let [a, b] = new_bytes;
    let new_bytes = [a?, b?];
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let _lock = lock(root)?;
    if journal_exists(root)? {
        recover_locked(root, hook)?;
    }
    let stage = tempfile::Builder::new()
        .prefix(PREFIX)
        .tempdir_in(root)
        .map_err(|e| e.to_string())?;
    let mut journal = Journal {
        kind: "sleufbase-rust-settings-pair".into(),
        version: 1,
        stage: stage
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .ok_or("Ongeldige stagingnaam")?
            .into(),
        existed: [false; 2],
    };
    for i in 0..2 {
        journal.existed[i] = regular(&root.join(FILES[i]))?;
        if journal.existed[i] {
            let bytes = fs::read(root.join(FILES[i])).map_err(|e| e.to_string())?;
            write_synced(&stage.path().join(format!("old{i}")), &bytes)?;
        }
        write_synced(&stage.path().join(format!("new{i}")), &new_bytes[i])?;
    }
    let body = serde_json::to_vec(&journal).map_err(|e| e.to_string())?;
    write_synced(&stage.path().join(OWNER), &body)?;
    let mut pending = tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    pending
        .write_all(&body)
        .and_then(|_| pending.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    // Keep recovery material before publishing the journal; no fallible step may
    // drop this directory once the journal can become visible.
    let stage_path = stage.keep();
    if let Err(e) = pending.persist_noclobber(root.join(JOURNAL)) {
        cleanup_stage(&stage_path, &journal);
        return Err(e.error.to_string());
    }
    let publish = (|| {
        for i in 0..2 {
            hook(Event::Publish(i))?;
            replace(root, i, &new_bytes[i])?;
        }
        fs::remove_file(root.join(JOURNAL)).map_err(|e| e.to_string())
    })();
    if let Err(error) = publish {
        return match recover_locked(root, hook) {
            Ok(()) => Err(format!("Instellingen opslaan mislukt; oude instellingen hersteld: {error}")),
            Err(rollback) => Err(format!("Instellingen opslaan mislukt: {error}; herstel mislukt (journal bewaard): {rollback}")),
        };
    }
    cleanup_stage(&stage_path, &journal);
    Ok(())
}

/// No root directory or lock is created when there is nothing to recover.
#[cfg(test)]
pub(crate) fn recover(root: &Path) -> Result<(), String> {
    if !journal_exists(root)? {
        return Ok(());
    }
    let _lock = lock(root)?;
    if journal_exists(root)? {
        recover_locked(root, &mut |_| Ok(()))?;
    }
    Ok(())
}
/// Readers share the writer lock so a second desktop process cannot load a
/// half-published pair between the two atomic file replacements.
pub(crate) fn read_pair(root: &Path) -> Result<(MigratedSettings, AutosaveSettings), String> {
    if !root.exists() {
        return Ok((MigratedSettings::default(), AutosaveSettings::default()));
    }
    let _lock = lock(root)?;
    if journal_exists(root)? {
        recover_locked(root, &mut |_| Ok(()))?;
    }
    Ok((
        sleufbase_rust::settings_compat::read_rust_settings(&root.join(FILES[0]))?,
        sleufbase_rust::project::read_autosave_settings(&root.join(FILES[1])),
    ))
}
fn read_journal(root: &Path) -> Result<(Journal, PathBuf), String> {
    let path = root.join(JOURNAL);
    if !regular(&path)? || fs::metadata(&path).map_err(|e| e.to_string())?.len() > 4096 {
        return Err("Ongeldig instellingenjournal".into());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let j: Journal =
        serde_json::from_slice(&bytes).map_err(|e| format!("Ongeldig instellingenjournal: {e}"))?;
    let suffix = j
        .stage
        .strip_prefix(PREFIX)
        .ok_or("Vreemd instellingenjournal")?;
    if j.kind != "sleufbase-rust-settings-pair"
        || j.version != 1
        || suffix.len() < 6
        || !suffix
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err("Vreemd instellingenjournal".into());
    }
    let stage = root.join(&j.stage);
    let meta = fs::symlink_metadata(&stage).map_err(|e| e.to_string())?;
    if !meta.file_type().is_dir() {
        return Err("Onveilige instellingenstage".into());
    }
    if !regular(&stage.join(OWNER))?
        || fs::read(stage.join(OWNER)).map_err(|e| e.to_string())? != bytes
    {
        return Err("Stage hoort niet bij instellingenjournal".into());
    }
    // Reject unexpected entries before changing either destination. Cleanup only
    // ever removes this closed list, never a recursively selected directory.
    for entry in fs::read_dir(&stage).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if ![OWNER, "new0", "new1", "old0", "old1"]
            .iter()
            .any(|allowed| name == *allowed)
            || !regular(&entry.path())?
        {
            return Err("Onbekende inhoud in instellingenstage".into());
        }
    }
    for i in 0..2 {
        if regular(&stage.join(format!("old{i}")))? != j.existed[i] {
            return Err("Onvolledige instellingenback-up".into());
        }
        regular(&root.join(FILES[i]))?;
    }
    Ok((j, stage))
}
fn recover_locked(
    root: &Path,
    hook: &mut impl FnMut(Event) -> Result<(), String>,
) -> Result<(), String> {
    let (journal, stage) = read_journal(root)?;
    // Read both backups before changing a destination.
    let mut old = [None, None];
    for i in 0..2 {
        if journal.existed[i] {
            old[i] = Some(fs::read(stage.join(format!("old{i}"))).map_err(|e| e.to_string())?);
        }
    }
    for i in 0..2 {
        hook(Event::Restore(i))?;
        if let Some(bytes) = &old[i] {
            replace(root, i, bytes)?;
        } else if regular(&root.join(FILES[i]))? {
            fs::remove_file(root.join(FILES[i])).map_err(|e| e.to_string())?;
        }
    }
    fs::remove_file(root.join(JOURNAL)).map_err(|e| e.to_string())?;
    cleanup_stage(&stage, &journal);
    Ok(())
}
fn cleanup_stage(stage: &Path, journal: &Journal) {
    for name in [OWNER, "new0", "new1", "old0", "old1"] {
        let _ = fs::remove_file(stage.join(name));
    }
    // remove_dir refuses unknown children; do not recurse into a foreign directory.
    if stage.file_name().and_then(|s| s.to_str()) == Some(journal.stage.as_str()) {
        let _ = fs::remove_dir(stage);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn seed(root: &Path) {
        fs::write(root.join(FILES[0]), b"old settings\r\n").unwrap();
        fs::write(root.join(FILES[1]), b"old autosave \n").unwrap();
    }
    fn assert_old(root: &Path) {
        assert_eq!(fs::read(root.join(FILES[0])).unwrap(), b"old settings\r\n");
        assert_eq!(fs::read(root.join(FILES[1])).unwrap(), b"old autosave \n");
    }
    #[test]
    fn success_matches_existing_writers() {
        let root = tempfile::tempdir().unwrap();
        let expected = tempfile::tempdir().unwrap();
        seed(root.path());
        let settings = MigratedSettings::default();
        let autosave = AutosaveSettings {
            interval_minutes: 0,
            ..Default::default()
        };
        save_pair(root.path(), &settings, &autosave).unwrap();
        save_rust_settings(&expected.path().join(FILES[0]), &settings).unwrap();
        write_autosave_settings(&expected.path().join(FILES[1]), &autosave).unwrap();
        for name in FILES {
            assert_eq!(
                fs::read(root.path().join(name)).unwrap(),
                fs::read(expected.path().join(name)).unwrap()
            );
        }
        assert!(!root.path().join(JOURNAL).exists());
        assert_eq!(read_pair(root.path()).unwrap().1.interval_minutes, 1);
    }
    #[cfg(windows)]
    #[test]
    fn exclusive_writer_also_blocks_readers_without_modifying_files() {
        let root = tempfile::tempdir().unwrap();
        seed(root.path());
        let held = lock(root.path()).unwrap();
        assert!(read_pair(root.path()).is_err());
        assert!(save_pair(
            root.path(),
            &MigratedSettings::default(),
            &AutosaveSettings::default()
        )
        .is_err());
        assert_old(root.path());
        drop(held);
        save_pair(
            root.path(),
            &MigratedSettings::default(),
            &AutosaveSettings::default(),
        )
        .unwrap();
        assert!(read_pair(root.path()).is_ok());
    }
    #[test]
    fn validation_failure_does_not_create_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("absent");
        let mut settings = MigratedSettings::default();
        settings.dxf_label_gap = f64::NAN;
        assert!(save_pair(&root, &settings, &AutosaveSettings::default()).is_err());
        assert!(!root.exists());
        recover(&root).unwrap();
        assert!(!root.exists());
    }
    #[test]
    fn second_publish_failure_restores_exact_bytes() {
        let root = tempfile::tempdir().unwrap();
        seed(root.path());
        assert!(save_with(
            root.path(),
            &MigratedSettings::default(),
            &AutosaveSettings::default(),
            &mut |event| {
                if matches!(event, Event::Publish(1)) {
                    Err("second publication failed".into())
                } else {
                    Ok(())
                }
            }
        )
        .is_err());
        assert_old(root.path());
        assert!(!root.path().join(JOURNAL).exists());
    }
    #[test]
    fn rollback_failure_keeps_material_and_recovery_repairs_half_publish() {
        let root = tempfile::tempdir().unwrap();
        seed(root.path());
        let error = save_with(
            root.path(),
            &MigratedSettings::default(),
            &AutosaveSettings::default(),
            &mut |event| {
                if matches!(event, Event::Publish(1) | Event::Restore(0)) {
                    Err("injected failure".into())
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
        assert!(error.contains("journal bewaard"));
        assert_ne!(
            fs::read(root.path().join(FILES[0])).unwrap(),
            b"old settings\r\n"
        );
        assert_eq!(
            fs::read(root.path().join(FILES[1])).unwrap(),
            b"old autosave \n"
        );
        let (_, stage) = read_journal(root.path()).unwrap();
        assert!(stage.join("old0").exists());
        recover(root.path()).unwrap();
        assert_old(root.path());
        assert!(!stage.exists());
    }
    #[test]
    fn invalid_and_foreign_journals_preserve_everything() {
        for stage in ["../outside", ".settings-stage-foreign"] {
            let root = tempfile::tempdir().unwrap();
            seed(root.path());
            let foreign = root.path().join(".settings-stage-foreign");
            fs::create_dir(&foreign).unwrap();
            fs::write(foreign.join("precious"), b"keep").unwrap();
            let j = Journal {
                kind: "sleufbase-rust-settings-pair".into(),
                version: 1,
                stage: stage.into(),
                existed: [true; 2],
            };
            let bytes = serde_json::to_vec(&j).unwrap();
            fs::write(root.path().join(JOURNAL), &bytes).unwrap();
            assert!(recover(root.path()).is_err());
            assert_old(root.path());
            assert_eq!(fs::read(root.path().join(JOURNAL)).unwrap(), bytes);
            assert_eq!(fs::read(foreign.join("precious")).unwrap(), b"keep");
        }
    }
    #[test]
    fn recovery_restores_original_absence() {
        let root = tempfile::tempdir().unwrap();
        assert!(save_with(
            root.path(),
            &MigratedSettings::default(),
            &AutosaveSettings::default(),
            &mut |event| {
                if matches!(event, Event::Publish(1) | Event::Restore(0)) {
                    Err("crash".into())
                } else {
                    Ok(())
                }
            }
        )
        .is_err());
        recover(root.path()).unwrap();
        for name in FILES {
            assert!(!root.path().join(name).exists());
        }
    }
}
