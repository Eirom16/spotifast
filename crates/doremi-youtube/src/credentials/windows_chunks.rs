//! Windows vault entries hold at most 2560 bytes. Oversized sessions stay in
//! that same OS-protected vault, split into bounded, generation-scoped entries.
//! Publish the small index last; a failed write preserves the previous session.
use super::{Error, SERVICE, entry_for};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ptr};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_NOT_FOUND, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0,
    },
    Security::Credentials::{
        CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredEnumerateW, CredFree,
    },
    System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
};

pub(super) const MAGIC: &[u8] = b"Doremi-YTM-vault-chunks\0";
const LIMIT: usize = 64 * 1024;
const CHUNK: usize = 2048;
const DIRECT_LIMIT: usize = 2560;

// No Debug: even the vault's index is private storage metadata.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u8,
    generation: String,
    length: usize,
    chunks: usize,
}
fn index(bytes: &[u8]) -> Result<Option<Index>, Error> {
    let Some(bytes) = bytes.strip_prefix(MAGIC) else {
        return Ok(None);
    };
    let value: Index = serde_json::from_slice(bytes).map_err(|_| Error::InvalidResponse)?;
    if value.version != 1 {
        return Err(Error::Unsupported);
    }
    if value.length == 0
        || value.length > LIMIT
        || value.chunks != value.length.div_ceil(CHUNK)
        || value.generation.len() != 32
        || !value.generation.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(Error::InvalidResponse);
    }
    Ok(Some(value))
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
struct Lock(HANDLE);
impl Lock {
    fn acquire(user: &str) -> Result<Self, Error> {
        if user.is_empty()
            || user.len() > 128
            || !user
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(Error::InvalidRequest);
        }
        let name = wide(&format!("Local\\Doremi-YTM-vault-{user}"));
        // A bounded OS mutex serializes readers/writers in this desktop logon.
        let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(Error::Unavailable);
        }
        let status = unsafe { WaitForSingleObject(handle, 5000) };
        if status == WAIT_OBJECT_0 || status == WAIT_ABANDONED {
            Ok(Self(handle))
        } else {
            unsafe { CloseHandle(handle) };
            Err(Error::Unavailable)
        }
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}
fn prefix(user: &str) -> String {
    format!("{SERVICE}/{user}/chunks/")
}
pub(super) fn chunk_target(user: &str, generation: &str, part: usize) -> String {
    format!("{}{generation}/{part:02}", prefix(user))
}
fn chunk_entry(user: &str, generation: &str, part: usize) -> Result<keyring_core::Entry, Error> {
    use keyring_core::api::CredentialStoreApi;
    let target = chunk_target(user, generation, part);
    windows_native_keyring_store::Store::new()
        .map_err(|_| Error::Unavailable)?
        .build(
            SERVICE,
            user,
            Some(&HashMap::from([("target", target.as_str())])),
        )
        .map_err(|_| Error::Unavailable)
}
struct Enumeration(*mut *mut CREDENTIALW);
impl Drop for Enumeration {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CredFree(self.0.cast()) }
        }
    }
}
pub(super) fn owned_targets(user: &str) -> Result<Vec<String>, Error> {
    let prefix = prefix(user);
    let filter = wide(&format!("{prefix}*"));
    let mut count = 0;
    let mut entries = ptr::null_mut();
    // The OS filters by our exact service/user namespace; never enumerate the
    // user's other applications. CredFree owns all returned pointers.
    if unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut entries) } == 0 {
        return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
            Ok(Vec::new())
        } else {
            Err(Error::Unavailable)
        };
    }
    let _allocation = Enumeration(entries);
    if count > 4096 || (count != 0 && entries.is_null()) {
        return Err(Error::TooLarge);
    }
    let mut targets = Vec::new();
    for part in 0..count as usize {
        // CredEnumerateW guarantees `count` valid credential pointers and
        // NUL-terminated TargetName strings while the allocation is alive.
        let credential = unsafe { *entries.add(part) };
        if credential.is_null() {
            return Err(Error::InvalidResponse);
        }
        if unsafe { (*credential).Type } != CRED_TYPE_GENERIC {
            continue;
        }
        let name = unsafe { (*credential).TargetName };
        if name.is_null() {
            return Err(Error::InvalidResponse);
        }
        let mut units = Vec::new();
        for at in 0..512 {
            let value = unsafe { *name.add(at) };
            if value == 0 {
                break;
            }
            units.push(value);
        }
        if units.len() == 512 {
            return Err(Error::TooLarge);
        }
        let target = String::from_utf16(&units).map_err(|_| Error::InvalidResponse)?;
        if target.starts_with(&prefix) {
            targets.push(target)
        }
    }
    Ok(targets)
}
fn delete(target: &str) -> Result<(), Error> {
    if unsafe { CredDeleteW(wide(target).as_ptr(), CRED_TYPE_GENERIC, 0) } != 0
        || unsafe { GetLastError() } == ERROR_NOT_FOUND
    {
        Ok(())
    } else {
        Err(Error::Unavailable)
    }
}
fn cleanup(user: &str, keep: Option<&str>) -> Result<(), Error> {
    let keep = keep.map(|generation| format!("{}{generation}/", prefix(user)));
    let mut result = Ok(());
    for target in owned_targets(user)? {
        if keep
            .as_ref()
            .is_none_or(|prefix| !target.starts_with(prefix))
            && delete(&target).is_err()
        {
            result = Err(Error::Unavailable);
        }
    }
    result
}
fn primary(user: &str) -> Result<Option<Vec<u8>>, Error> {
    match entry_for(user)?.get_secret() {
        Ok(bytes) => Ok(Some(bytes)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(_) => Err(Error::Unavailable),
    }
}
pub(super) fn load_for(user: &str) -> Result<Option<String>, Error> {
    let _lock = Lock::acquire(user)?;
    let Some(bytes) = primary(user)? else {
        return Ok(None);
    };
    let bytes = if let Some(index) = index(&bytes)? {
        let mut value = Vec::with_capacity(index.length);
        for part in 0..index.chunks {
            let chunk = chunk_entry(user, &index.generation, part)?
                .get_secret()
                .map_err(|_| Error::InvalidResponse)?;
            let expected = (index.length - value.len()).min(CHUNK);
            if chunk.len() != expected {
                return Err(Error::InvalidResponse);
            }
            value.extend_from_slice(&chunk);
        }
        value
    } else {
        bytes
    };
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| Error::InvalidResponse)
}
pub(super) fn store_for(user: &str, secret: &str) -> Result<(), Error> {
    if secret.is_empty() || secret.len() > LIMIT {
        return Err(Error::TooLarge);
    }
    let _lock = Lock::acquire(user)?;
    if let Some(bytes) = primary(user)? {
        let _ = index(&bytes)?;
    }
    if secret.len() <= DIRECT_LIMIT {
        entry_for(user)?
            .set_secret(secret.as_bytes())
            .map_err(|_| Error::Unavailable)?;
        // The active value is durable now. GC failure must not hide that fact
        // from the actor's cancellation cleanup; explicit logout reports it.
        let _ = cleanup(user, None);
        return Ok(());
    }
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).map_err(|_| Error::Unavailable)?;
    let generation = nonce
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect::<String>();
    let index = Index {
        version: 1,
        generation,
        length: secret.len(),
        chunks: secret.len().div_ceil(CHUNK),
    };
    let mut document = MAGIC.to_vec();
    document.extend(serde_json::to_vec(&index).map_err(|_| Error::InvalidResponse)?);
    let result = (|| {
        for (part, chunk) in secret.as_bytes().chunks(CHUNK).enumerate() {
            chunk_entry(user, &index.generation, part)?
                .set_secret(chunk)
                .map_err(|_| Error::Unavailable)?;
        }
        entry_for(user)?
            .set_secret(&document)
            .map_err(|_| Error::Unavailable)
    })();
    if result.is_err() {
        for part in 0..index.chunks {
            let _ = delete(&chunk_target(user, &index.generation, part));
        }
        return result;
    }
    let _ = cleanup(user, Some(&index.generation));
    Ok(())
}
pub(super) fn forget_for(user: &str) -> Result<(), Error> {
    let _lock = Lock::acquire(user)?;
    let chunks = cleanup(user, None);
    let primary = match entry_for(user)?.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(_) => Err(Error::Unavailable),
    };
    chunks.and(primary)
}
