//! Local LAS/LAZ sources, durable survey annotations and offline Potree import.
//!
//! PotreeConverter is a pinned, bundled runtime component. Source files remain
//! untouched. Only complete, validated conversions are published to the cache.
use crate::model::{Dataset, Feature, Geometry, Vertex};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, VecDeque},
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const MAX_SOURCES: usize = 512;
pub const MAX_ANNOTATIONS: usize = 100_000;
pub const MAX_ANNOTATION_VERTICES: usize = 1_000_000;
pub const MAX_ANNOTATION_TYPE_BYTES: usize = 1024;
const MAX_METADATA_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PROJECTION_BYTES: usize = 1024 * 1024;
const STAMP_FILE: &str = "source-info.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudCrs {
    #[default]
    Unknown,
    RdNew,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationKind { Point, Polyline }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationPurpose {
    #[default]
    Object,
    TrenchStart,
    TrenchEnd,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub id: String,
    pub name: String,
    pub kind: AnnotationKind,
    pub vertices: Vec<Vertex>,
    #[serde(default)]
    pub purpose: AnnotationPurpose,
    /// Human-readable utility type from the active object palette.
    #[serde(default)]
    pub object_type: String,
    /// Stable rule keyword used by the native map/profile/DXF classification.
    #[serde(default)]
    pub type_key: String,
    /// Material, diameter, and bundle/description, matching existing objects.
    #[serde(default)]
    pub attributes: [String; 3],
}

impl Annotation {
    pub fn is_object(&self) -> bool { self.purpose == AnnotationPurpose::Object }

    /// Old untyped annotations retain their existing display name.
    pub fn display_name(&self) -> &str {
        if self.object_type.trim().is_empty() { &self.name } else { self.object_type.trim() }
    }

    /// Match the chosen type rather than incidental words in an editable name.
    pub fn source_name(&self) -> &str {
        if self.type_key.trim().is_empty() { self.display_name() } else { self.type_key.trim() }
    }

