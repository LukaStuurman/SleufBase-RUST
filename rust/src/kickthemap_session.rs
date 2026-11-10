//! Local account and session storage. Windows encrypts both files for the
//! current user. Project files and job caches never contain these secrets.
use serde_json::Value;
#[cfg(windows)]
use std::io::{Read, Write};
use std::{fs, io, path::PathBuf};

#[cfg(windows)]
const MAX_BYTES: usize = 256 * 1024;

fn path() -> PathBuf {
    crate::loaded_jobs::kickthemap_cache_dir().join("rust-session.dat")
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Account {
    pub email: String,
    pub password: String,
}

fn account_path() -> PathBuf {
    path().with_file_name("rust-account.dat")
}

#[cfg(windows)]
pub fn load_account() -> io::Result<Option<Account>> {
    let Some(bytes) = read_encrypted(account_path())? else { return Ok(None); };
    Ok(Some(serde_json::from_slice(&bytes)?))
}

#[cfg(not(windows))]
pub fn load_account() -> io::Result<Option<Account>> { Ok(None) }

#[cfg(windows)]
pub fn save_account(account: &Account) -> io::Result<()> {
    write_encrypted(account_path(), &serde_json::to_vec(account)?)
}

#[cfg(not(windows))]
pub fn save_account(_: &Account) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Account bewaren vereist Windows"))
}

pub fn forget_account() -> io::Result<()> {
    remove(account_path())
}

pub fn account_exists() -> bool {
    account_path().is_file()
}

pub fn exists() -> bool {
    path().is_file()
}

pub fn forget() -> io::Result<()> {
    remove(path())
}

fn remove(path: PathBuf) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(windows)]
fn read_encrypted(path: PathBuf) -> io::Result<Option<Vec<u8>>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    windows::transform(&bytes, false).map(Some)
}

#[cfg(windows)]
pub fn load() -> io::Result<Option<Value>> {
    let Some(plaintext) = read_encrypted(path())? else { return Ok(None); };
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
    let plaintext = serde_json::to_vec(&serde_json::json!({
        "saved_at": now_seconds(),
        "cookies": cookies,
    }))?;
    write_encrypted(path(), &plaintext)
}

#[cfg(windows)]
fn write_encrypted(path: PathBuf, plaintext: &[u8]) -> io::Result<()> {
    let parent = path.parent().expect("local account directory");
    fs::create_dir_all(parent)?;
    let encrypted = windows::transform(plaintext, true)?;
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
    fn remembered_account_is_encrypted_and_can_be_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("account.dat");
        let mut account = Account { email: "fixture@example.test".into(), password: "fixture-only-password".into() };
        for password in ["fixture-only-password", "new-fixture-password"] {
            account.password = password.into();
            write_encrypted(path.clone(), &serde_json::to_vec(&account).unwrap()).unwrap();
            let raw = fs::read(&path).unwrap();
            assert!(!raw.windows(password.len()).any(|bytes| bytes == password.as_bytes()));
            let plaintext = read_encrypted(path.clone()).unwrap().unwrap();
            let loaded: Account = serde_json::from_slice(&plaintext).unwrap();
            assert_eq!(loaded.email, account.email);
            assert_eq!(loaded.password, password);
        }
        remove(path.clone()).unwrap();
        assert!(read_encrypted(path).unwrap().is_none());
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
