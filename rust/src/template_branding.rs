//! Paper-layout logos and editable Techbase contact details. Logo pixels
//! are prepared once per source and shared by all used sheets; IMAGE ownership
//! and publication stay in the existing transactional image batch.
use crate::{
    dxf_document::Document, export_control::Control, template_copy as copy, template_images::Image,
    Bounds,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Logos {
    pub techbase: Option<PathBuf>,
    pub client: Option<PathBuf>,
}
impl Logos {
    pub fn is_empty(&self) -> bool {
        self.techbase.is_none() && self.client.is_none()
    }
}

const TECHBASE: Bounds = Bounds {
    min_x: -178.6673353950342,
    min_y: 1.223957895626142,
    max_x: -162.6727998461477,
    max_y: 17.66716785422732,
};
const CLIENT: Bounds = Bounds {
    min_x: -178.6673353950342,
    min_y: (34.0 + 53.96902090226447) / 2.0 - 12.40736127731883 / 2.0,
    max_x: -178.6673353950342 + 37.30787274584635,
    max_y: (34.0 + 53.96902090226447) / 2.0 + 12.40736127731883 / 2.0,
};

/// Editable contact lines in the Techbase drawing stamp.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContactDetails {
    pub street: String,
    pub postcode: String,
    pub city: String,
    pub phone: String,
    pub email: String,
    pub website: String,
}
impl Default for ContactDetails {
    fn default() -> Self {
        Self {
            street: "De Bloemendaal 10".into(),
            postcode: "5221 EC".into(),
            city: "'s-Hertogenbosch".into(),
            phone: "073 220 0341".into(),
            email: "contact@hdbtechbase.nl".into(),
            website: "www.techbasebv.nl".into(),
        }
    }
}
impl ContactDetails {
    pub fn is_default(&self) -> bool { self == &Self::default() }
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("Straat en huisnummer", &self.street), ("Postcode", &self.postcode),
            ("Plaats", &self.city), ("Telefoon", &self.phone),
            ("E-mail", &self.email), ("Website", &self.website),
        ] {
            if !contact_text(value) {
                return Err(format!("Techbase {name}: kort deze contactregel in en gebruik één regel."));
            }
        }
        Ok(())
    }
    /// Legacy settings are an allowlisted, tolerant import. Unknown fields are
    /// retained by the project archive; invalid individual values keep defaults.
    pub(crate) fn from_legacy(value: &serde_json::Value) -> Self {
        let mut out = Self::default();
        out.apply_legacy(value);
        out
    }
    pub(crate) fn apply_legacy(&mut self, value: &serde_json::Value) {
        for (key, target) in [
            ("street", &mut self.street), ("postcode", &mut self.postcode),
            ("city", &mut self.city), ("phone", &mut self.phone),
            ("email", &mut self.email), ("website", &mut self.website),
        ] {
            if let Some(value) = value.get(key).and_then(serde_json::Value::as_str)
                .filter(|value| contact_text(value))
            {
                *target = crate::python_values::trim(value).to_owned();
            }
        }
    }
}
fn contact_text(value: &str) -> bool {
    let value = crate::python_values::trim(value);
    value.len() <= 4096 && !value.chars().any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
}
fn contact_field(value: &str) -> Option<usize> {
    match value {
        "De Diepteweg 16" | "De Bloemendaal 10" => Some(0),
        "5236 PV" | "5221 EC" => Some(1),
        "'s-Hertogenbosch" => Some(2),
        "073 - 220 0341" | "073 220 0341" | "0732200341" => Some(3),
        "contact@hdbtechbase.nl" => Some(4),
        "www.hdbtechbase.nl" | "hdbtechbase.nl"
            | "http://www.hdbtechbase.nl" | "https://www.hdbtechbase.nl"
            | "http://hdbtechbase.nl" | "https://hdbtechbase.nl"
            | "www.techbasebv.nl" | "techbasebv.nl"
            | "http://www.techbasebv.nl" | "https://www.techbasebv.nl"
            | "http://techbasebv.nl" | "https://techbasebv.nl" => Some(5),
        _ => None,
    }
}
/// Keep the existing default-repair API for standalone template tooling.
pub fn restore_contact(document: &mut Document, cancel: &AtomicBool) -> Result<usize, String> {
    update_contact_impl(document, &ContactDetails::default(), cancel, false)
}
/// Fill complete recognized Techbase contact sets, including anonymous dynamic
/// variants and cloned sheets. Unrelated text and custom source contacts remain
/// intact. Empty fields deliberately leave that contact line visually blank.
pub fn update_contact(document: &mut Document, contact: &ContactDetails, cancel: &AtomicBool) -> Result<usize, String> {
    update_contact_impl(document, contact, cancel, true)
}
fn update_contact_impl(document: &mut Document, contact: &ContactDetails, cancel: &AtomicBool, complete_sets: bool) -> Result<usize, String> {
    crate::input::check(cancel)?;
    contact.validate()?;
    let mut owners = HashMap::<u64, u8>::new();
    if complete_sets {
        for r in document.records() {
            if r.index().is_multiple_of(1024) { crate::input::check(cancel)?; }
            if r.kind() != "TEXT" || r.section() != Some("BLOCKS") { continue; }
            if let (Some(owner), Some(field)) = (copy::owner(r),
                r.current_value(1)?.as_deref().and_then(contact_field))
            {
                *owners.entry(owner).or_default() |= 1 << field;
            }
        }
    }
    let values = [&contact.street, &contact.postcode, &contact.city,
        &contact.phone, &contact.email, &contact.website];
    let mut changes = Vec::new();
    let mut bytes = 0usize;
    for r in document.records() {
        if r.index().is_multiple_of(1024) { crate::input::check(cancel)?; }
        if r.kind() != "TEXT" || (complete_sets && (r.section() != Some("BLOCKS")
            || !copy::owner(r).is_some_and(|owner| owners.get(&owner) == Some(&0b11_1111))))
        { continue; }
        let current = r.current_value(1)?;
        if let Some(field) = current.as_deref().and_then(contact_field) {
            let value = crate::python_values::trim(values[field]);
            if current.as_deref() == Some(value) { continue; }
            let change = document.tag_change(r.index(), 1, value)?;
            bytes = bytes.saturating_add(change.text.len() + std::mem::size_of_val(&change));
            if bytes > 32 * 1024 * 1024 { return Err("Te veel contactwijzigingen in één export".into()); }
            changes.push(change);
        }
    }
    let count = changes.len();
    crate::input::check(cancel)?;
    document.apply(changes)?;
    Ok(count)
}

