//! Durable local source records, independent of projects and disposable octrees.
//!
//! Each source has its own atomically published JSON file and OS lock. Writes
//! compare the complete expected source under that lock, so a stale editor
//! cannot overwrite another session's changes. Original LAS/LAZ files are
//! never copied, moved, opened or deleted by this catalog.
use crate::pointcloud::{self, PointCloudSource};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, TryLockError},
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

const FORMAT: &str = "sleufbase-pointcloud-source";
const VERSION: u32 = 1;
const MAX_RECORD_BYTES: u64 = 32 * 1024 * 1024;
const MAX_LOAD_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CATALOG_SOURCES: usize = 4096;
const MAX_DIRECTORY_ENTRIES: usize = 10_000;

/// Callers can classify this optimistic-concurrency failure as HTTP 409.
pub const CONFLICT_MESSAGE: &str = "Deze puntenwolk is elders gewijzigd. Je nieuwe wijzigingen zijn nog niet bewaard. Herlaad het puntenwolkoverzicht voordat je de bron opnieuw bewerkt.";


#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub sources: Vec<PointCloudSource>,
    /// Reading reports damaged records and leaves their original bytes intact.
    pub errors: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: String,
    version: u32,
    source: PointCloudSource,
}

#[derive(Serialize)]
struct RecordRef<'a> {
    format: &'static str,
    version: u32,
    source: &'a PointCloudSource,
}

fn directory() -> PathBuf {
    crate::project::app_data_directory().join("pointcloud-catalog")
}

fn record_path(id: &str) -> Result<PathBuf, String> {
    if !pointcloud::valid_id(id) { return Err("Ongeldige puntenwolkidentiteit in de bronlijst.".into()); }
    Ok(directory().join(format!("{id}.json")))
}

fn check_directory(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err("De opgeslagen puntenwolklijst is geen gewone map.".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("De opgeslagen puntenwolklijst kan niet worden geopend: {error}")),
    }
}

struct LockedSource {
    path: PathBuf,
    // This handle stays alive across reading, comparison, syncing and publish.
    // Never remove the lock file: unlinking it would let later callers lock a
    // different inode while another process still holds the original lock.
    _file: File,
}

fn lock_source(id: &str) -> Result<LockedSource, String> {
    let path = record_path(id)?;
    let directory = directory();
    fs::create_dir_all(&directory)
        .map_err(|error| format!("De opgeslagen puntenwolklijst kan niet worden aangemaakt: {error}"))?;
    check_directory(&directory)?;
    let lock_path = path.with_extension("lock");
    match fs::symlink_metadata(&lock_path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err("De bronvergrendeling is geen gewoon bestand; de bron is niet gewijzigd.".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("De bronvergrendeling kan niet worden gelezen: {error}")),
    }
    let file = File::options().read(true).write(true).create(true).truncate(false).open(&lock_path)
        .map_err(|error| format!("De puntenwolk kan niet worden vergrendeld: {error}"))?;
    match file.try_lock() {
        Ok(()) => Ok(LockedSource { path, _file: file }),
        Err(TryLockError::WouldBlock) => Err("Deze puntenwolk wordt momenteel in een ander venster opgeslagen. Probeer zo opnieuw; je wijzigingen zijn nog niet bewaard.".into()),
        Err(TryLockError::Error(error)) => Err(format!("De puntenwolk kan niet worden vergrendeld: {error}")),
    }
}

fn record_size(path: &Path) -> Result<Option<u64>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            let size = metadata.len();
            if size == 0 || size > MAX_RECORD_BYTES {
                return Err("Het opgeslagen bronrecord is leeg of groter dan 32 MB.".into());
            }
            Ok(Some(size))
        }
        Ok(_) => Err("Het opgeslagen bronrecord is geen gewoon bestand.".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Het opgeslagen bronrecord kan niet worden gelezen: {error}")),
    }
}

fn read_record(path: &Path, id: &str) -> Result<Option<PointCloudSource>, String> {
    let Some(size) = record_size(path)? else { return Ok(None); };
    let file = File::open(path).map_err(|error| format!("Bronrecord openen mislukt: {error}"))?;
    let mut bytes = Vec::with_capacity(size.min(1024 * 1024) as usize);
    file.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes)
        .map_err(|error| format!("Bronrecord lezen mislukt: {error}"))?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("Het opgeslagen bronrecord is groter dan 32 MB.".into());
    }
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Het opgeslagen bronrecord is ongeldig: {error}"))?;
    if record.format != FORMAT || record.version != VERSION {
        return Err("Dit opgeslagen bronrecord gebruikt een niet ondersteund formaat.".into());
    }
    if record.source.id != id {
        return Err("De identiteit van het bronrecord komt niet overeen met de bestandsnaam.".into());
    }
    record.source.validate()?;
    Ok(Some(record.source))
}

