//! Stable identities, not transient stream URLs or provider response objects.

/// Invalid identities are rejected at the adapter boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityError {
    InvalidProvider,
    EmptyLocalId,
}

/// A namespace such as `local` or an explicitly selected catalog provider.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
        let value = value.into();
        if value.is_empty()
            || !value
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
        {
            return Err(IdentityError::InvalidProvider);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Provider-local opaque identity. Adapters must supply stable IDs, not signed URLs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MediaId {
    pub provider: ProviderId,
    local: String,
}

impl MediaId {
    pub fn new(provider: ProviderId, local: impl Into<String>) -> Result<Self, IdentityError> {
        let local = local.into();
        if local.is_empty() {
            return Err(IdentityError::EmptyLocalId);
        }
        Ok(Self { provider, local })
    }

    pub fn local(&self) -> &str {
        &self.local
    }
}

/// Only metadata needed by F1A. Availability/streams belong to provider adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Media {
    pub id: MediaId,
    pub title: String,
    pub duration_ms: Option<u64>,
}
