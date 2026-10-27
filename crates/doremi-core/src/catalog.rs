//! Provider-neutral search -> explicit selection -> resolution consumer.
//! Effects cross a worker boundary; this module never calls a provider or clock.

use crate::media::{Media, MediaId, ProviderId};
use crate::requests::{Lane, Router, RoutingError, SessionId, Ticket};
use std::fmt;

pub const MAX_RESULTS: u16 = 50;
pub const MAX_QUERY_BYTES: usize = 512;
pub const MAX_DEADLINE_MS: u64 = 30_000;

/// Milliseconds on a monotonic host timeline, not a persisted wall-clock timestamp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tick(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderError {
    Cancelled,
    Timeout,
    Unavailable,
    AccessDenied,
    RateLimited { retry_after_ms: Option<u64> },
    Unsupported,
    UnsupportedFormat,
    Transport,
    InvalidRequest,
    InvalidResponse,
    TooLarge,
    Exhausted,
}

impl From<RoutingError> for ProviderError {
    fn from(_: RoutingError) -> Self {
        Self::Exhausted
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioFormat {
    WavPcm16,
    AacMp4,
    OpusWebm,
    Flac,
}

#[derive(Clone, Debug)]
pub struct Capabilities {
    pub provider: ProviderId,
    pub search: bool,
    pub resolve: bool,
    pub max_results: u16,
    pub formats: Vec<AudioFormat>,
}

#[derive(Clone, Debug)]
pub struct SearchRequest {
    pub ticket: Ticket,
    pub provider: ProviderId,
    pub query: String,
    pub limit: u16,
    pub deadline: Tick,
}

#[derive(Clone, Debug)]
pub struct ResolveRequest {
    pub ticket: Ticket,
    pub media: MediaId,
    pub deadline: Tick,
}

/// Never serialize this wrapper or use its exposed value in logging.
#[derive(Clone, PartialEq, Eq)]
struct Sensitive(String);

impl fmt::Debug for Sensitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseBasis {
    ProviderExpiry,
    ConservativeHostLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamInfo {
    pub format: AudioFormat,
    pub byte_len: Option<u64>,
    pub seekable: bool,
    pub usable_until: Tick,
    pub lease_basis: LeaseBasis,
}

/// Ephemeral worker resource. Deliberately absent from Media and queue Snapshot.
/// No serde/Display; Debug redacts URI and header values. Clone is memory-only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemporaryResource {
    media: MediaId,
    uri: Sensitive,
    headers: Vec<(String, Sensitive)>,
    info: StreamInfo,
}

impl TemporaryResource {
    pub fn http(
        media: MediaId,
        uri: String,
        headers: Vec<(String, String)>,
        info: StreamInfo,
    ) -> Result<Self, ProviderError> {
        let authority = uri
            .strip_prefix("https://")
            .or_else(|| uri.strip_prefix("http://"))
            .and_then(|rest| rest.split(['/', '?', '#']).next());
        if uri.len() > 8192 || headers.len() > 16 {
            return Err(ProviderError::TooLarge);
        }
        if authority.is_none_or(|host| host.is_empty() || host.contains('@'))
            || uri.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(ProviderError::InvalidResponse);
        }
        if headers.iter().any(|(name, value)| {
            name.is_empty()
                || name.len() > 128
                || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                || value.len() > 8192
                || value.chars().any(char::is_control)
        }) {
            return Err(ProviderError::InvalidResponse);
        }
        Ok(Self {
            media,
            uri: Sensitive(uri),
            headers: headers
                .into_iter()
                .map(|(k, v)| (k, Sensitive(v)))
                .collect(),
            info,
        })
    }

    pub fn media(&self) -> &MediaId {
        &self.media
    }
    pub fn info(&self) -> StreamInfo {
        self.info
    }
    /// Transport only. Adapters must additionally validate hosts, redirects and access policy.
    pub fn expose_uri(&self) -> &str {
        &self.uri.0
    }
    pub fn expose_headers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.headers.iter().map(|(k, v)| (k.as_str(), v.0.as_str()))
    }
}

#[derive(Clone, Debug)]
pub enum ProviderCommand {
    Search(SearchRequest),
    Resolve(ResolveRequest),
    Cancel(Ticket),
}

