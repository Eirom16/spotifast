//! System protected store. Never fall back to plaintext on a locked keyring.
use doremi_core::catalog::ProviderError as Error;
use keyring_core::api::CredentialStoreApi;
const SERVICE: &str = "dev.eirom.doremi.youtube-music";
const USER: &str = "session-v1";
fn entry() -> Result<keyring_core::Entry, Error> {
    entry_for(USER)
}
fn entry_for(user: &str) -> Result<keyring_core::Entry, Error> {
    entry_at(SERVICE, user)
}
fn entry_at(service: &str, user: &str) -> Result<keyring_core::Entry, Error> {
    #[cfg(target_os = "linux")]
    let store = zbus_secret_service_keyring_store::Store::new();
    #[cfg(target_os = "windows")]
    let store = windows_native_keyring_store::Store::new();
    store
        .map_err(|_| Error::Unavailable)?
        .build(service, user, None)
        .map_err(|_| Error::Unavailable)
}
pub fn load() -> Result<Option<String>, Error> {
    match entry()?.get_secret() {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| Error::InvalidResponse),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(_) => Err(Error::Unavailable),
    }
}
pub fn store(cookie: &str) -> Result<(), Error> {
    entry()?
        .set_secret(cookie.as_bytes())
        .map_err(|_| Error::Unavailable)
}
pub fn forget() -> Result<(), Error> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(_) => Err(Error::Unavailable),
    }
}

/// Explicit user-selected migration only. The source entry is read-only; the
/// caller must verify the musical account before writing Doremi's own entry.
pub fn import_doremi_rust() -> Result<String, Error> {
    let bytes = entry_at("doremi", "youtube_music_auth")?
        .get_secret()
        .map_err(|_| Error::Unavailable)?;
    crate::legacy_cookie(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a real unlocked system credential store; synthetic key only"]
    fn native_store_round_trip() {
        let name = format!("fixture-round-trip-{}", std::process::id());
        let entry = entry_for(&name).expect("native store unavailable");
        entry
            .set_secret(b"SAPISID=synthetic-fixture-only")
            .expect("native write failed");
        let value = entry.get_secret().expect("native read failed");
        let removed = entry.delete_credential();
        assert!(removed.is_ok());
        assert_eq!(value, b"SAPISID=synthetic-fixture-only");
        assert!(matches!(
            entry.get_secret(),
            Err(keyring_core::Error::NoEntry)
        ));
    }
}