/// Normalize a selected logo without the photo-strip resizing rule. Invalid
/// logos are reported before publication instead of silently copying corrupt
/// bytes with a .png extension (an original fallback bug).
pub fn prepare_logo(source: &Path, destination: &Path, control: &Control) -> Result<u64, String> {
    crate::template_photo::prepare_logo(source, destination, control)
}

fn number(t: &copy::Tags, code: i32, default: f64) -> Result<f64, String> {
    let n = copy::first(t, code).map_or(Ok(default), |s| {
        s.trim()
            .parse::<f64>()
            .map_err(|_| "Ongeldige logosjablooncoördinaat")
    })?;
    if !n.is_finite() || n.abs() > 1e12 {
        return Err("Ongeldige logosjablooncoördinaat".into());
    }
    Ok(n)
}
/// Plan both logos for each used existing paper layout's first INSERT, exactly
/// as the original exporter does. Source paths must already be normalized PNGs
/// in the transaction bundle. Rotation is intentionally not applied: the
/// original title-block contract uses only insertion and x/y scale.
pub fn images(
    document: &Document,
    layouts: &[String],
    techbase: Option<&Path>,
    client: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<Vec<Image>, String> {
    crate::input::check(cancel)?;
    if layouts.len() > 12_500 {
        return Err("Te veel logobladen".into());
    }
    if techbase.is_none() && client.is_none() {
        return Ok(Vec::new());
    }
    let mut names = HashMap::new();
    for r in document.records().filter(|r| r.kind() == "LAYOUT") {
        if let (Some(name), Some(handle)) = (
            r.tags().filter(|(c, _)| *c == 1).last().map(|(_, v)| v),
            r.handle(),
        ) {
            names.insert(crate::python_values::uppercase(name), handle);
        }
    }
    let mut spaces = HashMap::new();
    for r in document.records().filter(|r| r.kind() == "BLOCK_RECORD") {
        if let (Some(layout), Some(handle)) =
            (r.first(340).and_then(|s| copy::id(s).ok()), r.handle())
        {
            spaces.insert(layout, handle);
        }
    }
    let mut first_inserts = HashMap::new();
    for r in document.records() {
        if r.index().is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if r.kind() == "INSERT" && matches!(r.section(), Some("ENTITIES" | "BLOCKS")) {
            if let Some(owner) = copy::owner(r) {
                first_inserts.entry(owner).or_insert(r);
            }
        }
    }
    let mut result = Vec::new();
    for name in layouts {
        crate::input::check(cancel)?;
        if crate::python_values::uppercase(name) == "MODEL" {
            continue;
        }
        let Some(insert) = names
            .get(&crate::python_values::uppercase(name))
            .and_then(|h| spaces.get(h))
            .and_then(|h| first_inserts.get(h))
        else {
            continue;
        };
        let t = copy::tags(*insert);
        let x = number(&t, 10, 0.)?;
        let y = number(&t, 20, 0.)?;
        // Python's `getattr(..., 1.0) or 1.0` also substitutes a zero scale.
        let sx = number(&t, 41, 1.)?;
        let sy = number(&t, 42, 1.)?;
        let sx = if sx == 0. { 1. } else { sx };
        let sy = if sy == 0. { 1. } else { sy };
        for (source, b, suffix) in [
            (techbase, TECHBASE, "TECHBASE_LOGO"),
            (client, CLIENT, "OPDRACHTGEVER_LOGO"),
        ] {
            let Some(source) = source else {
                continue;
            };
            let xs = [x + b.min_x * sx, x + b.max_x * sx];
            let ys = [y + b.min_y * sy, y + b.max_y * sy];
            let bounds = Bounds {
                min_x: xs[0].min(xs[1]),
                min_y: ys[0].min(ys[1]),
                max_x: xs[0].max(xs[1]),
                max_y: ys[0].max(ys[1]),
            };
            if [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
                .into_iter()
                .any(|v| !v.is_finite() || v.abs() > 1e12)
            {
                return Err("Logovak ligt buiten bereik".into());
            }
            result.push(Image {
                path: source.to_owned(),
                bounds,
                definition_name: format!("{}_{suffix}", crate::python_values::uppercase(name)),
                inset: 0.,
                preserve_aspect: true,
                layout: Some(name.clone()),
            });
        }
    }
    Ok(result)
}
