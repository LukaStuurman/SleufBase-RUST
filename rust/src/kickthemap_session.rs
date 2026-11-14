//! Local, opt-in session storage. Windows encrypts the cookie payload for the
//! current user. Project files and job caches never contain these cookies.
use serde_json::Value;
#[cfg(windows)]
use std::io::{Read, Write};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const PROFILE: &str = "rust-webview-profile";
const SESSION: &str = "rust-session.dat";
const ACCOUNT: &str = "rust-account.dat";
const SIGNED_OUT: &str = "rust-login-disabled";

#[cfg(windows)]
const MAX_BYTES: usize = 256 * 1024;

fn path() -> PathBuf {
    crate::loaded_jobs::kickthemap_cache_dir().join(SESSION)
}

// Credentials are optional and live only in a Windows-user DPAPI encrypted file.
// Never persist an account in project files, snapshots, logs or plaintext settings.
fn account_path() -> PathBuf {
    crate::loaded_jobs::kickthemap_cache_dir().join(ACCOUNT)
}

pub struct Account {
    pub email: String,
    pub password: String,
}

pub fn account_exists() -> bool {
    saved_account_in(&crate::loaded_jobs::kickthemap_cache_dir())
}

/// Explicit sign-out remains effective even if another process holds the profile.
/// This marker contains no account data and is removed only after a verified,
/// opt-in sign-in has successfully persisted its new account or session.
pub fn automatic_login_disabled() -> bool {
    login_disabled_in(&crate::loaded_jobs::kickthemap_cache_dir())
}

fn login_disabled_in(root: &Path) -> bool {
    root.join(SIGNED_OUT).is_file()
}

fn saved_account_in(root: &Path) -> bool {
    !login_disabled_in(root) && root.join(ACCOUNT).is_file()
}

fn saved_session_in(root: &Path) -> bool {
    !login_disabled_in(root) && (root.join(SESSION).is_file() || root.join(PROFILE).is_dir())
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

pub fn sign_out() -> io::Result<()> {
    sign_out_from(&crate::loaded_jobs::kickthemap_cache_dir())
}

fn sign_out_from(root: &Path) -> io::Result<()> {
    let disabled = disable_saved_login(root);
    // Always attempt both cleanups, including when the opt-out write fails.
    let session = forget_from(root);
    let account = remove_if_present(&root.join(ACCOUNT));
    disabled.and(session).and(account)
}

fn disable_saved_login(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root)
        .and_then(|()| fs::write(root.join(SIGNED_OUT), b"Explicitly signed out\n"))
}

#[cfg(any(windows,test))]
fn replace_account_in(root: &Path, write_account: impl FnOnce()->io::Result<()>) -> io::Result<()> {
    // No older browser/account identity may remain eligible for auto-login if
    // encryption, cleanup or saving the new verified account fails.
    sign_out_from(root)?;
    write_account()?;
    remove_if_present(&root.join(SIGNED_OUT))
}

#[cfg(any(windows,test))]
fn replace_browser_session_in(root: &Path, write_session: impl FnOnce()->io::Result<()>) -> io::Result<()> {
    // Keep the live browser profile: this process owns it and has just verified
    // its account. Older cookies and native credentials must not outlive it.
    let disabled=disable_saved_login(root);
    let old_session=remove_if_present(&root.join(SESSION));
    let old_account=remove_if_present(&root.join(ACCOUNT));
    disabled.and(old_session).and(old_account)?;
    write_session()?;
    remove_if_present(&root.join(SIGNED_OUT))
}

pub fn forget_account() -> io::Result<()> {
    remove_if_present(&account_path())
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
    if automatic_login_disabled() { return Ok(None); }
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
    let path=account_path();
    let parent=path.parent().expect("local account directory");
    replace_account_in(parent, || {
        if !valid_account(account) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,"Ongeldige KickTheMap-inloggegevens"));
        }
        let plaintext=serde_json::to_vec(&serde_json::json!({
            "email":account.email.trim(),"password":account.password,
        }))?;
        let encrypted=windows::transform(&plaintext,true)?;
        let mut file=tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&encrypted)?;
        file.persist(&path)?;
        Ok(())
    })
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
    saved_session_in(&crate::loaded_jobs::kickthemap_cache_dir())
}