#[derive(Debug)]
pub enum ProviderEvent {
    Search {
        ticket: Ticket,
        result: Result<Vec<Media>, ProviderError>,
    },
    Resolve {
        ticket: Ticket,
        result: Result<TemporaryResource, ProviderError>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Pending,
    Ready,
    Failed(ProviderError),
}

/// A real consumer of the protocol, used by the fixture harness, not by legacy UI.
/// Results have one owner; pending state contains fences, not another result list.
pub struct Catalog {
    caps: Capabilities,
    router: Router,
    search: Option<SearchRequest>,
    resolve: Option<ResolveRequest>,
    results: Vec<Media>,
    resource: Option<TemporaryResource>,
    search_status: Status,
    resolve_status: Status,
    now: Tick,
}

impl Catalog {
    pub fn new(session: SessionId, caps: Capabilities) -> Result<Self, ProviderError> {
        if caps.max_results > MAX_RESULTS
            || (caps.search && caps.max_results == 0)
            || caps.formats.len() > 8
            || (caps.resolve && caps.formats.is_empty())
        {
            return Err(ProviderError::InvalidRequest);
        }
        Ok(Self {
            caps,
            router: Router::new(session),
            search: None,
            resolve: None,
            results: Vec::new(),
            resource: None,
            search_status: Status::Idle,
            resolve_status: Status::Idle,
            now: Tick(0),
        })
    }

    pub fn results(&self) -> &[Media] {
        &self.results
    }
    pub fn resource(&self) -> Option<&TemporaryResource> {
        self.resource.as_ref()
    }
    pub fn search_status(&self) -> Status {
        self.search_status
    }
    pub fn resolve_status(&self) -> Status {
        self.resolve_status
    }

    fn deadline(&self, now: Tick, budget_ms: u64) -> Result<Tick, ProviderError> {
        if now < self.now || budget_ms == 0 || budget_ms > MAX_DEADLINE_MS {
            return Err(ProviderError::InvalidRequest);
        }
        now.0
            .checked_add(budget_ms)
            .map(Tick)
            .ok_or(ProviderError::InvalidRequest)
    }

    fn cancel_lane(&mut self, lane: Lane, out: &mut Vec<ProviderCommand>) {
        if let Some(ticket) = self.router.cancel(lane) {
            out.push(ProviderCommand::Cancel(ticket));
        }
    }

    pub fn search(
        &mut self,
        query: String,
        limit: u16,
        now: Tick,
        budget_ms: u64,
    ) -> Result<Vec<ProviderCommand>, ProviderError> {
        if !self.caps.search {
            return Err(ProviderError::Unsupported);
        }
        if query.trim().is_empty()
            || query.len() > MAX_QUERY_BYTES
            || limit == 0
            || limit > self.caps.max_results
        {
            return Err(ProviderError::InvalidRequest);
        }
        let deadline = self.deadline(now, budget_ms)?;
        // Reserve before cancellation so exhaustion cannot destroy a valid operation.
        let (ticket, old) = self.router.begin(Lane::Search)?;
        let mut out = old
            .into_iter()
            .map(ProviderCommand::Cancel)
            .collect::<Vec<_>>();
        self.cancel_lane(Lane::Resolve, &mut out);
        self.resolve = None;
        self.resource = None;
        self.resolve_status = Status::Idle;
        self.results.clear();
        self.now = now;
        let request = SearchRequest {
            ticket,
            provider: self.caps.provider.clone(),
            query,
            limit,
            deadline,
        };
        self.search = Some(request.clone());
        self.search_status = Status::Pending;
        out.push(ProviderCommand::Search(request));
        Ok(out)
    }

    /// Selection is an explicit stable ID returned by this search, never title matching.
    pub fn resolve(
        &mut self,
        media: MediaId,
        now: Tick,
        budget_ms: u64,
    ) -> Result<Vec<ProviderCommand>, ProviderError> {
        if !self.caps.resolve {
            return Err(ProviderError::Unsupported);
        }
        if !self.results.iter().any(|item| item.id == media) {
            return Err(ProviderError::InvalidRequest);
        }
        let deadline = self.deadline(now, budget_ms)?;
        let (ticket, old) = self.router.begin(Lane::Resolve)?;
        self.now = now;
        self.resource = None;
        let request = ResolveRequest {
            ticket,
            media,
            deadline,
        };
        self.resolve = Some(request.clone());
        self.resolve_status = Status::Pending;
        let mut out = old
            .into_iter()
            .map(ProviderCommand::Cancel)
            .collect::<Vec<_>>();
        out.push(ProviderCommand::Resolve(request));
        Ok(out)
    }

