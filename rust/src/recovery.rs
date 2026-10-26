//! Read-only autosave recovery catalog. Original contract:
//! autosave_restore_ui.py:list_autosaves/load_autosave/show_autosave_selector.
//! Recovery delegates to the ordinary loader; it never rewrites/prunes backups.
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, mpsc, Arc},
    time::SystemTime,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
    pub bytes: u64,
}
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub entries: Vec<Entry>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadKind {
    Native,
    Python,
}
/// Native and original Python backup directories are distinct; scanning never
/// creates either directory or reads original settings/accounts.
pub fn default_directories() -> Vec<PathBuf> {
    let mut dirs = vec![crate::project::backup_directory()];
    let python = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("SleufBase"))
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|p| p.join(".sleufbase"))
        });
    if let Some(python) = python {
        dirs.push(python.join("Backups"));
    }
    dirs
}
pub fn scan(directories: &[PathBuf], cancel: &AtomicBool) -> Result<Catalog, String> {
    let mut out = Catalog::default();
    let mut seen = std::collections::HashSet::new();
    let mut visited = 0usize;
    for directory in directories {
        crate::input::check(cancel)?;
        let reader = match std::fs::read_dir(directory) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                out.warnings.push(format!("{}: {e}", directory.display()));
                continue;
            }
        };
        for candidate in reader {
            crate::input::check(cancel)?;
            visited += 1;
            if visited > 50_000 {
                return Err(
                    "Back-upmappen bevatten te veel bestanden voor het hersteloverzicht".into(),
                );
            }
            let entry = match candidate {
                Ok(e) => e,
                Err(e) => {
                    out.warnings.push(e.to_string());
                    continue;
                }
            };
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("autosave_") || name.contains(".tmp") {
                continue;
            }
            // No recursive traversal or following symlinks into other directories.
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    out.warnings
                        .push(format!("{}: {e}", entry.path().display()));
                    continue;
                }
            };
            let path = entry.path();
            if !seen.insert(path.clone()) {
                continue;
            }
            if out.entries.len() >= 10_000 {
                return Err("Te veel autosaves voor het hersteloverzicht".into());
            }
            out.entries.push(Entry {
                path,
                modified: metadata.modified().ok(),
                bytes: metadata.len(),
            });
        }
    }
    out.entries.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| b.path.file_name().cmp(&a.path.file_name()))
            .then_with(|| b.path.cmp(&a.path))
    });
    Ok(out)
}
/// Inspect only the selected file in a worker. Extensions are insufficient:
/// original autosaves can be either .json or .klicproject. Ordinary loaders still
/// perform their complete version/schema/asset validation afterwards.
pub fn load_kind(path: &Path, cancel: &AtomicBool) -> Result<LoadKind, String> {
    let bytes = crate::input::read(path, crate::project::MAX_PROJECT_BYTES, "Autosave", cancel)?;
    #[derive(serde::Deserialize)]
    struct Header {
        format: Option<String>,
        version: u32,
    }
    let header: Header =
        serde_json::from_slice(&bytes).map_err(|e| format!("Ongeldige autosave: {e}"))?;
    crate::input::check(cancel)?;
    match (header.format.as_deref(), header.version) {
        (Some("sleufbase-native-rust"), 1..=crate::project::FORMAT_VERSION) => Ok(LoadKind::Native),
        (None, 1) => Ok(LoadKind::Python),
        _ => Err("Onbekend autosaveformaat of niet ondersteunde versie".into()),
    }
}
pub struct ScanWorker {
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<Catalog, String>>,
}
impl ScanWorker {
    pub fn start(directories: Vec<PathBuf>) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| scan(&directories, &signal))
                .unwrap_or_else(|_| Err("Hersteloverzicht kon niet worden gelezen".into()));
            let _ = tx.send(result);
        });
        Self { cancel, rx }
    }
    pub fn poll(&self) -> Option<Result<Catalog, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Hersteloverzicht is afgebroken".into()))
            }
        }
    }
    pub fn cancel(&self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
impl Drop for ScanWorker {
    fn drop(&mut self) {
        self.cancel();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_catalog_includes_both_formats_excludes_staging_and_preserves_files() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "autosave_a.json",
            "autosave_b.klicproject",
            "autosave_c.custom",
            "autosave_x.tmp.json",
            "normal.json",
        ] {
            std::fs::write(dir.path().join(name), b"{}").unwrap();
        }
        std::fs::create_dir(dir.path().join("autosave_directory")).unwrap();
        let c = scan(
            &[dir.path().into(), dir.path().into()],
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(c.entries.len(), 3);
        assert!(c.entries.iter().all(|e| e.bytes == 2));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 6);
    }
    #[test]
    fn selected_file_is_sniffed_by_schema_not_extension() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("autosave_wrong.klicproject");
        let c = AtomicBool::new(false);
        std::fs::write(&p, r#"{"format":"sleufbase-native-rust","version":5}"#).unwrap();
        assert_eq!(load_kind(&p, &c).unwrap(), LoadKind::Native);
        std::fs::write(&p, r#"{"version":1}"#).unwrap();
        assert_eq!(load_kind(&p, &c).unwrap(), LoadKind::Python);
        std::fs::write(&p, r#"{"version":9}"#).unwrap();
        assert!(load_kind(&p, &c).is_err());
        assert!(scan(&[dir.path().into()], &AtomicBool::new(true)).is_err());
    }
}
