//! Original-template downloads and durable snapshots of compatible custom DXFs.
//! Validation may prepare an in-memory copy, but the stored source stays exact.
use crate::{dxf_document::Document, input, template_setup};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const ORIGINAL: &[u8] = include_bytes!("../../assets/cadastral_template.dxf");
const SLOTS_PER_PAGE: usize = 8;

/// Save the immutable bundled original, independent of the selected preference.
/// The save dialog owns the overwrite choice; publication itself is atomic.
pub fn export_original(destination: &Path) -> Result<(), String> {
    if destination.file_name().is_none() {
        return Err("Kies een bestandsnaam voor het originele DXF-sjabloon.".into());
    }
    crate::storage::atomic_write(destination, true, |file| {
        file.write_all(ORIGINAL).map_err(|e| e.to_string())
    })
    .map_err(|e| format!("Origineel DXF-sjabloon opslaan mislukt: {e}"))
}

fn validate(document: &Document) -> Result<(), String> {
    let cancel = &input::NEVER_CANCEL;
    let slots = crate::template_slots::detect(document, cancel)?;
    if slots.len() < SLOTS_PER_PAGE {
        return Err("Het eerste blad moet acht herkenbare proefsleufvakken bevatten.".into());
    }
    // Exercise the same structural preparation as an export, without assets,
    // network access, drawing output, or any mutation of the imported source.
    let mut prepared = document.indexed_copy(cancel)?;
    let bounds = slots.iter().take(SLOTS_PER_PAGE)
        .map(|slot| Some(slot.row_box)).collect::<Vec<_>>();
    let result = template_setup::prepare(
        &mut prepared,
        &bounds,
        &template_setup::Options {
            project: None,
            drawn_by: String::new(),
            export_date: Some(crate::template_metadata::Date::new(2000, 1, 1)?),
            label_gap: 18.,
            profile_layers: Some(Vec::new()),
        },
        cancel,
    )?;
    // The export helper deliberately tolerates missing layouts for older
    // prepared inputs. A newly installed template needs its first output sheet.
    if result.pages.first().is_none_or(|page| page.scale.is_none()) {
        return Err("Blad1 mist een bruikbaar KADER-overzichtsvenster of tekenstempel.".into());
    }
    crate::profile_drawing::Templates::read(&prepared, cancel)?;
    Ok(())
}

/// Validate once and keep an independent, permanent copy in the app-data folder.
/// Every successful import gets a unique directory and keeps its original name;
/// changing preferences never overwrites files used by exports or open drafts.
/// The caller commits the returned path through cadastral_template_path.
pub fn import_custom(source: &Path) -> Result<PathBuf, String> {
    let name = source.file_name()
        .filter(|name| name.to_str().is_some())
        .ok_or("Kies een DXF-bestand met een geldige bestandsnaam.")?;
    let document = Document::load(source, &input::NEVER_CANCEL)
        .map_err(|e| format!("Aangepast DXF-sjabloon lezen mislukt: {e}"))?;
    validate(&document).map_err(|e| format!(
        "Dit DXF-bestand is niet geschikt als SleufBase-sjabloon: {e} \
         Gebruik het originele sjabloon als basis en behoud de bladen, proefsleufvakken en vaste bloknamen."
    ))?;

    let directory = crate::project::app_data_directory().join("templates");
    fs::create_dir_all(&directory)
        .map_err(|e| format!("Map voor aangepaste DXF-sjablonen maken mislukt: {e}"))?;
    let directory = fs::canonicalize(directory)
        .map_err(|e| format!("Map voor aangepaste DXF-sjablonen openen mislukt: {e}"))?;
    let snapshot = tempfile::Builder::new()
        .prefix("sjabloon-")
        .tempdir_in(directory)
        .map_err(|e| format!("Nieuwe sjabloonmap maken mislukt: {e}"))?;
    let destination = snapshot.path().join(name);
    let path = destination.to_str()
        .ok_or("Het pad voor het aangepaste DXF-sjabloon kan niet in de instellingen worden opgeslagen.")?;
    if path.len() > 32_768 {
        return Err("Het pad voor het aangepaste DXF-sjabloon is te lang.".into());
    }
    let mut file = OpenOptions::new().write(true).create_new(true)
        .open(&destination)
        .map_err(|e| format!("Aangepast DXF-sjabloon opslaan mislukt: {e}"))?;
    // No pending edits exist on this document. write_to retains every source
    // byte, including line endings, handles, proxy data and unknown sections.
    document.write_to(&mut file, &input::NEVER_CANCEL)
        .map_err(|e| format!("Aangepast DXF-sjabloon kopiëren mislukt: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("Aangepast DXF-sjabloon bewaren mislukt: {e}"))?;
    drop(file);
    // Before this point a failure drops the private directory and its file.
    // Old snapshots intentionally survive preference changes and restoration.
    let _permanent_directory = snapshot.keep();
    Ok(destination)
}
