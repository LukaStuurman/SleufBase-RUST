//! Original per-slot profile scale parsing and source-metadata defaults.
use serde_json::Value;
pub const METADATA_KEY: &str = "template_cross_section_scale_denominator";
fn whitespace(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}
fn digits(text: &str) -> Option<i64> {
    let mut found = false;
    let mut value = 0i64;
    for c in text.chars() {
        if let Some(n) = crate::object_layout::decimal(c) {
            found = true;
            value = (value * 10 + i64::from(n)).min(5001);
        } else {
            break;
        }
    }
    found.then_some(value)
}
fn extracted(text: &str) -> Option<i64> {
    // Python first searches for literal '1 : digits', then any digit run.
    for (i, c) in text.char_indices() {
        if c == '1' {
            let tail = text[i + 1..].trim_start_matches(whitespace);
            if let Some(tail) = tail.strip_prefix(':') {
                if let Some(n) = digits(tail.trim_start_matches(whitespace)) {
                    return Some(n);
                }
            }
        }
    }
    text.char_indices()
        .find(|(_, c)| crate::object_layout::decimal(*c).is_some())
        .and_then(|(i, _)| digits(&text[i..]))
}
fn auto(text: &str) -> bool {
    let t = crate::python_values::casefold(crate::python_values::trim(text));
    t.is_empty() || matches!(t.as_str(), "auto" | "automatisch")
}
pub fn parse(text: &str) -> Result<Option<i64>, String> {
    if auto(text) {
        return Ok(None);
    }
    let n = extracted(text).ok_or("Vul Auto of een schaal zoals 1:40 in.")?;
    if n < 20 {
        return Err("De schaal moet 1:20 of kleiner zijn, bijvoorbeeld 1:40.".into());
    }
    Ok(Some(n.min(5000)))
}
pub fn coerce(value: Option<&Value>) -> Option<i64> {
    let n = match value? {
        Value::String(s) => {
            if auto(s) {
                return None;
            }
            extracted(s)? as f64
        }
        v => crate::python_values::number(Some(v))?.round_ties_even(),
    };
    if !n.is_finite() || n < 20. {
        None
    } else {
        Some(n.min(5000.) as i64)
    }
}
