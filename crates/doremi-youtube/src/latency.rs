//! In-memory playback timestamps. No media identity, URL, account or payload.
//! All marks use one monotonic request origin, including reader worker marks.
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

#[derive(Clone, Copy)]
pub enum Stage {
    Queued,
    Worker,
    Resolve,
    ClientWait,
    ClientAcquired,
    Extraction,
    Extracted,
    Verified,
    Http,
    FirstBytes,
}

pub const STAGES: [&str; 10] = [
    "queued_us",
    "worker_us",
    "resolve_us",
    "client_wait_us",
    "client_acquired_us",
    "extraction_us",
    "extracted_us",
    "verified_us",
    "http_us",
    "first_bytes_us",
];

pub struct Trace {
    origin: Instant,
    marks: [AtomicU64; STAGES.len()],
    http: std::sync::Mutex<Option<std::collections::VecDeque<HttpRead>>>,
}

impl Default for Trace {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

impl Trace {
    pub fn new(origin: Instant) -> Self {
        Self {
            origin,
            http: std::sync::Mutex::new(None),
            marks: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    pub fn origin(&self) -> Instant {
        self.origin
    }

    pub fn mark(&self, stage: Stage) {
        let elapsed = self.origin.elapsed().as_micros().max(1) as u64;
        // A seek within the reader must not replace the first HTTP/bytes mark.
        let _ = self.marks[stage as usize].compare_exchange(
            0,
            elapsed,
            Ordering::Release,
            Ordering::Relaxed,
        );
    }

    pub fn snapshot(&self) -> [Option<u64>; STAGES.len()] {
        std::array::from_fn(|i| match self.marks[i].load(Ordering::Acquire) {
            0 => None,
            value => Some(value),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_stages_are_not_zero_latency_and_first_marks_cannot_be_overwritten() {
        let trace = Trace::default();
        assert!(trace.snapshot().iter().all(Option::is_none));
        trace.mark(Stage::Http);
        let first = trace.snapshot();
        trace.mark(Stage::Http);
        assert_eq!(trace.snapshot(), first);
        assert!(first[Stage::Http as usize].is_some());
        assert!(first[Stage::FirstBytes as usize].is_none());
    }
}

/// Safe per-operation evidence. No strings derived from headers or errors.
#[derive(Clone, Debug, serde::Serialize)]
pub struct HttpRead {
    pub sequence: u64,
    pub offset: u64,
    pub requested: u64,
    pub started_us: u64,
    pub ended_us: u64,
    pub status: u16,
    pub received: u64,
    pub content_length: Option<u64>,
    pub length_valid: Option<bool>,
    pub range_valid: Option<bool>,
    pub first_data_us: Option<u64>,
    pub outcome: &'static str,
    pub cancelled: bool,
    pub timeout: bool,
    pub eof: bool,
    pub incomplete: bool,
}
impl Trace {
    /// QA explicitly enables collection; normal playback keeps no HTTP records.
    pub fn enable_http(&self) {
        let mut ring = self.http.lock().unwrap_or_else(|e| e.into_inner());
        if ring.is_none() {
            *ring = Some(std::collections::VecDeque::with_capacity(256));
        }
    }
    pub fn http_snapshot(&self) -> Vec<HttpRead> {
        self.http
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|ring| ring.iter().cloned().collect())
            .unwrap_or_default()
    }
    pub(crate) fn record_http(&self, record: HttpRead) {
        let mut guard = self.http.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ring) = guard.as_mut() {
            if ring.len() == 256 {
                ring.pop_front();
            }
            ring.push_back(record);
        }
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    #[test]
    fn http_ring_is_opt_in_and_discards_oldest_records() {
        let trace = Trace::default();
        let row = HttpRead {
            sequence: 0,
            offset: 0,
            requested: 1,
            started_us: 0,
            ended_us: 1,
            status: 206,
            received: 1,
            content_length: Some(1),
            length_valid: Some(true),
            range_valid: Some(true),
            first_data_us: Some(1),
            outcome: "validated",
            cancelled: false,
            timeout: false,
            eof: false,
            incomplete: false,
        };
        trace.record_http(row.clone());
        assert!(trace.http_snapshot().is_empty());
        trace.enable_http();
        for sequence in 0..300 {
            trace.record_http(HttpRead {
                sequence,
                ..row.clone()
            });
        }
        let rows = trace.http_snapshot();
        assert_eq!(rows.len(), 256);
        assert_eq!(rows[0].sequence, 44);
        assert_eq!(rows[255].sequence, 299);
    }
}
