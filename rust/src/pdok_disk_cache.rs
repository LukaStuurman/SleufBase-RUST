//! Bounded cache of original, validated PDOK WMS image responses.
//! Unlike the in-memory RGBA tile cache, it survives an application restart.
//! All keys are derived from the exact HTTPS request URL (including RD BBOX,
//! source layer and output pixel dimensions), never user-provided filenames.
use super::{Source, MAX_RESPONSE_BYTES};
use ring::digest::{digest, SHA256};
use std::{
    fs, io::Write, path::PathBuf,
    sync::{mpsc::{self, SyncSender}, OnceLock, atomic::{AtomicBool, AtomicUsize, Ordering}},
    time::{Duration, SystemTime},
};

const ROOT: &str = "pdok-wms-cache-v1";
const MAX_CACHE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CACHE_FILES: usize = 2048;
const AERIAL_AGE: Duration = Duration::from_secs(6 * 60 * 60);
const CADASTRAL_AGE: Duration = Duration::from_secs(15 * 60);
static SAVES: AtomicUsize = AtomicUsize::new(0);

fn age(source: Source) -> Duration {
    match source {
        Source::Aerial | Source::AerialCadastral => AERIAL_AGE,
        Source::Cadastral | Source::CadastralOverlay => CADASTRAL_AGE,
    }
}
fn root() -> PathBuf {
    crate::project::app_data_directory().join(ROOT)
}
fn key(url: &reqwest::Url) -> String {
    let digest = digest(&SHA256, url.as_str().as_bytes());
    let mut name = String::with_capacity(68);
    name.push_str("wms-");
    for byte in digest.as_ref() {
        use std::fmt::Write as _;
        let _ = write!(&mut name, "{byte:02x}");
    }
    name
}
fn path(url: &reqwest::Url) -> PathBuf { root().join(format!("{}.img", key(url))) }

pub(super) fn lookup(source: Source, url: &reqwest::Url, cancel: &AtomicBool) -> Option<Vec<u8>> {
    if cfg!(test) || crate::input::check(cancel).is_err() { return None; }
    let file = path(url);
    let info = fs::symlink_metadata(&file).ok()?;
    if !info.file_type().is_file() || info.len() == 0 || info.len() > MAX_RESPONSE_BYTES as u64 {
        return None;
    }
    let elapsed = SystemTime::now().duration_since(info.modified().ok()?).ok()?;
    if elapsed > age(source) { return None; }
    let bytes = crate::input::read(&file, MAX_RESPONSE_BYTES as u64, "PDOK-cachebeeld", cancel).ok()?;
    crate::input::check(cancel).ok()?;
    Some(bytes)
}
pub(super) fn remove_invalid(url: &reqwest::Url) {
    if cfg!(test) { return; }
    let file = path(url);
    if fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_file()) {
        let _ = fs::remove_file(file);
    }
}
fn is_owned_image(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else { return false; };
    name.len() == 72 && name.starts_with("wms-") && name.ends_with(".img")
        && name.as_bytes()[4..68].iter().all(|c| c.is_ascii_hexdigit())
}
fn prune(root: &std::path::Path) {
    let Ok(entries) = fs::read_dir(root) else { return; };
    let mut images = Vec::new();
    let mut total = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_owned_image(&path) { continue; }
        let Ok(info) = fs::symlink_metadata(&path) else { continue; };
        if !info.file_type().is_file() { continue; }
        let modified = info.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        total = total.saturating_add(info.len());
        images.push((path, info.len(), modified));
    }
    if images.len() <= MAX_CACHE_FILES && total <= MAX_CACHE_BYTES { return; }
    images.sort_by_key(|(_, _, when)| *when);
    let mut count = images.len();
    for (path, bytes, _) in images {
        if count <= MAX_CACHE_FILES && total <= MAX_CACHE_BYTES { break; }
        if fs::remove_file(path).is_ok() {
            count -= 1;
            total = total.saturating_sub(bytes);
        }
    }
}
struct SaveJob {
    path: PathBuf,
    bytes: Vec<u8>,
}
fn sender() -> Option<&'static SyncSender<SaveJob>> {
    static WORKER: OnceLock<Option<SyncSender<SaveJob>>> = OnceLock::new();
    WORKER.get_or_init(|| {
        // Four queued maximum-size responses cap pending write buffers to
        // 96 MiB; cache writes can never block the map renderer or network.
        let (tx, rx) = mpsc::sync_channel::<SaveJob>(4);
        std::thread::Builder::new().name("sleufbase-pdok-tile-cache".into()).spawn(move || {
            while let Ok(job) = rx.recv() {
                let directory = root();
                let result = crate::storage::atomic_write(&job.path, true, |output| {
                    output.write_all(&job.bytes).map_err(|e| e.to_string())
                });
                if result.is_ok() && SAVES.fetch_add(1, Ordering::Relaxed) % 16 == 0 {
                    prune(&directory);
                }
            }
        }).ok().map(|_| tx)
    }).as_ref()
}
pub(super) fn remember(url: &reqwest::Url, bytes: Vec<u8>, cancel: &AtomicBool) {
    if cfg!(test) || bytes.is_empty() || bytes.len() > MAX_RESPONSE_BYTES
        || crate::input::check(cancel).is_err() {
        return;
    }
    let Some(tx) = sender() else { return; };
    // Best-effort bounded cache: never turn a successful map download into
    // an error or block the UI if storage is slow or full.
    let _ = tx.try_send(SaveJob { path: path(url), bytes });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_url_key_includes_resolution_and_bounds() {
        let a = reqwest::Url::parse("https://service.pdok.nl/example?w=512&bbox=0,0,1,1").unwrap();
        let b = reqwest::Url::parse("https://service.pdok.nl/example?w=1024&bbox=0,0,1,1").unwrap();
        let c = reqwest::Url::parse("https://service.pdok.nl/example?w=512&bbox=1,0,2,1").unwrap();
        assert_ne!(key(&a), key(&b));
        assert_ne!(key(&a), key(&c));
        assert!(is_owned_image(&path(&a)));
        assert!(!is_owned_image(&root().join("other.txt")));
        assert_eq!(age(Source::Cadastral), CADASTRAL_AGE);
        assert!(age(Source::Aerial) > age(Source::Cadastral));
    }
}