pub(crate) fn browser_profile() -> PathBuf {
    crate::loaded_jobs::kickthemap_cache_dir().join(PROFILE)
}
#[cfg(windows)]
pub(crate) fn lock_profile() -> io::Result<fs::File> {
    lock_profile_in(&crate::loaded_jobs::kickthemap_cache_dir())
}

fn lock_profile_in(root: &Path) -> io::Result<fs::File> {
    fs::create_dir_all(root)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("rust-webview-profile.lock"))?;
    file.try_lock().map_err(io::Error::from)?;
    Ok(file)
}

pub fn forget() -> io::Result<()> {
    forget_from(&crate::loaded_jobs::kickthemap_cache_dir())
}

fn forget_from(root: &Path) -> io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let _lock = lock_profile_in(root)?;
    match fs::remove_dir_all(root.join(PROFILE)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        result => result?,
    }
    match fs::remove_file(root.join(SESSION)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(test)]
#[test]
fn logout_clears_only_the_saved_profile_and_session_after_its_owner_exits() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join(PROFILE);
    let session = root.path().join(SESSION);
    let jobs = root.path().join("jobs_cache");
    fs::create_dir(&profile).unwrap();
    fs::write(profile.join("fixture"), b"test account only").unwrap();
    fs::write(&session, b"test session only").unwrap();
    fs::create_dir(&jobs).unwrap();
    let held = lock_profile_in(root.path()).unwrap();
    assert!(forget_from(root.path()).is_err());
    assert!(profile.is_dir() && session.is_file());
    drop(held);
    forget_from(root.path()).unwrap();
    assert!(!profile.exists() && !session.exists() && jobs.is_dir());
}

#[cfg(windows)]
pub fn load() -> io::Result<Option<Value>> {
    if automatic_login_disabled() { return Ok(None); }
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
    let path=path();
    let parent=path.parent().expect("local session directory");
    replace_browser_session_in(parent, || {
        if cookies.as_array().is_none_or(|cookies|cookies.is_empty()) {
            return Err(io::Error::new(io::ErrorKind::InvalidData,"Geen geldige sessiecookies"));
        }
        let plaintext=serde_json::to_vec(&serde_json::json!({
            "saved_at":now_seconds(),"cookies":cookies,
        }))?;
        let encrypted=windows::transform(&plaintext,true)?;
        if encrypted.len()>MAX_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData,"Ongeldige sessiegrootte"));
        }
        let mut file=tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&encrypted)?;
        file.persist(&path)?;
        Ok(())
    })
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

#[cfg(test)]
mod sign_out_tests {
    use super::*;

