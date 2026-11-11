//! Local, opt-in session storage. Windows encrypts the cookie payload for the
//! current user. Project files and job caches never contain these cookies.
use serde_json::Value;
#[cfg(windows)]
use std::io::{Read, Write};
use std::{fs, io, path::PathBuf};

#[cfg(windows)]
const MAX_BYTES: usize = 256 * 1024;

fn path() -> PathBuf {
    crate::loaded_jobs::kickthemap_cache_dir().join("rust-session.dat")
}

// Credentials are optional and live only in a Windows-user DPAPI encrypted file.
// Never persist an account in project files, snapshots, logs or plaintext settings.
fn account_path() -> PathBuf {
    crate::loaded_jobs::kickthemap_cache_dir().join("rust-account.dat")
}

pub struct Account {
    pub email: String,
    pub password: String,
}

pub fn account_exists() -> bool {
    account_path().is_file()
}

pub fn forget_account() -> io::Result<()> {
    match fs::remove_file(account_path()) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(windows)]
fn valid_account(account: &Account) -> bool {
    let email = account.email.trim();
    !email.is_empty() && email.len() <= 320
        && email.contains('@') && !email.chars().any(char::is_whitespace)
        && !account.password.is_empty() && account.password.len() <= 16384
}

#[cfg(windows)]
pub fn load_account() -> io::Result<Option<Account>> {
    let file = match fs::File::open(account_path()) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut encrypted = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut encrypted)?;
    let plaintext = windows::transform(&encrypted, false)?;
    let data: Value = serde_json::from_slice(&plaintext)?;
    let account = Account {
        email: data["email"].as_str().unwrap_or_default().to_owned(),
        password: data["password"].as_str().unwrap_or_default().to_owned(),
    };
    if !valid_account(&account) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Ongeldig bewaard KickTheMap-account"));
    }
    Ok(Some(account))
}

#[cfg(windows)]
pub fn save_account(account: &Account) -> io::Result<()> {
    if !valid_account(account) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Ongeldige KickTheMap-inloggegevens"));
    }
    let plaintext = serde_json::to_vec(&serde_json::json!({
        "email": account.email.trim(),
        "password": account.password,
    }))?;
    let encrypted = windows::transform(&plaintext, true)?;
    let path = account_path();
    let parent = path.parent().expect("local account directory");
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&encrypted)?;
    file.persist(path)?;
    Ok(())
}

#[cfg(not(windows))]
pub fn load_account() -> io::Result<Option<Account>> {
    Ok(None)
}

#[cfg(not(windows))]
pub fn save_account(_: &Account) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Account bewaren vereist Windows"))
}

pub fn exists() -> bool {
    path().is_file()
}

pub fn forget() -> io::Result<()> {
    match fs::remove_file(path()) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(windows)]
pub fn load() -> io::Result<Option<Value>> {
    let file = match fs::File::open(path()) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    let plaintext = windows::transform(&bytes, false)?;
    let value: Value = serde_json::from_slice(&plaintext)?;
    let now = now_seconds();
    let saved = value["saved_at"].as_u64().unwrap_or(0);
    if saved > now || now - saved > 7 * 24 * 60 * 60 {
        return Ok(None);
    }
    Ok(value.get("cookies").cloned())
}

#[cfg(windows)]
pub fn save(cookies: Value) -> io::Result<()> {
    let path = path();
    let parent = path.parent().expect("local session directory");
    fs::create_dir_all(parent)?;
    let plaintext = serde_json::to_vec(&serde_json::json!({
        "saved_at": now_seconds(),
        "cookies": cookies,
    }))?;
    let encrypted = windows::transform(&plaintext, true)?;
    if encrypted.len() > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Ongeldige sessiegrootte",
        ));
    }
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&encrypted)?;
    file.persist(path)?;
    Ok(())
}

#[cfg(windows)]
fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(not(windows))]
pub fn load() -> io::Result<Option<Value>> {
    Ok(None)
}

#[cfg(not(windows))]
pub fn save(_: Value) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Sessie bewaren vereist Windows",
    ))
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };

    pub(super) fn transform(input: &[u8], encrypt: bool) -> io::Result<Vec<u8>> {
        if input.is_empty() || input.len() > MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Ongeldige sessiegrootte",
            ));
        }
        let input = CRYPT_INTEGER_BLOB {
            cbData: input.len() as u32,
            pbData: input.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        // DPAPI allocates the result with LocalAlloc. The input is read-only.
        let ok = unsafe {
            if encrypt {
                CryptProtectData(
                    &input,
                    null(),
                    null(),
                    null(),
                    null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    null_mut(),
                    null(),
                    null(),
                    null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            }
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let result = unsafe {
            let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            LocalFree(output.pbData.cast());
            bytes
        };
        Ok(result)
    }

    #[test]
    fn credential_payload_is_protected_and_only_available_to_windows_user() {
        let account = Account { email: "fixture@example.test".into(), password: "unique-test-password-93847".into() };
        assert!(valid_account(&account));
        let plaintext = serde_json::to_vec(&serde_json::json!({"email": account.email, "password": account.password})).unwrap();
        let encrypted = transform(&plaintext, true).unwrap();
        assert!(!encrypted.windows(account.password.len()).any(|x| x == account.password.as_bytes()));
        assert_eq!(transform(&encrypted, false).unwrap(), plaintext);
    }

    #[test]
    fn saved_cookie_bytes_are_encrypted_and_tampering_fails() {
        let cookie = b"fixture-only-session-cookie";
        let mut encrypted = transform(cookie, true).unwrap();
        assert!(!encrypted.windows(cookie.len()).any(|part| part == cookie));
        assert_eq!(transform(&encrypted, false).unwrap(), cookie);
        let middle = encrypted.len() / 2;
        encrypted[middle] ^= 1;
        assert!(transform(&encrypted, false).is_err());
    }
}
