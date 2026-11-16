//! Native cable/pipe form values and a deliberately narrow DOM bridge.
//!
//! KickTheMap owns drawing and saving. These scripts only read the current
//! properties form or fill it after an explicit user action in the native UI.
use serde::{Deserialize, Serialize};

const BRIDGE: &str = include_str!("kickthemap_form.js");
const MAX_REPLY_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormValues {
    pub name: String,
    pub material: String,
    pub diameter: String,
    pub bundle: String,
    pub cover_band: bool,
}

impl FormValues {
    /// Keep provider text verbatim; only the reserved Attribute 3 value has
    /// special meaning. No unit conversion or material whitelist is applied.
    pub fn from_attributes(name: &str, material: &str, diameter: &str, attribute3: &str) -> Self {
        let cover_band = attribute3.trim().eq_ignore_ascii_case("dekband");
        Self {
            name: name.into(),
            material: material.into(),
            diameter: diameter.into(),
            bundle: if cover_band { String::new() } else { attribute3.into() },
            cover_band,
        }
    }

    pub fn attribute3(&self) -> &str {
        if self.cover_band { "dekband" } else { &self.bundle }
    }

    pub fn validate(&self) -> Result<(), String> {
        for (label, value, limit) in [
            ("Kabel/leiding", self.name.as_str(), 512),
            ("Materiaal", self.material.as_str(), 2048),
            ("Diameter", self.diameter.as_str(), 2048),
            ("Bundel", self.bundle.as_str(), 2048),
        ] {
            validate_text(value, limit, label)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormSnapshot {
    pub token: String,
    pub url: String,
    pub values: FormValues,
}

impl FormSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.token.is_empty() || self.token.len() > 128
            || !self.token.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err("KickTheMap-formulier heeft geen geldige selectiecode.".into());
        }
        validate_job_url(&self.url)?;
        self.values.validate()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeReply {
    pub snapshot: Option<FormSnapshot>,
    pub message: String,
}

impl BridgeReply {
    pub fn from_json(raw: &str) -> Result<Self, String> {
        if raw.len() > MAX_REPLY_BYTES {
            return Err("KickTheMap-formulierantwoord is te groot.".into());
        }
        let reply: Self = serde_json::from_str(raw)
            .map_err(|_| "KickTheMap gaf geen geldig formulierantwoord terug.".to_owned())?;
        validate_text(&reply.message, 2048, "Formuliermelding")?;
        if let Some(snapshot) = &reply.snapshot {
            snapshot.validate()?;
        }
        Ok(reply)
    }
}

fn validate_text(value: &str, limit: usize, label: &str) -> Result<(), String> {
    if value.chars().count() > limit {
        return Err(format!("{label} mag maximaal {limit} tekens bevatten."));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{label} bevat een ongeldig besturingsteken."));
    }
    Ok(())
}

fn validate_job_url(raw: &str) -> Result<(), String> {
    let invalid = || "Open eerst een geldige KickTheMap-job in het ingebouwde venster.".to_owned();
    if raw.len() > 4096 || raw.chars().any(char::is_control) {
        return Err(invalid());
    }
    let url = reqwest::Url::parse(raw).map_err(|_| invalid())?;
    let route = url.path().strip_prefix("/viewJob/").filter(|s| !s.is_empty() && !s.contains('/'));
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("www.my.kickthemap.com" | "my.kickthemap.com"))
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty() || url.password().is_some()
        || url.query().is_some() || url.fragment().is_some() || route.is_none()
    {
        return Err(invalid());
    }
    Ok(())
}

/// Returns a JSON string containing BridgeReply when evaluated in the owned
/// top-level job page. Merely taking a snapshot never changes a form field.
pub fn snapshot_script() -> String {
    format!("{BRIDGE}({{\"action\":\"snapshot\"}})")
}

/// All user text is encoded as JSON data, never interpolated into executable
/// JavaScript. A successful fill returns a fresh snapshot and consumes the old
/// token. The browser's own save action is deliberately not invoked.
pub fn apply_script(snapshot: &FormSnapshot, values: &FormValues) -> Result<String, String> {
    snapshot.validate()?;
    values.validate()?;
    let request = serde_json::json!({
        "action": "apply",
        "token": snapshot.token,
        "url": snapshot.url,
        "original": snapshot.values,
        "fields": [&values.name, &values.material, &values.diameter, values.attribute3()],
    });
    let encoded = serde_json::to_string(&request)
        .map_err(|_| "Kabelgegevens konden niet worden voorbereid.".to_owned())?;
    Ok(format!("{BRIDGE}({encoded})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> FormSnapshot {
        FormSnapshot {
            token: "fixture-selection-1".into(),
            url: "https://www.my.kickthemap.com/viewJob/123_2026-11-16".into(),
            values: FormValues::from_attributes("Onbekende leiding", "Gietijzer speciaal", " 2 × Ø110 / 63,5 mm ", "3 + reserve"),
        }
    }

    #[test]
    fn custom_values_and_diameter_text_round_trip_without_normalization() {
        let original = snapshot();
        let reply = BridgeReply { snapshot: Some(original.clone()), message: "Gelezen".into() };
        assert_eq!(BridgeReply::from_json(&serde_json::to_string(&reply).unwrap()).unwrap(), reply);
        assert_eq!(original.values.attribute3(), "3 + reserve");
        assert_eq!(original.values.diameter, " 2 × Ø110 / 63,5 mm ");
    }

    #[test]
    fn cover_band_uses_the_original_reserved_attribute_without_losing_a_draft_bundle() {
        let mut values = FormValues::from_attributes("ls", "", "", "  DeKBand  ");
        assert!(values.cover_band);
        assert!(values.bundle.is_empty());
        values.bundle = "4".into();
        assert_eq!(values.attribute3(), "dekband");
        values.cover_band = false;
        assert_eq!(values.attribute3(), "4");
    }

    #[test]
    fn apply_serializes_untrusted_text_as_json_instead_of_executable_script() {
        let old = snapshot();
        let mut values = old.values.clone();
        values.material = "PE \" ); globalThis.injected = true; // \\ </script>".into();
        let script = apply_script(&old, &values).unwrap();
        let json = script.strip_prefix(BRIDGE).unwrap().strip_prefix('(').unwrap().strip_suffix(')').unwrap();
        let request: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(request["fields"][1], values.material);
        assert_eq!(request["original"]["diameter"], old.values.diameter);
        assert_eq!(request["token"], old.token);
    }

    #[test]
    fn rejects_oversized_values_and_control_characters_before_building_apply() {
        let old = snapshot();
        for value in ["x".repeat(513), "name\0suffix".into(), "name\nline".into()] {
            let values = FormValues { name: value, ..old.values.clone() };
            assert!(apply_script(&old, &values).is_err());
        }
        let mut values = old.values.clone();
        values.material = "é".repeat(2048);
        assert!(values.validate().is_ok());
        values.material.push('é');
        assert!(values.validate().is_err());
    }

    #[test]
    fn rejects_malformed_protocol_and_unrelated_or_invalid_selection_contexts() {
        for raw in ["null", "{}", r#"{"snapshot":null,"message":"ok","unexpected":1}"#] {
            assert!(BridgeReply::from_json(raw).is_err());
        }
        for url in [
            "https://example.com/viewJob/1", "http://www.my.kickthemap.com/viewJob/1",
            "https://www.my.kickthemap.com/signin", "https://www.my.kickthemap.com/viewJob/",
            "https://www.my.kickthemap.com/viewJob/1/other", "https://www.my.kickthemap.com/viewJob/1#other",
            "https://www.my.kickthemap.com/viewJob/1?q=other", "https://user@www.my.kickthemap.com/viewJob/1",
            "https://www.my.kickthemap.com:444/viewJob/1",
        ] {
            let mut old = snapshot();
            old.url = url.into();
            assert!(apply_script(&old, &old.values).is_err(), "{url}");
        }
        let mut old = snapshot();
        old.token.clear();
        assert!(apply_script(&old, &old.values).is_err());
        // A stale selection is a normal, non-success reply, not a parse error.
        let stale = BridgeReply::from_json(r#"{"snapshot":null,"message":"De selectie is gewijzigd."}"#).unwrap();
        assert!(stale.snapshot.is_none());
    }
}