    #[test]
    fn explicit_signout_removes_credentials_and_keeps_optout_after_cleanup() {
        let root=tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(PROFILE)).unwrap();
        fs::write(root.path().join(SESSION),b"corrupt session fixture").unwrap();
        fs::write(root.path().join(ACCOUNT),b"corrupt account fixture").unwrap();
        sign_out_from(root.path()).unwrap();
        assert!(login_disabled_in(root.path()));
        assert!(!root.path().join(PROFILE).exists());
        assert!(!root.path().join(SESSION).exists());
        assert!(!root.path().join(ACCOUNT).exists());
        // Cleanup and subsequent sign-outs must never re-enable automatic login.
        forget_from(root.path()).unwrap();
        sign_out_from(root.path()).unwrap();
        assert!(login_disabled_in(root.path()));
    }

    #[test]
    fn native_account_and_browser_session_have_separate_resume_paths() {
        let root=tempfile::tempdir().unwrap();
        fs::write(root.path().join(ACCOUNT),b"native account fixture").unwrap();
        assert!(saved_account_in(root.path()));
        assert!(!saved_session_in(root.path()));
        fs::remove_file(root.path().join(ACCOUNT)).unwrap();
        fs::create_dir(root.path().join(PROFILE)).unwrap();
        assert!(saved_session_in(root.path()));
        assert!(!saved_account_in(root.path()));
        // An opt-out applies even if account/session contents are corrupt.
        fs::write(root.path().join(ACCOUNT),b"corrupt account").unwrap();
        fs::write(root.path().join(SESSION),b"corrupt session").unwrap();
        fs::write(root.path().join(SIGNED_OUT),b"signed out").unwrap();
        assert!(!saved_account_in(root.path()));
        assert!(!saved_session_in(root.path()));
    }

    #[test]
    fn replacing_native_account_waits_for_cleanup_before_writing_or_enabling() {
        let root=tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(PROFILE)).unwrap();
        fs::write(root.path().join(SESSION),b"old browser account").unwrap();
        fs::write(root.path().join(ACCOUNT),b"old native account").unwrap();
        let lock=lock_profile_in(root.path()).unwrap();
        let mut wrote=false;
        assert!(replace_account_in(root.path(),||{wrote=true;Ok(())}).is_err());
        assert!(!wrote && login_disabled_in(root.path()));
        assert!(!saved_session_in(root.path()) && !saved_account_in(root.path()));
        drop(lock);
        replace_account_in(root.path(),||{
            assert!(!root.path().join(PROFILE).exists());
            assert!(!root.path().join(SESSION).exists());
            assert!(login_disabled_in(root.path()));
            fs::write(root.path().join(ACCOUNT),b"new verified native account")
        }).unwrap();
        assert!(saved_account_in(root.path()) && !saved_session_in(root.path()));
        assert!(!login_disabled_in(root.path()));
    }

    #[test]
    fn failed_account_or_browser_save_keeps_old_identity_disabled() {
        for browser in [false,true] {
            let root=tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join(PROFILE)).unwrap();
            fs::write(root.path().join(SESSION),b"old browser account").unwrap();
            fs::write(root.path().join(ACCOUNT),b"old native account").unwrap();
            let fail=||Err(io::Error::other("simulated encrypted-write failure"));
            let result=if browser {replace_browser_session_in(root.path(),fail)}
                else {replace_account_in(root.path(),fail)};
            assert!(result.is_err());
            assert!(login_disabled_in(root.path()));
            assert!(!saved_session_in(root.path()) && !saved_account_in(root.path()));
            assert!(!root.path().join(SESSION).exists() && !root.path().join(ACCOUNT).exists());
        }
    }

    #[test]
    fn verified_browser_save_keeps_its_profile_and_removes_old_native_account() {
        let root=tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(PROFILE)).unwrap();
        fs::write(root.path().join(ACCOUNT),b"another native account").unwrap();
        let lock=lock_profile_in(root.path()).unwrap();
        replace_browser_session_in(root.path(),||{
            assert!(login_disabled_in(root.path()));
            assert!(!root.path().join(ACCOUNT).exists());
            fs::write(root.path().join(SESSION),b"verified current browser session")
        }).unwrap();
        assert!(saved_session_in(root.path()) && !saved_account_in(root.path()));
        assert!(root.path().join(PROFILE).exists());
        assert!(!login_disabled_in(root.path()));
        drop(lock);
    }

    #[test]
    fn locked_profile_cannot_reenable_automatic_login_after_signout() {
        let root=tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(PROFILE)).unwrap();
        fs::write(root.path().join(SESSION),b"existing session fixture").unwrap();
        fs::write(root.path().join(ACCOUNT),b"existing account fixture").unwrap();
        let lock=lock_profile_in(root.path()).unwrap();
        assert!(sign_out_from(root.path()).is_err());
        assert!(login_disabled_in(root.path()));
        assert!(root.path().join(PROFILE).exists());
        assert!(root.path().join(SESSION).exists());
        assert!(!saved_session_in(root.path()));
        assert!(!saved_account_in(root.path()));
        assert!(!root.path().join(ACCOUNT).exists());
        drop(lock);
        forget_from(root.path()).unwrap();
        assert!(login_disabled_in(root.path()));
    }
}