    /// Start/end references never enter the cable object layer or object DXF.
    /// Callers handling a complete source validate it before deriving features.
    pub fn to_feature(&self) -> Option<Feature> {
        if !self.is_object() { return None; }
        let geometry = match self.kind {
            AnnotationKind::Point => Geometry::Point(*self.vertices.first()?),
            AnnotationKind::Polyline if self.vertices.len() >= 2 => Geometry::Polyline(self.vertices.clone()),
            AnnotationKind::Polyline => return None,
        };
        Some(Feature {
            object_name: self.display_name().into(),
            source_name: self.source_name().into(),
            geometry,
            attributes: self.attributes.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointCloudSource {
    pub id: String,
    pub title: String,
    pub source_path: PathBuf,
    pub source_size: u64,
    /// SHA-256 of the complete input file; persists independently of the cache.
    #[serde(default)]
    pub source_fingerprint: String,
    pub point_count: u64,
    pub bounds_min: [f64; 3],
    pub bounds_max: [f64; 3],
    pub projection: String,
    #[serde(default)]
    pub crs: CloudCrs,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
}

pub(crate) fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn valid_text(value: &str, max: usize, allow_empty: bool) -> bool {
    value.len() <= max && (allow_empty || !value.trim().is_empty())
        && !value.chars().any(char::is_control)
}

pub fn validate_annotations(annotations: &[Annotation]) -> Result<(), String> {
    if annotations.len() > MAX_ANNOTATIONS {
        return Err("Te veel getekende puntenwolkobjecten".into());
    }
    let mut ids = HashSet::new();
    let mut count = 0usize;
    let mut trench_start = false;
    let mut trench_end = false;
    for annotation in annotations {
        if !valid_text(&annotation.id, 128, false) || !ids.insert(&annotation.id) {
            return Err("Een puntenwolkobject heeft een ongeldig of dubbel ID".into());
        }
        if !valid_text(&annotation.name, 512, false)
            || !valid_text(&annotation.object_type, MAX_ANNOTATION_TYPE_BYTES, true)
            || !valid_text(&annotation.type_key, MAX_ANNOTATION_TYPE_BYTES, true)
            || annotation.attributes.iter().any(|value| !valid_text(value, 4096, true)) {
            return Err("Een puntenwolkobject heeft een ongeldige naam of eigenschap".into());
        }
        let valid_count = match annotation.kind {
            AnnotationKind::Point => annotation.vertices.len() == 1,
            AnnotationKind::Polyline => annotation.vertices.len() >= 2,
        };
        count = count.saturating_add(annotation.vertices.len());
        if !valid_count || count > MAX_ANNOTATION_VERTICES {
            return Err("Een punt moet één positie hebben; een lijn minstens twee. Er zijn te veel lijnpunten.".into());
        }
        if !annotation.is_object() {
            if annotation.kind != AnnotationKind::Point {
                return Err("Het begin- en eindpunt van de proefsleuf moeten losse punten zijn.".into());
            }
            let (present, label) = match annotation.purpose {
                AnnotationPurpose::TrenchStart => (&mut trench_start, "beginpunt"),
                AnnotationPurpose::TrenchEnd => (&mut trench_end, "eindpunt"),
                AnnotationPurpose::Object => unreachable!(),
            };
            if *present { return Err(format!("Een puntenwolk mag maar één {label} van de proefsleuf hebben.")); }
            *present = true;
        }
        if annotation.vertices.iter().any(|vertex|
            !vertex.x.is_finite() || !vertex.y.is_finite()
                || vertex.z.is_none_or(|z| !z.is_finite())
                || vertex.x.abs() > 1e12 || vertex.y.abs() > 1e12
                || vertex.z.is_some_and(|z| z.abs() > 1e12)) {
            return Err("Een puntenwolkobject bevat ongeldige 3D-coördinaten".into());
        }
    }
    Ok(())
}

impl PointCloudSource {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) || !valid_text(&self.title, 1024, false)
            || self.source_path.as_os_str().is_empty()
            || self.source_path.as_os_str().len() > 32768 {
            return Err("Ongeldige puntenwolkbron".into());
        }
        if self.source_size < 227 || self.point_count == 0 {
            return Err("De puntenwolk bevat geen geldige LAS/LAZ-gegevens".into());
        }
        if !self.source_fingerprint.is_empty() && !valid_fingerprint(&self.source_fingerprint) {
            return Err("De bronidentiteit van de puntenwolk is ongeldig".into());
        }
        validate_bounds(self.bounds_min, self.bounds_max)?;
        if self.projection.len() > MAX_PROJECTION_BYTES || self.projection.contains('\0') {
            return Err("De coördinateninformatie van de puntenwolk is ongeldig".into());
        }
        if self.crs == CloudCrs::RdNew && projection_epsg(&self.projection).is_some_and(|code| code != 28992) {
            return Err("De bron vermeldt een ander coördinatenstelsel dan RD (EPSG:28992). Kies Ander / lokaal; een CRS kiezen verplaatst de scan niet.".into());
        }
        validate_annotations(&self.annotations)
    }

    /// Portable, stable identity for the derived map/DXF object layer.
    pub fn marker_path(&self) -> PathBuf {
        PathBuf::from(format!("pointcloud-{}.annotations.json", self.id))
    }

    /// Explicit survey references define the axis in start-to-end order.
    /// A partially drawn pair remains a valid source, but cannot create a trench.
    pub fn trench_endpoints(&self) -> Result<(&Annotation, &Annotation), String> {
        validate_annotations(&self.annotations)?;
        let start = self.annotations.iter().find(|a| a.purpose == AnnotationPurpose::TrenchStart);
        let end = self.annotations.iter().find(|a| a.purpose == AnnotationPurpose::TrenchEnd);
        match (start, end) {
            (Some(start), Some(end)) => Ok((start, end)),
            (None, None) => Err("Plaats eerst een beginpunt en een eindpunt van de proefsleuf op het maaiveld.".into()),
            (None, Some(_)) => Err("Het beginpunt van de proefsleuf ontbreekt. Plaats dit eerst op het maaiveld.".into()),
            (Some(_), None) => Err("Het eindpunt van de proefsleuf ontbreekt. Plaats dit eerst op het maaiveld.".into()),
        }
    }

    /// Only confirmed RD objects may enter the RD map. Native coordinates and
    /// their heights remain unchanged; declaring a CRS never reprojects data.
    pub fn to_dataset(&self) -> Result<Dataset, String> {
        self.validate()?;
        if self.crs != CloudCrs::RdNew {
            return Err("Bevestig eerst RD (EPSG:28992) voordat je objecten op de kaart overneemt.".into());
        }
        Ok(Dataset {
            job_id: 0,
            title: self.title.clone(),
            source_path: self.marker_path(),
            features: self.annotations.iter().filter_map(Annotation::to_feature).collect(),
        })
    }

    /// Missing originals do not prevent viewing a complete cached conversion.
    /// A changed original does: relink verifies its complete SHA-256 explicitly.
    pub fn validate_cache(&self) -> Result<PathBuf, String> {
        self.validate()?;
        let cloud = cloud_directory(&self.id)?;
        let stamp = read_stamp(&cloud)?;
        if !valid_fingerprint(&self.source_fingerprint)
            || stamp.fingerprint != self.source_fingerprint
            || stamp.size != self.source_size {
            return Err("De lokale puntenwolk hoort niet bij deze bron. Bouw de puntenwolk opnieuw op.".into());
        }
        let metadata = read_conversion(&cloud)?;
        validate_conversion_matches(&metadata, self.point_count, self.bounds_min, self.bounds_max)?;
        match fs::metadata(&self.source_path) {
            Ok(file) => {
                if !file.is_file() || file.len() != stamp.size || modified_time(&file)? != stamp.modified {
                    return Err("Het LAS/LAZ-bronbestand is gewijzigd. Kies Bron opnieuw koppelen om de inhoud te controleren, of importeer het als nieuwe puntenwolk.".into());
                }
                let current = inspect_las(&self.source_path)?;
                if current.sample_fingerprint != stamp.sample_fingerprint {
                    return Err("Het LAS/LAZ-bronbestand is gewijzigd. Importeer het als nieuwe puntenwolk.".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Het bronbestand kan niet worden gecontroleerd: {error}")),
        }
        Ok(cloud)
    }

    /// Repoint only to exactly the same file content, including after moving a
    /// project to another PC. This may read a large file; run it on a worker.
    pub fn relink(&mut self, path: &Path) -> Result<(), String> {
        self.relink_with_cancel(path, &AtomicBool::new(false))
    }

    pub fn relink_with_cancel(&mut self, path: &Path, cancel: &AtomicBool) -> Result<(), String> {
        self.validate()?;
        if !valid_fingerprint(&self.source_fingerprint) {
            return Err("De opgeslagen bronidentiteit ontbreekt. Importeer dit bestand als nieuwe puntenwolk.".into());
        }
        let inspected = inspect_las(path)?;
        let fingerprint = fingerprint_file(&inspected.path, cancel, |_| {})?;
        if fingerprint != self.source_fingerprint || inspected.size != self.source_size {
            return Err("Dit is een andere of gewijzigde puntenwolk. De bestaande punten en lijnen zijn niet verplaatst. Importeer dit bestand als nieuwe bron.".into());
        }
        let after = inspect_las(&inspected.path)?;
        if !inspected.same_file_state(&after) {
            return Err("Het bronbestand veranderde tijdens het controleren. Probeer opnieuw.".into());
        }
        let cloud = cloud_directory(&self.id)?;
        if cloud.parent().is_some_and(Path::is_dir) {
            write_stamp(&cloud, &SourceStamp::from_inspected(&after, fingerprint))?;
        }
        self.source_path = after.path;
        Ok(())
    }
}

pub fn is_cloud_dataset(dataset: &Dataset) -> bool {
    let Some(name) = dataset.source_path.to_str() else { return false; };
    name.strip_prefix("pointcloud-")
        .and_then(|value| value.strip_suffix(".annotations.json"))
        .is_some_and(valid_id)
}

/// Only application-controlled directories can provide executable components.
pub fn runtime_directory() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let installed = executable.parent().ok_or("De programmamap ontbreekt")?.join("pointcloud-runtime");
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("pointcloud-runtime");
    for candidate in [installed, development] {
        if candidate.join("converter/PotreeConverter.exe").is_file()
            && candidate.join("build/potree/potree.js").is_file()
            && candidate.join("libs/three.js/build/three.module.js").is_file() {
            return candidate.canonicalize().map_err(|error| format!("De puntenwolkonderdelen kunnen niet worden geopend: {error}"));
        }
    }
    Err("De meegeleverde puntenwolkonderdelen ontbreken. Installeer SleufBase opnieuw of pak de volledige portable ZIP uit.".into())
}

pub fn cloud_directory(id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) { return Err("Ongeldige puntenwolkidentiteit".into()); }
    Ok(crate::project::app_data_directory().join("pointclouds").join(id).join("cloud"))
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) { Err("Puntenwolk importeren geannuleerd".into()) } else { Ok(()) }
}

fn new_id() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| format!("Puntenwolkidentiteit aanmaken mislukt: {error}"))?;
    Ok(hex(&bytes))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes { let _ = write!(&mut result, "{byte:02x}"); }
    result
}

