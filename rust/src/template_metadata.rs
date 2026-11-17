//! Per-export title values. Attribute values are scoped to selected layouts;
//! the client caption follows their reachable title-block definitions without
//! changing the project-number value or unrelated labels and revision rows.
use crate::{
    dxf_document::{Change, Document, RecordView},
    python_values::{trim, uppercase},
};
use std::{collections::{HashMap, HashSet}, fmt, sync::atomic::AtomicBool};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    year: u16,
    month: u16,
    day: u16,
}
impl Date {
    pub fn new(year: u16, month: u16, day: u16) -> Result<Self, String> {
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days = match month {
            4 | 6 | 9 | 11 => 30,
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            _ => 0,
        };
        if year == 0 || year > 9999 || day == 0 || day > days {
            return Err("Ongeldige exportdatum".into());
        }
        Ok(Self { year, month, day })
    }
    /// Windows' local calendar, including its configured timezone and DST.
    /// Capture once per export rather than once per sheet.
    pub fn today() -> Result<Self, String> {
        #[cfg(windows)]
        {
            let mut time =
                std::mem::MaybeUninit::<windows_sys::Win32::Foundation::SYSTEMTIME>::uninit();
            // GetLocalTime initializes every SYSTEMTIME field at this valid pointer.
            let time = unsafe {
                windows_sys::Win32::System::SystemInformation::GetLocalTime(time.as_mut_ptr());
                time.assume_init()
            };
            Self::new(time.wYear, time.wMonth, time.wDay)
        }
        #[cfg(not(windows))]
        {
            Err("Automatische lokale exportdatum is beschikbaar in de Windows-preview".into())
        }
    }
}
impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}-{:02}-{:04}", self.day, self.month, self.year)
    }
}
/// Editable values in the drawing stamp. Missing persisted fields keep the
/// defaults used by new exports, including projects saved before these fields.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TitleFields {
    pub checked_by: String,
    pub status: String,
    pub version: String,
    pub reference_drawings: String,
    pub project_number: String,
    /// None keeps an older/custom stamp caption; Some("") uses a neutral label.
    #[serde(skip_serializing_if="Option::is_none")]
    pub project_client: Option<String>,
    pub drawing_number: String,
}
impl Default for TitleFields {
    fn default() -> Self {
        Self {
            checked_by: String::new(),
            status: String::new(),
            version: "1.0".into(),
            reference_drawings: "Proefsleuven".into(),
            project_number: String::new(),
            project_client: None,
            drawing_number: String::new(),
        }
    }
}
impl TitleFields {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(code) = &self.project_client { crate::client_logos::validate_code(code)?; }
        for (name, value) in [
            ("Gezien", &self.checked_by),
            ("Status", &self.status),
            ("Versie", &self.version),
            ("Referentie Tekeningen", &self.reference_drawings),
            ("Projectnummer", &self.project_number),
            ("TEK.NR.", &self.drawing_number),
        ] {
            text(value).map_err(|error| format!("{name}: {error}"))?;
        }
        Ok(())
    }
}
pub struct Values {
    pub date: Date,
    /// None preserves an existing project name; Some("") deliberately clears it.
    pub project: Option<String>,
    pub drawn_by: String,
}
fn owner(r: RecordView<'_>) -> Option<u64> {
    let mut depth = 0;
    for (code, value) in r.tags() {
        if code == 100 {
            break;
        }
        if code == 102 {
            if value.starts_with('{') {
                depth += 1;
            } else if value == "}" {
                depth -= 1;
            }
        }
        if code == 330 && depth == 0 {
            return u64::from_str_radix(value.trim(), 16).ok();
        }
    }
    None
}
fn text(value: &str) -> Result<&str, String> {
    let value = trim(value);
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err("Gebruik titelbloktekst op één regel van maximaal 4096 UTF-8-bytes".into());
    }
    Ok(value)
}
fn stage(changes: &mut Vec<Change>, bytes: &mut usize, change: Change) -> Result<(), String> {
    *bytes += change.text.len() + std::mem::size_of::<Change>();
    if *bytes > 32 * 1024 * 1024 {
        return Err("Te veel titelblokwijzigingen in één export".into());
    }
    changes.push(change);
    Ok(())
}
fn scalar(record: RecordView<'_>, code: i32, default: f64) -> Result<f64, String> {
    let value = record.current_value(code)?.map_or(Ok(default), |value| value.trim().parse::<f64>()
        .map_err(|_| "Ongeldige titelbloktekstmaat".to_owned()))?;
    if value.is_finite() { Ok(value) } else { Err("Ongeldige titelbloktekstmaat".into()) }
}
/// Follow the used paper layouts into nested title blocks and the original
/// dynamic master. Updating both visible and hidden caption variants prevents
/// the previous client name from returning after AutoCAD regenerates a block.
fn client_caption_changes(
    document: &Document, spaces: &HashSet<u64>, client: &str,
    changes: &mut Vec<Change>, bytes: &mut usize, cancel: &AtomicBool,
) -> Result<(), String> {
    let mut blocks = HashMap::new();
    let mut edges = HashMap::<u64, Vec<u64>>::new();
    for record in document.records() {
        if record.index().is_multiple_of(1024) { crate::input::check(cancel)?; }
        if record.kind() != "BLOCK_RECORD" { continue; }
        let Some(id) = record.handle() else { continue; };
        if let Some(name) = record.first(2) { blocks.insert(uppercase(name), id); }
        let mut dynamic_master = false;
        for (code, value) in record.tags() {
            if code == 1001 { dynamic_master = value.eq_ignore_ascii_case("AcDbBlockRepBTag"); }
            else if dynamic_master && code == 1005 {
                if let Ok(parent) = u64::from_str_radix(value.trim(), 16) {
                    edges.entry(id).or_default().push(parent);
                }
            }
        }
    }
    for record in document.records() {
        if record.index().is_multiple_of(1024) { crate::input::check(cancel)?; }
        if record.kind() != "INSERT" || !matches!(record.section(), Some("BLOCKS" | "ENTITIES")) { continue; }
        if let (Some(parent), Some(child)) = (owner(record), record.first(2)
            .and_then(|name| blocks.get(&uppercase(name))).copied()) {
            edges.entry(parent).or_default().push(child);
        }
    }
    let mut reachable = spaces.clone();
    let mut pending: Vec<u64> = spaces.iter().copied().collect();
    while let Some(parent) = pending.pop() {
        crate::input::check(cancel)?;
        for &child in edges.get(&parent).into_iter().flatten() {
            if reachable.insert(child) { pending.push(child); }
        }
    }
    // The built-in dynamic stamp already contains a correctly styled caption
    // for every standard client. Reuse that exact style/position reference so
    // selecting ENEXIS, BW etc. does not unnecessarily condense its lettering.
    let mut reference_widths = HashMap::<(u64, String), Vec<(f64, f64, f64, f64, String)>>::new();
    for record in document.records() {
        if record.index().is_multiple_of(1024) { crate::input::check(cancel)?; }
        if record.kind() != "TEXT" { continue; }
        let Some(parent) = owner(record).filter(|id| reachable.contains(id)) else { continue; };
        let Some(label) = record.current_value(1)? else { continue; };
        let label = trim(label);
        if label.len() > 256 || !label.get(..10).is_some_and(|prefix| prefix.eq_ignore_ascii_case("PROJECTNR.")) || !label.ends_with(':') { continue; }
        reference_widths.entry((parent, uppercase(label))).or_default().push((
            scalar(record, 10, 0.)?, scalar(record, 20, 0.)?, scalar(record, 40, 0.)?, scalar(record, 41, 1.)?,
            record.current_value(7)?.unwrap_or("").to_owned(),
        ));
    }
    // The fixed title cell's vertical border bounds even long custom captions.
    // Coordinates remain in the block's local system; INSERT scale is untouched.
    let mut borders = HashMap::<u64, Vec<(f64, f64, f64)>>::new();
    for record in document.records() {
        if record.index().is_multiple_of(1024) { crate::input::check(cancel)?; }
        if record.kind() != "LINE" { continue; }
        let Some(parent) = owner(record).filter(|id| reachable.contains(id)) else { continue; };
        let x1 = scalar(record, 10, 0.)?;
        let x2 = scalar(record, 11, 0.)?;
        let y1 = scalar(record, 20, 0.)?;
        let y2 = scalar(record, 21, 0.)?;
        if (x1 - x2).abs() < 1e-7 && (y1 - y2).abs() > 1e-7 {
            borders.entry(parent).or_default().push((x1, y1.min(y2), y1.max(y2)));
        }
    }
    for record in document.records() {
        if record.index().is_multiple_of(1024) { crate::input::check(cancel)?; }
        if record.kind() != "TEXT" || !matches!(record.section(), Some("BLOCKS" | "ENTITIES")) { continue; }
        let Some(parent) = owner(record).filter(|id| reachable.contains(id)) else { continue; };
        let Some(current) = record.current_value(1)? else { continue; };
        let current = trim(current);
        if current.len() > 256 || !current.get(..10).is_some_and(|prefix| prefix.eq_ignore_ascii_case("PROJECTNR.")) || !current.ends_with(':') { continue; }
        let old_code = current[10..current.len()-1].trim();
        if crate::client_logos::validate_code(old_code).is_err() { continue; }
        let replacement = if client.is_empty() { "PROJECTNR.:".to_owned() }
            else { format!("PROJECTNR. {client}:") };
        if current == replacement { continue; }
        let height = scalar(record, 40, 0.)?;
        let width = scalar(record, 41, 1.)?;
        if height <= 0. || width <= 0. { return Err("Ongeldige breedte of hoogte van het projectnummerlabel".into()); }
        let x = scalar(record, 10, 0.)?;
        let y = scalar(record, 20, 0.)?;
        let right = if scalar(record, 50, 0.)?.abs() < 1e-7 && scalar(record, 72, 0.)? == 0. {
            borders.get(&parent).into_iter().flatten()
                .filter(|(edge, bottom, top)| *edge > x + height && *bottom <= y && *top >= y + height)
                .map(|(edge, _, _)| *edge).min_by(f64::total_cmp)
        } else { None };
        // 1.5 cap-heights per glyph is deliberately conservative, including
        // wide capitals. Never enlarge an existing width factor. A custom
        // borderless/rotated stamp stays within a conservative prior footprint.
        let budget = right.map(|right| (right-x-height*0.75).max(height))
            .unwrap_or_else(|| current.chars().count() as f64 * height * width * 0.5);
        let reference = reference_widths.get(&(parent, uppercase(&replacement))).into_iter().flatten()
            .find(|(rx, ry, rh, rw, style)| (*rx-x).abs() < 1e-7 && (*ry-y).abs() < 1e-7
                && (*rh-height).abs() < 1e-7 && *rw > 0. && style.as_str() == record.first(7).unwrap_or(""))
            .map(|(_, _, _, rw, _)| *rw);
        let fitted = width.min(reference.unwrap_or_else(||
            budget / (replacement.chars().count().max(1) as f64 * height * 1.5)));
        if fitted < width {
            // TEXT commonly omits its default width factor. Insert it inside
            // AcDbText and publish caption + width as one record replacement.
            let mut tags = crate::template_copy::tags(record);
            crate::template_copy::scope_set(&mut tags, "AcDbText", 1, replacement)?;
            crate::template_copy::scope_set(&mut tags, "AcDbText", 41, fitted.to_string())?;
            stage(changes, bytes, document.record_change(record.index(), crate::template_copy::encode(&tags)))?;
        } else {
            stage(changes, bytes, document.tag_change(record.index(), 1, &replacement)?)?;
        }
    }
    Ok(())
}

