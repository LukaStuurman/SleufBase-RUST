//! Original SleufBase job identity rules, shared by navigation and downloads.
use serde_json::Value;
fn text(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .filter_map(|k| value.get(*k))
        .map(|v| match v {
            Value::String(s) => s.trim().to_owned(),
            Value::Number(n) => n.to_string(),
            _ => String::new(),
        })
        .find(|s| !s.is_empty())
        .unwrap_or_default()
}
pub(crate) fn prefix(value: &Value) -> String {
    text(value, &["Prefix", "s3_root_dir"])
}
pub(crate) fn project(value: &Value, account: &str) -> (String, String) {
    let prefix = prefix(value);
    static PREFIX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^(.+)_(\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2})$").unwrap()
    });
    if let Some(c) = PREFIX.captures(&prefix) {
        return (c[1].to_owned(), c[2].to_owned());
    }
    let mail = [
        "project_mail",
        "email",
        "mail",
        "user_email",
        "customer_email",
    ]
    .iter()
    .map(|k| text(value, &[*k]))
    .find(|v| v.contains('@'))
    .unwrap_or_else(|| account.trim().to_owned());
    static DATE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\d{4}-\d{2}-\d{2}[T_ -]\d{2}[:-]\d{2}[:-]\d{2}").unwrap()
    });
    let date = DATE
        .find(&prefix)
        .map(|m| m.as_str().to_owned())
        .unwrap_or_else(|| text(value, &["project_date", "created_at"]));
    (
        mail,
        date.replace(['T', ' '], "_")
            .replace(':', "-")
            .chars()
            .take(19)
            .collect(),
    )
}
pub(crate) fn view_parts(value: &Value) -> Result<(String, String), String> {
    let (mail, date) = project(value, "");
    let id = text(value, &["user_id", "userId", "UserId", "userID", "UserID"]);
    let owner = if id.is_empty() { mail } else { id };
    if owner.is_empty()
        || date.is_empty()
        || owner.len() > 254
        || date.len() > 100
        || owner.chars().any(char::is_control)
        || date.chars().any(char::is_control)
    {
        return Err("Job heeft geen betrouwbare browseridentiteit".into());
    }
    Ok((owner, date))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_prefix_takes_precedence_over_creation_date() {
        let v = serde_json::json!({"Prefix":"user_name@example.test_2026-10-24_13-14-15","user_id":42,"created_at":"2026-10-25T08:00:00Z"});
        assert_eq!(
            view_parts(&v).unwrap(),
            ("42".into(), "2026-10-24_13-14-15".into())
        );
        assert_eq!(project(&v, "").0, "user_name@example.test");
    }
    #[test]
    fn date_and_email_fallbacks_match_original() {
        let v = serde_json::json!({"s3_root_dir":"invalid_2026-10-24T13:14:15_extra","user_email":"owner@example.test","created_at":"wrong"});
        assert_eq!(
            view_parts(&v).unwrap(),
            ("owner@example.test".into(), "2026-10-24_13-14-15".into())
        );
        assert!(view_parts(&serde_json::json!({"user_id":42})).is_err());
    }
}
