//! Authenticated legacy storage fallback used by original SleufBase.
//! Credentials are returned by KickTheMap for this session; never persisted or logged.
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use base64::Engine;
use md5::{Digest, Md5};
use serde::Deserialize;
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    pub prefix: String,
    pub owner: String,
}
impl Identity {
    pub fn key(&self, kind: crate::kickthemap_live::RemoteFileKind) -> Result<String, String> {
        if self.owner.is_empty() || self.prefix.is_empty() {
            return Err("Job mist de oorspronkelijke opslagidentiteit".into());
        }
        let filename = match kind {
            crate::kickthemap_live::RemoteFileKind::GeoTiff => format!("{}.tiff", self.prefix),
            _ => "jobFeatures.json".into(),
        };
        let key = format!("{}/{}/cloud/{filename}", self.owner, self.prefix);
        if key.len() > 8192
            || key.chars().any(char::is_control)
            || key.split('/').any(|v| matches!(v, "." | ".."))
        {
            return Err("Ongeldige job-opslagidentiteit".into());
        }
        Ok(key)
    }
}
#[derive(Deserialize)]
pub(crate) struct Credentials {
    #[serde(rename = "aws_access_key")]
    access: String,
    #[serde(rename = "aws_secret_access_key")]
    secret: String,
    bucket: String,
    region: String,
    #[serde(default, rename = "aws_session_token")]
    token: Option<String>,
}
impl Credentials {
    pub fn decode(payload: &str) -> Result<Self, String> {
        let decode = || -> Option<Self> {
            if payload.len() > 65536 {
                return None;
            }
            let outer: serde_json::Value = serde_json::from_str(payload).ok()?;
            let envelope: serde_json::Value =
                serde_json::from_str(outer["adata"].as_str()?).ok()?;
            let password = outer["thg"].as_str()?.as_bytes();
            if password.is_empty() || password.len() > 8192 {
                return None;
            }
            let salt = unhex(envelope["s"].as_str()?)?;
            if salt.len() != 8 {
                return None;
            }
            let mut data = base64::engine::general_purpose::STANDARD
                .decode(envelope["ct"].as_str()?)
                .ok()?;
            if data.is_empty() || data.len() > 32768 || data.len() % 16 != 0 {
                return None;
            }
            let mut derived = Vec::new();
            let mut previous = Vec::new();
            while derived.len() < 48 {
                let mut digest = Md5::new();
                digest.update(&previous);
                digest.update(password);
                digest.update(&salt);
                previous = digest.finalize().to_vec();
                derived.extend_from_slice(&previous);
            }
            let plain =
                cbc::Decryptor::<aes::Aes256>::new_from_slices(&derived[..32], &derived[32..48])
                    .ok()?
                    .decrypt_padded_mut::<Pkcs7>(&mut data)
                    .ok()?;
            serde_json::from_slice(plain).ok()
        };
        let config = decode().ok_or("KickTheMap gaf geen bruikbare opslagconfiguratie")?;
        if config.access.is_empty()
            || config.access.len() > 256
            || config.secret.is_empty()
            || config.secret.len() > 4096
            || config.region.is_empty()
            || config.region.len() > 32
            || !config
                .region
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            || config.bucket.len() < 3
            || config.bucket.len() > 63
            || !config
                .bucket
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b".-".contains(&c))
            || config.bucket.contains("..")
            || config.token.as_ref().is_some_and(|v| v.len() > 8192)
        {
            return Err("KickTheMap-opslagconfiguratie is ongeldig".into());
        }
        Ok(config)
    }
    pub fn signed_url(&self, key: &str, filename: &str) -> Result<reqwest::Url, String> {
        let now = time::OffsetDateTime::now_utc();
        let date = format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            now.year(),
            u8::from(now.month()),
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        );
        self.sign(key, Some(filename), &date, 900)
    }
    fn sign(
        &self,
        key: &str,
        filename: Option<&str>,
        date: &str,
        expires: u32,
    ) -> Result<reqwest::Url, String> {
        let date_only = &date[..8];
        let scope = format!("{date_only}/{}/s3/aws4_request", self.region);
        let host = if self.region == "us-east-1" {
            format!("{}.s3.amazonaws.com", self.bucket)
        } else {
            format!("{}.s3.{}.amazonaws.com", self.bucket, self.region)
        };
        let (host, key) = if self.bucket.contains('.') {
            (
                format!("s3.{}.amazonaws.com", self.region),
                format!("{}/{key}", self.bucket),
            )
        } else {
            (host, key.to_owned())
        };
        let path = format!(
            "/{}",
            key.split('/').map(encode).collect::<Vec<_>>().join("/")
        );
        let mut query = vec![
            ("X-Amz-Algorithm", "AWS4-HMAC-SHA256".into()),
            ("X-Amz-Credential", format!("{}/{scope}", self.access)),
            ("X-Amz-Date", date.into()),
            ("X-Amz-Expires", expires.to_string()),
            ("X-Amz-SignedHeaders", "host".into()),
        ];
        if let Some(token) = &self.token {
            query.push(("X-Amz-Security-Token", token.clone()));
        }
        if let Some(filename) = filename {
            if filename.len() > 32768 || filename.chars().any(char::is_control) {
                return Err("Ongeldige downloadnaam".into());
            }
            query.push((
                "response-content-disposition",
                format!(
                    "attachment; filename=\"{}\"",
                    filename.replace(['\"', '\\'], "_")
                ),
            ));
        }
        let mut query = query
            .into_iter()
            .map(|(k, v)| (encode(k), encode(&v)))
            .collect::<Vec<_>>();
        query.sort();
        let query = query
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        let canonical = format!("GET\n{path}\n{query}\nhost:{host}\n\nhost\nUNSIGNED-PAYLOAD");
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{date}\n{scope}\n{}",
            hex(ring::digest::digest(&ring::digest::SHA256, canonical.as_bytes()).as_ref())
        );
        let signing = hmac(
            &hmac(
                &hmac(
                    &hmac(format!("AWS4{}", self.secret).as_bytes(), date_only),
                    &self.region,
                ),
                "s3",
            ),
            "aws4_request",
        );
        crate::kickthemap_live::validate_signed_url(&format!(
            "https://{host}{path}?{query}&X-Amz-Signature={}",
            hex(&hmac(&signing, &string_to_sign))
        ))
    }
}
fn hmac(key: &[u8], value: &str) -> Vec<u8> {
    ring::hmac::sign(
        &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key),
        value.as_bytes(),
    )
    .as_ref()
    .to_vec()
}
fn hex(value: &[u8]) -> String {
    value.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(value.get(i..i + 2)?, 16).ok())
        .collect()
}
fn encode(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|&b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aws_published_signature_vector_matches() {
        let c = Credentials {
            access: "AKIAIOSFODNN7EXAMPLE".into(),
            secret: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            bucket: "examplebucket".into(),
            region: "us-east-1".into(),
            token: None,
        };
        let u = c.sign("test.txt", None, "20130524T000000Z", 86400).unwrap();
        assert!(u.as_str().ends_with(
            "X-Amz-Signature=aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"
        ));
    }
    #[test]
    fn original_python_envelope_and_independent_botocore_control_match() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/kickthemap/storage-control.json"
        ))
        .unwrap();
        let c = Credentials::decode(&fixture["payload"].to_string()).unwrap();
        let url = c
            .sign(
                fixture["key"].as_str().unwrap(),
                fixture["filename"].as_str(),
                fixture["date"].as_str().unwrap(),
                900,
            )
            .unwrap();
        let expected = reqwest::Url::parse(fixture["url"].as_str().unwrap()).unwrap();
        assert_eq!(url.host_str(), expected.host_str());
        assert_eq!(url.path(), expected.path());
        assert_eq!(
            url.query_pairs()
                .collect::<std::collections::BTreeMap<_, _>>(),
            expected
                .query_pairs()
                .collect::<std::collections::BTreeMap<_, _>>()
        );
    }
    #[test]
    fn invalid_envelopes_do_not_echo_sensitive_payloads() {
        for s in [
            "not-json",
            r#"{"adata":"secret-input","thg":"do-not-echo"}"#,
        ] {
            let e = Credentials::decode(s).err().unwrap();
            assert!(!e.contains("do-not-echo"));
        }
    }
}