pub fn update(
    document: &mut Document,
    layouts: &[String],
    values: &Values,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    update_impl(document, layouts, values, None, cancel)
}
/// Update editable stamp fields only in INSERT attributes of the used layouts.
/// This deliberately leaves the dynamic block visibility option named Versie
/// and all unused reference/revision rows unchanged.
pub fn update_with_title_fields(
    document: &mut Document,
    layouts: &[String],
    values: &Values,
    title_fields: &TitleFields,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    update_impl(document, layouts, values, Some(title_fields), cancel)
}
fn update_impl(
    document: &mut Document,
    layouts: &[String],
    values: &Values,
    title_fields: Option<&TitleFields>,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    crate::input::check(cancel)?;
    if let Some(fields) = title_fields {
        fields.validate()?;
    }
    let project = values.project.as_deref().map(text).transpose()?;
    let drawn_by = text(&values.drawn_by)?;
    let date = values.date.to_string();
    let names: HashSet<_> = layouts
        .iter()
        .map(|v| uppercase(v))
        .filter(|v| v != "MODEL")
        .collect();
    let mut ids = HashSet::new();
    for r in document.records() {
        if r.index().is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if r.kind() == "LAYOUT"
            && r.tags()
                .filter(|(c, _)| *c == 1)
                .last()
                .is_some_and(|(_, v)| names.contains(&uppercase(v)))
        {
            if let Some(id) = r.handle() {
                ids.insert(id);
            }
        }
    }
    let mut spaces = HashSet::new();
    for r in document.records() {
        if r.index().is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if r.kind() == "BLOCK_RECORD"
            && r.first(340)
                .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
                .is_some_and(|id| ids.contains(&id))
        {
            if let Some(id) = r.handle() {
                spaces.insert(id);
            }
        }
    }
    let mut changes = Vec::new();
    let mut bytes = 0;
    let mut records = document.records().peekable();
    while let Some(insert) = records.next() {
        if insert.index().is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if insert.kind() != "INSERT"
            || !owner(insert).is_some_and(|id| spaces.contains(&id))
            || !matches!(insert.section(), Some("BLOCKS" | "ENTITIES"))
        {
            continue;
        }
        let mut attributes = Vec::new();
        while records.peek().is_some_and(|r| r.kind() == "ATTRIB") {
            let r = records.next().unwrap();
            if r.index().is_multiple_of(1024) {
                crate::input::check(cancel)?;
            }
            let tag = trim(r.first(2).unwrap_or(""));
            // None of the recognized tags can exceed this length, even before
            // Unicode uppercase expansion. Avoid allocating for arbitrary tags.
            if tag.len() > 64 {
                continue;
            }
            if attributes.len() >= 10_000 {
                return Err("Te veel attributen in één titelblok".into());
            }
            attributes.push((r, uppercase(tag)));
        }
        for (numbered, plain, value) in [
            ("DATUM1", "DATUM", date.as_str()),
            ("OMSCHRIJVING1", "OMSCHRIJVING", "Proefsleuven"),
            ("TEKENAAR1", "TEKENAAR", drawn_by),
        ] {
            let numbered_exists = attributes.iter().any(|(_, tag)| tag == numbered);
            let wanted = if numbered_exists { numbered } else { plain };
            for (r, _) in attributes
                .iter()
                .filter(|(_, tag)| tag == wanted)
                .take(if numbered_exists { usize::MAX } else { 1 })
            {
                stage(
                    &mut changes,
                    &mut bytes,
                    document.tag_change(r.index(), 1, value)?,
                )?;
            }
        }
        if let Some(fields) = title_fields {
            // The built-in dynamic stamp uses row 1 and VERSIE.NR. Older BAM
            // stamps repeat unnumbered tags; update their first row only.
            for (numbered, plain, value) in [
                ("GEZIEN1", "GEZIEN", fields.checked_by.as_str()),
                ("STATUS1", "STATUS", fields.status.as_str()),
                ("VERSIE1", "VERSIE", fields.version.as_str()),
                ("REFTEK1", "REFTEK", fields.reference_drawings.as_str()),
            ] {
                let numbered_exists = attributes.iter().any(|(_, tag)| tag == numbered);
                let wanted = if numbered_exists { numbered } else { plain };
                for (r, _) in attributes
                    .iter()
                    .filter(|(_, tag)| tag == wanted)
                    .take(if numbered_exists { usize::MAX } else { 1 })
                {
                    stage(
                        &mut changes,
                        &mut bytes,
                        document.tag_change(r.index(), 1, trim(value))?,
                    )?;
                }
            }
            for (wanted, value) in [
                ("VERSIE.NR", fields.version.as_str()),
                ("PROJECT.NR", fields.project_number.as_str()),
                ("TEK.NR", fields.drawing_number.as_str()),
            ] {
                for (r, _) in attributes.iter().filter(|(_, tag)| tag == wanted) {
                    stage(
                        &mut changes,
                        &mut bytes,
                        document.tag_change(r.index(), 1, trim(value))?,
                    )?;
                }
            }
        }
        if let Some(project) = project {
            for (r, _) in attributes.iter().filter(|(_, tag)| {
                matches!(
                    tag.as_str(),
                    "GEM-PROJECT" | "PROJECTOMSCHRIJVING" | "PROJECTNAAM"
                )
            }) {
                stage(
                    &mut changes,
                    &mut bytes,
                    document.tag_change(r.index(), 1, project)?,
                )?;
            }
        }
    }
    crate::input::check(cancel)?;
    drop(records);
    if let Some(client) = title_fields.and_then(|fields| fields.project_client.as_deref()) {
        client_caption_changes(document, &spaces, client, &mut changes, &mut bytes, cancel)?;
    }
    let count = changes.len();
    document.apply(changes)?;
    Ok(count)
}
