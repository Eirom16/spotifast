//! System protected store. Never fall back to plaintext on a locked keyring.
use doremi_core::catalog::ProviderError as Error;
use keyring_core::api::CredentialStoreApi;
const SERVICE: &str = "dev.eirom.doremi.youtube-music";
const USER: &str = "session-v1";
#[cfg(target_os = "windows")]
mod windows_chunks;
#[cfg(target_os = "linux")]
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
    #[cfg(target_os = "windows")]
    {
        windows_chunks::load_for(USER)
    }
    #[cfg(target_os = "linux")]
    match entry()?.get_secret() {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| Error::InvalidResponse),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(_) => Err(Error::Unavailable),
    }
}
pub fn store(cookie: &str) -> Result<(), Error> {
    #[cfg(target_os = "windows")]
    {
        windows_chunks::store_for(USER, cookie)
    }
    #[cfg(target_os = "linux")]
    entry()?
        .set_secret(cookie.as_bytes())
        .map_err(|_| Error::Unavailable)
}
pub fn forget() -> Result<(), Error> {
    #[cfg(target_os = "windows")]
    {
        windows_chunks::forget_for(USER)
    }
    #[cfg(target_os = "linux")]
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(_) => Err(Error::Unavailable),
    }
}

/// Explicit user-selected migration only. The source entry is read-only; the
/// caller must verify the musical account before writing Doremi's own entry.
pub fn import_doremi_rust() -> Result<String, Error> {
    #[cfg(target_os = "linux")]
    let headers = entry_at("doremi", "youtube_music_auth")?
        .get_secret()
        .map_err(|_| Error::Unavailable)?;
    #[cfg(target_os = "windows")]
    let headers = windows_legacy_headers("doremi", "youtube_music_auth")?.into_bytes();
    crate::legacy_cookie(&headers)
}
#[cfg(target_os = "windows")]
fn windows_legacy_headers(service: &str, user: &str) -> Result<String, Error> {
    // python-keyring writes a UTF-16 password under the service target, moving
    // colliding users to user@service. Doremi's own raw-secret target differs.
    let store = windows_native_keyring_store::Store::new().map_err(|_| Error::Unavailable)?;
    for target in [service.to_owned(), format!("{user}@{service}")] {
        let modifiers = std::collections::HashMap::from([("target", target.as_str())]);
        let entry = store
            .build(service, user, Some(&modifiers))
            .map_err(|_| Error::Unavailable)?;
        let attributes = match entry.get_attributes() {
            Ok(attributes) => attributes,
            Err(keyring_core::Error::NoEntry) => continue,
            Err(_) => return Err(Error::Unavailable),
        };
        if attributes.get("username").map(String::as_str) != Some(user) {
            continue;
        }
        return entry.get_password().map_err(|_| Error::InvalidResponse);
    }
    Err(Error::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires a native Windows vault; synthetic namespace only"]
    fn large_windows_sessions_round_trip_and_cleanup_without_overwriting_future_indexes() {
        use windows_chunks::{MAGIC, chunk_target, forget_for, load_for, owned_targets, store_for};
        let user = format!("fixture-large-session-{}", std::process::id());
        let fixture = format!("SAPISID=synthetic-only; EXTRA={}", "x".repeat(12000));
        let result = (|| {
            assert!(load_for(&user)?.is_none());
            store_for(&user, &fixture)?;
            assert_eq!(load_for(&user)?.as_deref(), Some(fixture.as_str()));
            let entry = entry_for(&user)?;
            let index = entry.get_secret().map_err(|_| Error::Unavailable)?;
            assert!(index.len() < 2560 && index.starts_with(MAGIC));
            let value: serde_json::Value = serde_json::from_slice(&index[MAGIC.len()..]).unwrap();
            let first = chunk_target(&user, value["generation"].as_str().unwrap(), 0);
            let store = windows_native_keyring_store::Store::new().unwrap();
            let first_entry = store
                .build(
                    SERVICE,
                    &user,
                    Some(&std::collections::HashMap::from([(
                        "target",
                        first.as_str(),
                    )])),
                )
                .unwrap();
            store_for(&user, "SAPISID=small-synthetic-only")?;
            assert!(matches!(
                first_entry.get_secret(),
                Err(keyring_core::Error::NoEntry)
            ));
            assert_eq!(
                load_for(&user)?.as_deref(),
                Some("SAPISID=small-synthetic-only")
            );
            store_for(&user, &fixture)?;
            assert_eq!(
                store_for(&user, &"x".repeat(65537)).err(),
                Some(Error::TooLarge)
            );
            let missing = owned_targets(&user)?.into_iter().next().unwrap();
            let missing_entry = store
                .build(
                    SERVICE,
                    &user,
                    Some(&std::collections::HashMap::from([(
                        "target",
                        missing.as_str(),
                    )])),
                )
                .unwrap();
            missing_entry.delete_credential().unwrap();
            assert_eq!(load_for(&user).err(), Some(Error::InvalidResponse));
            store_for(&user, &fixture)?;
            let index = entry.get_secret().unwrap();
            let mut future: serde_json::Value =
                serde_json::from_slice(&index[MAGIC.len()..]).unwrap();
            future["version"] = 2.into();
            let mut sealed = MAGIC.to_vec();
            sealed.extend(serde_json::to_vec(&future).unwrap());
            entry.set_secret(&sealed).unwrap();
            assert_eq!(load_for(&user).err(), Some(Error::Unsupported));
            assert_eq!(store_for(&user, &fixture).err(), Some(Error::Unsupported));
            assert_eq!(entry.get_secret().unwrap(), sealed);
            Ok::<_, Error>(())
        })();
        let removed = forget_for(&user);
        assert!(removed.is_ok(), "Synthetic vault cleanup failed");
        assert!(result.is_ok(), "Synthetic vault round trip failed");
        assert!(load_for(&user).unwrap().is_none());
        assert!(owned_targets(&user).unwrap().is_empty());
    }
    #[test]
    #[ignore = "requires a real unlocked system credential store; synthetic key only"]
    fn native_store_round_trip() {
        let name = format!("fixture-round-trip-{}", std::process::id());
        let entry = entry_for(&name).expect("native store unavailable");
        let fixture=crate::session::credential_document(&serde_json::json!({"schema":1,"cookie":"SAPISID=synthetic-fixture-only","visitor_data":"synthetic-visitor","data_sync_id":"synthetic-channel","auth_user":"1"}).to_string()).unwrap();
        entry
            .set_secret(fixture.as_bytes())
            .expect("native write failed");
        let value = entry.get_secret().expect("native read failed");
        let removed = entry.delete_credential();
        assert!(removed.is_ok());
        assert_eq!(value, fixture.as_bytes());
        assert!(matches!(
            entry.get_secret(),
            Err(keyring_core::Error::NoEntry)
        ));
    }
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires a native Windows credential store; synthetic target only"]
    fn python_keyring_service_target_and_utf16_are_read_without_changing_the_source() {
        let service = format!("dev.eirom.doremi.python-fixture-{}", std::process::id());
        let user = "fixture-user";
        let store = windows_native_keyring_store::Store::new().expect("native store unavailable");
        let modifiers = std::collections::HashMap::from([("target", service.as_str())]);
        let entry = store
            .build(&service, user, Some(&modifiers))
            .expect("fixture target unavailable");
        let headers = r#"{"cookie":"SAPISID=synthetic-fixture","x-goog-authuser":"0"}"#;
        entry.set_password(headers).expect("fixture write failed");
        entry
            .update_attributes(&std::collections::HashMap::from([("username", user)]))
            .expect("fixture username failed");
        let result = windows_legacy_headers(&service, user);
        let preserved = entry.get_password();
        let removed = entry.delete_credential();
        assert!(removed.is_ok());
        assert_eq!(result.unwrap(), headers);
        assert_eq!(preserved.unwrap(), headers);
    }
}