    pub fn cancel(&mut self) -> Vec<ProviderCommand> {
        let mut out = Vec::new();
        self.cancel_lane(Lane::Search, &mut out);
        self.cancel_lane(Lane::Resolve, &mut out);
        self.search = None;
        self.resolve = None;
        self.results.clear();
        self.resource = None;
        self.search_status = Status::Idle;
        self.resolve_status = Status::Idle;
        out
    }

    pub fn replace_session(
        &mut self,
        session: SessionId,
    ) -> Result<Vec<ProviderCommand>, ProviderError> {
        // Reject reused epochs instead of accepting callbacks from another host lifetime.
        let tickets = self
            .router
            .replace_session(session)
            .map_err(|_| ProviderError::InvalidRequest)?;
        self.search = None;
        self.resolve = None;
        self.results.clear();
        self.resource = None;
        self.search_status = Status::Idle;
        self.resolve_status = Status::Idle;
        Ok(tickets.into_iter().map(ProviderCommand::Cancel).collect())
    }

    pub fn advance(&mut self, now: Tick) -> Result<Vec<ProviderCommand>, ProviderError> {
        if now < self.now {
            return Err(ProviderError::InvalidRequest);
        }
        self.now = now;
        let mut out = Vec::new();
        if self.search.as_ref().is_some_and(|r| now >= r.deadline) {
            self.cancel_lane(Lane::Search, &mut out);
            self.search = None;
            self.search_status = Status::Failed(ProviderError::Timeout);
        }
        if self.resolve.as_ref().is_some_and(|r| now >= r.deadline) {
            self.cancel_lane(Lane::Resolve, &mut out);
            self.resolve = None;
            self.resolve_status = Status::Failed(ProviderError::Timeout);
        }
        if self
            .resource
            .as_ref()
            .is_some_and(|r| now >= r.info.usable_until)
        {
            self.resource = None;
            self.resolve_status = Status::Failed(ProviderError::Unavailable);
        }
        Ok(out)
    }

    pub fn complete(
        &mut self,
        event: ProviderEvent,
        now: Tick,
    ) -> Result<Vec<ProviderCommand>, ProviderError> {
        let out = self.advance(now)?;
        match event {
            ProviderEvent::Search { ticket, result }
                if self.router.accepts(ticket)
                    && self.search.as_ref().is_some_and(|r| r.ticket == ticket) =>
            {
                let request = self.search.take().expect("matching search");
                self.router.finish(ticket);
                let result = result.and_then(|items| {
                    if items.len() > usize::from(request.limit) {
                        return Err(ProviderError::TooLarge);
                    }
                    if items.iter().any(|m| {
                        m.id.provider != self.caps.provider
                            || m.id.local().len() > 1024
                            || m.title.len() > 4096
                    }) {
                        return Err(ProviderError::InvalidResponse);
                    }
                    Ok(items)
                });
                match result {
                    Ok(items) => {
                        self.results = items;
                        self.search_status = Status::Ready;
                    }
                    Err(error) => self.search_status = Status::Failed(error),
                }
            }
            ProviderEvent::Resolve { ticket, result }
                if self.router.accepts(ticket)
                    && self.resolve.as_ref().is_some_and(|r| r.ticket == ticket) =>
            {
                let request = self.resolve.take().expect("matching resolution");
                self.router.finish(ticket);
                let result = result.and_then(|resource| {
                    if resource.media != request.media || now >= resource.info.usable_until {
                        return Err(ProviderError::InvalidResponse);
                    }
                    if !self.caps.formats.contains(&resource.info.format) {
                        return Err(ProviderError::UnsupportedFormat);
                    }
                    Ok(resource)
                });
                match result {
                    Ok(resource) => {
                        self.resource = Some(resource);
                        self.resolve_status = Status::Ready;
                    }
                    Err(error) => self.resolve_status = Status::Failed(error),
                }
            }
            _ => {} // Old session/generation, cancelled, expired or duplicate completion.
        }
        Ok(out)
    }
}