fn validate_bounds(min: [f64; 3], max: [f64; 3]) -> Result<(), String> {
    if (0..3).any(|index| !min[index].is_finite() || !max[index].is_finite()
        || min[index] > max[index] || min[index].abs() > 1e12 || max[index].abs() > 1e12) {
        return Err("De puntenwolk bevat ongeldige 3D-grenzen".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModifiedTime { seconds: u64, nanos: u32 }

fn modified_time(metadata: &fs::Metadata) -> Result<ModifiedTime, String> {
    let time = metadata.modified().map_err(|error| format!("Bestandsdatum lezen mislukt: {error}"))?
        .duration_since(UNIX_EPOCH).map_err(|error| format!("Ongeldige bestandsdatum: {error}"))?;
    Ok(ModifiedTime { seconds: time.as_secs(), nanos: time.subsec_nanos() })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceStamp {
    size: u64,
    modified: ModifiedTime,
    fingerprint: String,
    sample_fingerprint: String,
}

impl SourceStamp {
    fn from_inspected(source: &InspectedLas, fingerprint: String) -> Self {
        Self { size: source.size, modified: source.modified.clone(), fingerprint,
            sample_fingerprint: source.sample_fingerprint.clone() }
    }
}

fn read_stamp(cloud: &Path) -> Result<SourceStamp, String> {
    let path = cloud.parent().ok_or("De puntenwolkmap ontbreekt")?.join(STAMP_FILE);
    let file = File::open(path).map_err(|_| "De lokale broninformatie ontbreekt. Bouw de puntenwolk opnieuw op.".to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > 8192 {
        return Err("De lokale broninformatie is ongeldig".into());
    }
    serde_json::from_reader(file).map_err(|_| "De lokale broninformatie is ongeldig. Bouw de puntenwolk opnieuw op.".into())
}

fn write_stamp(cloud: &Path, stamp: &SourceStamp) -> Result<(), String> {
    let path = cloud.parent().ok_or("De puntenwolkmap ontbreekt")?.join(STAMP_FILE);
    crate::storage::atomic_write(&path, true, |file| {
        serde_json::to_writer(file, stamp).map_err(|error| error.to_string())
    })
}

struct InspectedLas {
    path: PathBuf,
    size: u64,
    modified: ModifiedTime,
    point_count: u64,
    bounds_min: [f64; 3],
    bounds_max: [f64; 3],
    projection: String,
    epsg: Option<u32>,
    sample_fingerprint: String,
}

impl InspectedLas {
    fn same_file_state(&self, other: &Self) -> bool {
        self.size == other.size && self.modified == other.modified
            && self.sample_fingerprint == other.sample_fingerprint
    }
}

fn read_exact_at(file: &mut File, offset: u64, bytes: &mut [u8]) -> Result<(), String> {
    file.seek(SeekFrom::Start(offset)).and_then(|_| file.read_exact(bytes))
        .map_err(|error| format!("LAS/LAZ-bestand lezen mislukt: {error}"))
}
fn u16_at(bytes: &[u8], offset: usize) -> u16 { u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()) }
fn u32_at(bytes: &[u8], offset: usize) -> u32 { u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) }
fn u64_at(bytes: &[u8], offset: usize) -> u64 { u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()) }
fn f64_at(bytes: &[u8], offset: usize) -> f64 { f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()) }

/// Read only the standard LAS header and CRS VLRs. Actual point decoding,
/// including LAS 1.4 formats and LAZ compression, belongs to PotreeConverter.
fn inspect_las(path: &Path) -> Result<InspectedLas, String> {
    let extension = path.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    if !matches!(extension.as_str(), "las" | "laz") {
        return Err("Kies een LAS- of LAZ-bestand".into());
    }
    let path = path.canonicalize().map_err(|error| format!("LAS/LAZ-bestand niet gevonden: {error}"))?;
    let mut file = File::open(&path).map_err(|error| format!("LAS/LAZ-bestand openen mislukt: {error}"))?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() < 227 { return Err("Geen geldig LAS/LAZ-bestand".into()); }
    let mut header = [0_u8; 375];
    read_exact_at(&mut file, 0, &mut header[..227])?;
    if &header[..4] != b"LASF" || header[24] != 1 || header[25] > 4 {
        return Err("Dit bestand heeft geen ondersteunde LAS 1.0–1.4-header".into());
    }
    let header_size = u16_at(&header, 94) as u64;
    let minimum_header = match header[25] { 4 => 375, 3 => 235, _ => 227 };
    let data_offset = u32_at(&header, 96) as u64;
    if header_size < minimum_header || data_offset < header_size || data_offset >= metadata.len() {
        return Err("De LAS/LAZ-header of puntenoffset is ongeldig".into());
    }
    if minimum_header > 227 {
        read_exact_at(&mut file, 227, &mut header[227..minimum_header as usize])?;
    }
    let point_format = header[104] & 0x3f;
    let min_lengths = [20_u16, 28, 26, 34, 57, 63, 30, 36, 38, 59, 67];
    if point_format > 10 || u16_at(&header, 105) < min_lengths[point_format as usize] {
        return Err("Het LAS/LAZ-puntformaat wordt niet ondersteund of is beschadigd".into());
    }
    let legacy_count = u32_at(&header, 107) as u64;
    let extended_count = if header[25] == 4 { u64_at(&header, 247) } else { 0 };
    if legacy_count > 0 && extended_count > 0 && legacy_count != extended_count {
        return Err("De aantallen punten in de LAS-header spreken elkaar tegen. Laat het bronbestand herstellen.".into());
    }
    let point_count = if extended_count > 0 { extended_count } else { legacy_count };
    if point_count == 0 { return Err("Het LAS/LAZ-bestand bevat geen punten".into()); }
    for index in [131, 139, 147] {
        let scale = f64_at(&header, index);
        if !scale.is_finite() || scale <= 0. { return Err("De coördinaatschaal van het LAS/LAZ-bestand is ongeldig".into()); }
    }
    for index in [155, 163, 171] {
        if !f64_at(&header, index).is_finite() { return Err("De coördinaatoffset van het LAS/LAZ-bestand is ongeldig".into()); }
    }
    let bounds_min = [f64_at(&header, 187), f64_at(&header, 203), f64_at(&header, 219)];
    let bounds_max = [f64_at(&header, 179), f64_at(&header, 195), f64_at(&header, 211)];
    validate_bounds(bounds_min, bounds_max)?;
    let mut projection = String::new();
    let mut epsg = None;
    let count = u32_at(&header, 100) as u64;
    if count > 100_000 { return Err("Het LAS-bestand bevat te veel metadatarecords".into()); }
    read_crs_records(&mut file, header_size, count, false, data_offset, &mut projection, &mut epsg)?;
    if header[25] == 4 {
        let offset = u64_at(&header, 235);
        let count = u32_at(&header, 243) as u64;
        if count > 100_000 { return Err("Het LAS-bestand bevat te veel uitgebreide metadatarecords".into()); }
        if count > 0 {
            if offset < data_offset || offset >= metadata.len() { return Err("De uitgebreide LAS-metadataoffset is ongeldig".into()); }
            read_crs_records(&mut file, offset, count, true, metadata.len(), &mut projection, &mut epsg)?;
        }
    }
    if let Some(wkt_epsg) = wkt_horizontal_epsg(&projection) {
        if epsg.is_some_and(|geokey| geokey != wkt_epsg) {
            return Err("De coördinatenstelsels in de LAS-metadata spreken elkaar tegen".into());
        }
        epsg = Some(wkt_epsg);
    }
    if projection.is_empty() { if let Some(code) = epsg { projection = format!("EPSG:{code}"); } }
    let mut sample = vec![0_u8; metadata.len().min(65_536) as usize];
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    read_exact_at(&mut file, 0, &mut sample)?;
    digest.update(&sample);
    if metadata.len() > sample.len() as u64 {
        read_exact_at(&mut file, metadata.len() - sample.len() as u64, &mut sample)?;
        digest.update(&sample);
    }
    Ok(InspectedLas { path, size: metadata.len(), modified: modified_time(&metadata)?, point_count,
        bounds_min, bounds_max, projection, epsg, sample_fingerprint: hex(digest.finish().as_ref()) })
}

fn read_crs_records(file: &mut File, mut offset: u64, count: u64, extended: bool, limit: u64,
    projection: &mut String, epsg: &mut Option<u32>) -> Result<(), String> {
    let header_size = if extended { 60 } else { 54 };
    let mut read_bytes = 0_u64;
    for _ in 0..count {
        if offset.checked_add(header_size).is_none_or(|end| end > limit) {
            return Err("Een LAS-metadatarecord valt buiten het bestand".into());
        }
        let mut header = [0_u8; 60];
        read_exact_at(file, offset, &mut header[..header_size as usize])?;
        let length = if extended { u64_at(&header, 20) } else { u16_at(&header, 20) as u64 };
        let next = offset.checked_add(header_size).and_then(|start| start.checked_add(length))
            .filter(|end| *end <= limit).ok_or("Een LAS-metadatarecord is afgebroken")?;
        let user_id = String::from_utf8_lossy(&header[2..18]).trim_end_matches('\0').trim().to_owned();
        let record_id = u16_at(&header, 18);
        if user_id == "LASF_Projection" && matches!(record_id, 2112 | 34735) {
            read_bytes = read_bytes.saturating_add(length);
            if length > MAX_PROJECTION_BYTES as u64 || read_bytes > 4 * MAX_PROJECTION_BYTES as u64 {
                return Err("De LAS-coördinateninformatie is te groot".into());
            }
            let mut data = vec![0_u8; length as usize];
            read_exact_at(file, offset + header_size, &mut data)?;
            if record_id == 2112 {
                let text = String::from_utf8(data).map_err(|_| "De LAS-projectietekst is geen geldige UTF-8")?;
                *projection = text.trim_matches('\0').trim().to_owned();
            } else if data.len() >= 8 && data.len() % 2 == 0 {
                let keys = u16_at(&data, 6) as usize;
                if keys > (data.len() - 8) / 8 { return Err("De LAS-GeoKey-informatie is afgebroken".into()); }
                let mut projected = None;
                let mut geographic = None;
                let mut custom_projected = false;
                for index in 0..keys {
                    let at = 8 + index * 8;
                    if u16_at(&data, at + 2) != 0 || u16_at(&data, at + 4) != 1 { continue; }
                    let code = u16_at(&data, at + 6) as u32;
                    match u16_at(&data, at) {
                        3072 if code == 32767 => custom_projected = true,
                        3072 if code != 0 => projected = Some(code),
                        2048 if code != 0 && code != 32767 => geographic = Some(code),
                        _ => {}
                    }
                }
                if let Some(code) = projected.or_else(|| (!custom_projected).then_some(geographic).flatten()) {
                    *epsg = Some(code);
                }
            }
        }
        offset = next;
    }
    Ok(())
}

fn projection_epsg(projection: &str) -> Option<u32> {
    projection.trim().strip_prefix("EPSG:").and_then(|code| code.parse().ok())
        .or_else(|| wkt_horizontal_epsg(projection))
}

/// Use only the horizontal CRS's own authority, never a nested datum or vertical
/// CRS. This recognizes both WKT1 AUTHORITY and WKT2 ID without guessing by name.
fn wkt_horizontal_epsg(wkt: &str) -> Option<u32> {
    let projected = regex::Regex::new(r"(?i)\b(?:PROJCS|PROJCRS)\s*\[").ok()?;
    let geographic = regex::Regex::new(r"(?i)\b(?:GEOGCS|GEOGCRS|GEODCRS|GEOGRAPHICCRS)\s*\[").ok()?;
    let project = projected.find(wkt).or_else(|| geographic.find(wkt))?;
    let suffix = &wkt[project.end()..];
    let authority = regex::Regex::new(r#"(?i)(?:AUTHORITY|ID)\s*\[\s*"EPSG"\s*,\s*"?([0-9]+)"?\s*\]"#).ok()?;
    let matches = authority.captures_iter(suffix).filter_map(|capture|
        Some((capture.get(0)?.start(), capture.get(1)?.as_str().parse::<u32>().ok()?)))
        .collect::<Vec<_>>();
    let mut depth = 1_i32;
    let mut quoted = false;
    let mut next = 0;
    for (index, ch) in suffix.char_indices() {
        if matches.get(next).is_some_and(|hit| hit.0 == index) {
            let code = matches[next].1;
            next += 1;
            if depth == 1 && !quoted { return Some(code); }
        }
        if ch == '"' { quoted = !quoted; }
        if !quoted {
            if ch == '[' { depth += 1; }
            if ch == ']' { depth -= 1; if depth == 0 { break; } }
        }
    }
    None
}

fn fingerprint_file(path: &Path, cancel: &AtomicBool, mut progress: impl FnMut(String)) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let size = file.metadata().map_err(|error| error.to_string())?.len();
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut read = 0_u64;
    let mut previous = SystemTime::now();
    loop {
        check_cancel(cancel)?;
        let count = file.read(&mut buffer).map_err(|error| format!("Bronbestand controleren mislukt: {error}"))?;
        if count == 0 { break; }
        context.update(&buffer[..count]);
        read = read.saturating_add(count as u64);
        if previous.elapsed().unwrap_or_default() >= Duration::from_secs(1) {
            progress(format!("Bronbestand controleren: {:.0}%", 100. * read as f64 / size.max(1) as f64));
            previous = SystemTime::now();
        }
    }
    if read != size { return Err("Het bronbestand veranderde tijdens het controleren".into()); }
    Ok(hex(context.finish().as_ref()))
}

#[derive(Deserialize)]
struct ConversionMetadata {
    version: String,
    points: u64,
    #[serde(default)]
    projection: String,
    #[serde(rename = "boundingBox")]
    bounds: ConversionBounds,
    scale: [f64; 3],
    offset: [f64; 3],
    hierarchy: ConversionHierarchy,
    encoding: String,
}
#[derive(Deserialize)]
struct ConversionBounds { min: [f64; 3], max: [f64; 3] }
#[derive(Deserialize)]
struct ConversionHierarchy { #[serde(rename = "firstChunkSize")] first_chunk_size: u64 }

fn read_conversion(cloud: &Path) -> Result<ConversionMetadata, String> {
    let file = File::open(cloud.join("metadata.json"))
        .map_err(|_| "De lokale puntenwolk ontbreekt. Bouw de puntenwolk opnieuw op.".to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > MAX_METADATA_BYTES {
        return Err("De puntenwolkmetadata is te groot".into());
    }
    let metadata: ConversionMetadata = serde_json::from_reader(file)
        .map_err(|error| format!("De puntenwolkmetadata is ongeldig: {error}"))?;
    if metadata.version != "2.0" || metadata.points == 0 || metadata.encoding != "DEFAULT"
        || metadata.scale.iter().any(|scale| !scale.is_finite() || *scale <= 0.)
        || metadata.offset.iter().any(|value| !value.is_finite())
        || metadata.projection.len() > MAX_PROJECTION_BYTES {
        return Err("De geconverteerde puntenwolk heeft een niet-ondersteunde indeling".into());
    }
    validate_bounds(metadata.bounds.min, metadata.bounds.max)?;
    for name in ["octree.bin", "hierarchy.bin"] {
        let info = fs::symlink_metadata(cloud.join(name)).map_err(|_| format!("De puntenwolk is onvolledig: {name} ontbreekt"))?;
        if !info.is_file() || info.len() == 0 { return Err(format!("De puntenwolk is onvolledig: {name} is leeg of ongeldig")); }
        if name == "hierarchy.bin" && (metadata.hierarchy.first_chunk_size < 22
            || metadata.hierarchy.first_chunk_size % 22 != 0 || metadata.hierarchy.first_chunk_size > info.len()) {
            return Err("De puntenwolkstructuur is onvolledig".into());
        }
    }
    Ok(metadata)
}

fn validate_conversion_matches(metadata: &ConversionMetadata, count: u64, min: [f64; 3], max: [f64; 3]) -> Result<(), String> {
    if metadata.points != count { return Err("Het aantal geconverteerde punten komt niet overeen met de bron".into()); }
    for index in 0..3 {
        let tolerance = (metadata.scale[index] * 2.).max(1e-6);
        if metadata.bounds.min[index] > min[index] + tolerance
            || metadata.bounds.max[index] < max[index] - tolerance {
            return Err("De geconverteerde puntenwolk heeft andere coördinaatgrenzen dan de bron".into());
        }
    }
    Ok(())
}

#[cfg(windows)]
type ConverterChild = crate::windows_job::Process;
#[cfg(not(windows))]
type ConverterChild = std::process::Child;

struct RunningConverter {
    child: ConverterChild,
    _log: tempfile::NamedTempFile,
    reader: File,
    line: Vec<u8>,
}
impl RunningConverter {
    fn spawn(command: &mut Command) -> Result<Self, String> {
        let log = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
        let reader = File::open(log.path()).map_err(|error| error.to_string())?;
        #[cfg(windows)]
        let child = ConverterChild::spawn(command, log.as_file());
        #[cfg(not(windows))]
        let child = {
            // Duplicates share one output offset; the reader has its own handle.
            let out = log.as_file().try_clone().map_err(|error| error.to_string())?;
            let err = log.as_file().try_clone().map_err(|error| error.to_string())?;
            command.stdout(out).stderr(err).spawn()
        };
        let child = child.map_err(|error| format!("De lokale puntenwolkverwerking kon niet starten: {error}"))?;
        Ok(Self { child, _log: log, reader, line: Vec::with_capacity(4096) })
    }
    fn read_output(&mut self) -> Result<Vec<String>, String> {
        // Bound disk output as well as buffers. Converter diagnostics are not
        // point data, and should never occupy an unbounded amount of disk.
        if self.reader.metadata().map_err(|error| error.to_string())?.len() > 64 * 1024 * 1024 {
            return Err("De puntenwolkverwerking produceert te veel foutmeldingen en is gestopt.".into());
        }
        let mut lines = Vec::new();
        let mut buffer = [0_u8; 4096];
        for _ in 0..16 {
            let count = self.reader.read(&mut buffer).map_err(|error| error.to_string())?;
            if count == 0 { break; }
            for byte in &buffer[..count] {
                if matches!(*byte, b'\n' | b'\r') {
                    if !self.line.is_empty() {
                        let text: String = String::from_utf8_lossy(&self.line).chars()
                            .filter(|ch| !ch.is_control()).collect();
                        if !text.trim().is_empty() { lines.push(text.trim().to_owned()); }
                        self.line.clear();
                    }
                } else if self.line.len() < 4096 { self.line.push(*byte); }
            }
        }
        Ok(lines)
    }
}
impl Drop for RunningConverter {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        // On Windows the job's own drop also terminates any descendants, even
        // when the converter itself has already exited. OS handle closure
        // enforces the same rule if the main application crashes.
    }
}

fn conversion_progress(line: &str) -> Option<String> {
    static PERCENT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(||
        regex::Regex::new(r"([0-9]{1,3}(?:\.[0-9]+)?)\s*%").unwrap());
    if let Some(capture) = PERCENT.captures(line) {
        if let Ok(percent) = capture[1].parse::<f64>() {
            if (0. ..=100.).contains(&percent) {
                return Some(format!("Puntenwolk voorbereiden: {percent:.0}%"));
            }
        }
    }
    let line = line.to_ascii_lowercase();
    if line.contains("indexing") || line.contains("sampling") {
        Some("3D-weergave opbouwen…".into())
    } else if line.contains("chunking") || line.contains("reading") {
        Some("Scanpunten inlezen…".into())
    } else if line.contains("hierarchy") || line.contains("metadata") {
        Some("Puntenwolk afronden…".into())
    } else { None }
}

fn convert(path: &Path, output: &Path, cancel: &AtomicBool, progress: &mut impl FnMut(String)) -> Result<(), String> {
    let runtime = runtime_directory()?;
    let mut command = Command::new(runtime.join("converter/PotreeConverter.exe"));
    command.arg(path).arg("-o").arg(output).arg("--encoding").arg("DEFAULT")
        .current_dir(runtime.join("converter")).stdin(Stdio::null());
    check_cancel(cancel)?;
    let mut child = RunningConverter::spawn(&mut command)?;
    let mut recent = VecDeque::new();
    let mut previous = std::time::Instant::now();
    let mut pending_progress = None;
    progress("Puntenwolk lokaal voorbereiden…".into());
    loop {
        check_cancel(cancel)?;
        for line in child.read_output()? {
            if recent.len() == 8 { recent.pop_front(); }
            pending_progress = conversion_progress(&line).or(pending_progress);
            recent.push_back(line);
        }
        if previous.elapsed() >= Duration::from_millis(250) {
            if let Some(message) = pending_progress.take() { progress(message); }
            previous = std::time::Instant::now();
        }
        if let Some(status) = child.child.try_wait().map_err(|error| error.to_string())? {
            if !status.success() {
                for line in child.read_output()? {
                    if recent.len() == 8 { recent.pop_front(); }
                    recent.push_back(line);
                }
                let details = recent.into_iter().rev().take(3).collect::<Vec<_>>().into_iter().rev()
                    .map(|line| line.chars().take(512).collect::<String>()).collect::<Vec<_>>().join("\n");
                return Err(format!("Het LAS/LAZ-bestand kon niet worden verwerkt ({status}).\n{details}"));
            }
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub fn import(path: &Path, title: impl AsRef<str>, crs: CloudCrs, cancel: &AtomicBool,
    mut progress: impl FnMut(String)) -> Result<PointCloudSource, String> {
    let title = title.as_ref().trim();
    if !valid_text(title, 1024, false) { return Err("Vul een geldige naam voor de puntenwolk in".into()); }
    import_inner(path, title, crs, None, cancel, &mut progress)
}

/// Rebuild a lost/corrupt cache without changing source IDs or annotations.
pub fn import_existing(source: &PointCloudSource, cancel: &AtomicBool,
    mut progress: impl FnMut(String)) -> Result<PointCloudSource, String> {
    source.validate()?;
    if !valid_fingerprint(&source.source_fingerprint) {
        return Err("De opgeslagen bronidentiteit ontbreekt. Importeer het bestand als nieuwe puntenwolk.".into());
    }
    import_inner(&source.source_path, &source.title, source.crs, Some(source), cancel, &mut progress)
}

fn import_inner(path: &Path, title: &str, crs: CloudCrs, previous: Option<&PointCloudSource>,
    cancel: &AtomicBool, progress: &mut impl FnMut(String)) -> Result<PointCloudSource, String> {
    check_cancel(cancel)?;
    let inspected = inspect_las(path)?;
    if crs == CloudCrs::RdNew && inspected.epsg.is_some_and(|code| code != 28992) {
        return Err(format!("Het bestand noemt EPSG:{}, geen RD (EPSG:28992). Open het in het oorspronkelijke coördinatenstelsel; de bron wordt niet automatisch getransformeerd.", inspected.epsg.unwrap()));
    }
    let fingerprint = fingerprint_file(&inspected.path, cancel, &mut *progress)?;
    if previous.is_some_and(|source| source.source_fingerprint != fingerprint || source.source_size != inspected.size) {
        return Err("Het bronbestand is gewijzigd. Importeer het als nieuwe puntenwolk; de bestaande punten en lijnen blijven behouden.".into());
    }
    let id = previous.map(|source| source.id.clone()).map(Ok).unwrap_or_else(new_id)?;
    let cache_root = crate::project::app_data_directory().join("pointclouds");
    fs::create_dir_all(&cache_root).map_err(|error| format!("Puntenwolkmap aanmaken mislukt: {error}"))?;
    let staging = tempfile::Builder::new().prefix(".pointcloud-import-").tempdir_in(&cache_root)
        .map_err(|error| format!("Tijdelijke puntenwolkmap aanmaken mislukt: {error}"))?;
    let output = staging.path().join("cloud");
    convert(&inspected.path, &output, cancel, progress)?;
    check_cancel(cancel)?;
    let metadata = read_conversion(&output)?;
    validate_conversion_matches(&metadata, inspected.point_count, inspected.bounds_min, inspected.bounds_max)?;
    let after = inspect_las(&inspected.path)?;
    if !inspected.same_file_state(&after) {
        return Err("Het LAS/LAZ-bestand veranderde tijdens de verwerking. Probeer opnieuw.".into());
    }
    let crs = match (crs, inspected.epsg) {
        (CloudCrs::Unknown, Some(28992)) => CloudCrs::RdNew,
        (CloudCrs::Unknown, Some(_)) => CloudCrs::Other,
        (value, _) => value,
    };
    let source = PointCloudSource {
        id: id.clone(), title: title.into(), source_path: inspected.path.clone(), source_size: inspected.size,
        source_fingerprint: fingerprint.clone(), point_count: inspected.point_count,
        bounds_min: inspected.bounds_min, bounds_max: inspected.bounds_max,
        projection: if inspected.projection.is_empty() { metadata.projection } else { inspected.projection.clone() },
        crs, annotations: previous.map_or_else(Vec::new, |source| source.annotations.clone()),
    };
    source.validate()?;
    write_stamp(&output, &SourceStamp::from_inspected(&after, fingerprint))?;
    let destination = cache_root.join(&id);
    let backup = cache_root.join(format!(".replaced-{id}-{}", new_id()?));
    check_cancel(cancel)?;
    let had_previous = destination.exists();
    if had_previous && previous.is_none() { return Err("Puntenwolkidentiteit bestaat al. Importeer opnieuw.".into()); }
    if had_previous {
        fs::rename(&destination, &backup).map_err(|error| format!("Sluit de puntenwolkeditor voordat je opnieuw opbouwt: {error}"))?;
    }
    // Staging and destination share the same volume. On failure the previous
    // cache is restored; cancellation never replaces a usable conversion.
    if let Err(error) = fs::rename(staging.path(), &destination) {
        if had_previous { let _ = fs::rename(&backup, &destination); }
        return Err(format!("Puntenwolk bewaren mislukt: {error}"));
    }
    if had_previous { let _ = fs::remove_dir_all(backup); }
    progress(format!("Puntenwolk gereed: {} punten", source.point_count));
    Ok(source)
}