/// Read available sources without requiring their original files or octrees.
/// A missing catalog is normal. Existing damaged records remain on disk and
/// produce visible errors alongside the sources which could be opened.
pub fn load() -> Result<Catalog, String> {
    let directory = directory();
    if !check_directory(&directory)? { return Ok(Catalog::default()); }
    let entries = fs::read_dir(&directory)
        .map_err(|error| format!("De opgeslagen puntenwolklijst kan niet worden gelezen: {error}"))?;
    let mut catalog = Catalog::default();
    let mut ids = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= MAX_DIRECTORY_ENTRIES {
            catalog.errors.push("De bronmap bevat te veel bestanden; niet alle bronnen zijn ingelezen.".into());
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => { catalog.errors.push(format!("Een bronrecord kon niet worden gevonden: {error}")); continue; }
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue; };
        let Some(id) = name.strip_suffix(".json") else { continue; };
        if !pointcloud::valid_id(id) {
            catalog.errors.push(format!("Bronrecord met ongeldige bestandsnaam overgeslagen: {name}"));
            continue;
        }
        ids.push(id.to_owned());
    }
    ids.sort();
    if ids.len() > MAX_CATALOG_SOURCES {
        catalog.errors.push(format!("Er zijn meer dan {MAX_CATALOG_SOURCES} opgeslagen bronnen; niet alle bronnen zijn ingelezen."));
        ids.truncate(MAX_CATALOG_SOURCES);
    }
    let mut total_bytes = 0_u64;
    for id in ids {
        let path = directory.join(format!("{id}.json"));
        let result = (|| {
            let Some(size) = record_size(&path)? else { return Ok(None); };
            total_bytes = total_bytes.saturating_add(size);
            if total_bytes > MAX_LOAD_BYTES {
                return Err("De bronlijst is groter dan 256 MB; dit bronrecord is niet ingelezen.".into());
            }
            read_record(&path, &id)
        })();
        match result {
            Ok(Some(source)) => catalog.sources.push(source),
            Ok(None) => {}
            Err(error) => catalog.errors.push(format!("Bron {id}: {error}")),
        }
    }
    catalog.sources.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    Ok(catalog)
}

pub fn get(id: &str) -> Result<Option<PointCloudSource>, String> {
    let path = record_path(id)?;
    if !check_directory(&directory())? { return Ok(None); }
    read_record(&path, id)
}

struct LimitedWriter<'a> {
    file: &'a mut File,
    remaining: u64,
}
impl Write for LimitedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Het bronrecord is groter dan 32 MB"));
        }
        let written = self.file.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> { self.file.flush() }
}

fn write_record(source: &PointCloudSource, locked: &LockedSource, overwrite: bool) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(&locked.path) {
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("Het bestaande bronrecord is geen gewoon bestand; het is niet vervangen.".into());
        }
    }
    let record = RecordRef { format: FORMAT, version: VERSION, source };
    crate::storage::atomic_write(&locked.path, overwrite, |file| {
        let mut writer = BufWriter::with_capacity(64 * 1024, LimitedWriter { file, remaining: MAX_RECORD_BYTES });
        serde_json::to_writer(&mut writer, &record)
            .map_err(|error| format!("De puntenwolk en tekenobjecten konden niet worden bewaard: {error}"))?;
        writer.write_all(b"\n").and_then(|()| writer.flush()).map_err(|error| error.to_string())
    })
}

/// Publish only if the complete saved source still matches the caller's edit
/// baseline. None creates a new source; Some is the unmodified original
/// snapshot, never a freshly fetched replacement for an outdated baseline.
/// A previously published identical candidate is an idempotent success.
pub fn save_if_current(source: &PointCloudSource, expected: Option<&PointCloudSource>) -> Result<(), String> {
    source.validate()?;
    if expected.is_some_and(|original| original.id != source.id) {
        return Err("De verwachte bronidentiteit hoort niet bij deze puntenwolk.".into());
    }
    let locked = lock_source(&source.id)?;
    let current = read_record(&locked.path, &source.id)?;
    if current.as_ref() == Some(source) { return Ok(()); }
    if current.as_ref() != expected { return Err(CONFLICT_MESSAGE.into()); }
    write_record(source, &locked, current.is_some())
}

/// Add sources from older project files without replacing newer saved edits.
/// The same per-source OS lock coordinates this migration with ordinary edits.
/// The persistent .lock file is inert after its handle closes, including after
/// a process crash; a leftover file is never interpreted as a held lock.
pub fn remember_if_missing(source: &PointCloudSource) -> Result<(), String> {
    source.validate()?;
    let locked = lock_source(&source.id)?;
    if read_record(&locked.path, &source.id)?.is_some() { return Ok(()); }
    write_record(source, &locked, false)
}
