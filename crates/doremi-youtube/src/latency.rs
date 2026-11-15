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
