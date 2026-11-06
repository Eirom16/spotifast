//! Listening policy shared by the legacy adapter and native hosts. No clocks or I/O.
use crate::queue::{Entry, EntryId, RequestId};

/// Preserve Spotifast's minimum: thirty seconds or half a shorter track,
/// always requiring at least one second. Unknown durations use thirty seconds.
pub fn counts_after_ms(duration_ms: Option<u64>) -> u64 {
    duration_ms.map_or(30_000, |ms| (ms / 2).clamp(1_000, 30_000))
}

#[derive(Default)]
pub struct Counter {
    current: Option<Play>,
}
struct Play {
    entry: EntryId,
    request: RequestId,
    threshold_ms: u64,
    elapsed_ms: u64,
    request_ms: u64,
    counted: bool,
}
impl Counter {
    /// Bind each load before starting its adapter. Seeking within an occurrence
    /// keeps the listening total, while the new request starts its frame count at zero.
    pub fn bind(&mut self, entry: &Entry, request: RequestId) {
        if let Some(play) = &mut self.current
            && play.entry == entry.id
        {
            play.request = request;
            play.request_ms = 0;
            return;
        }
        self.current = Some(Play {
            entry: entry.id,
            request,
            threshold_ms: counts_after_ms(entry.media.duration_ms),
            elapsed_ms: 0,
            request_ms: 0,
            counted: false,
        });
    }
    /// `frames` is the cumulative output frames carrying source audio delivered by this
    /// request, excluding pause, buffering silence and seek position offsets.
    /// Returns true exactly once after crossing the threshold.
    pub fn observe(&mut self, request: RequestId, frames: u64, output_rate: u32) -> bool {
        let Some(play) = &mut self.current else {
            return false;
        };
        if play.request != request || output_rate == 0 || play.counted {
            return false;
        }
        let ms =
            (u128::from(frames) * 1000 / u128::from(output_rate)).min(u128::from(u64::MAX)) as u64;
        if ms < play.request_ms {
            return false;
        }
        play.elapsed_ms = play.elapsed_ms.saturating_add(ms - play.request_ms);
        play.request_ms = ms;
        if play.elapsed_ms >= play.threshold_ms {
            play.counted = true;
            return true;
        }
        false
    }
    /// Clearing history must not immediately reinsert the current listen.
    pub fn suppress_current(&mut self) {
        if let Some(play) = &mut self.current {
            play.counted = true;
        }
    }
    pub fn reset(&mut self) {
        self.current = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        media::{Media, MediaId, ProviderId},
        queue::{Command, Effect, Order, Queue},
        requests::SessionId,
    };
    fn loaded(effects: Vec<Effect>) -> (Entry, RequestId) {
        effects
            .into_iter()
            .find_map(|effect| match effect {
                Effect::Load { entry, request, .. } => Some((entry, request)),
                _ => None,
            })
            .unwrap()
    }
    #[test]
    fn delivered_frames_count_once_and_seeking_cannot_fabricate_a_listen() {
        assert_eq!(counts_after_ms(Some(0)), 1_000);
        assert_eq!(counts_after_ms(Some(40_000)), 20_000);
        assert_eq!(counts_after_ms(Some(u64::MAX)), 30_000);
        let media = Media {
            id: MediaId::new(ProviderId::new("test").unwrap(), "music").unwrap(),
            title: "Music".into(),
            duration_ms: Some(240_000),
        };
        let mut queue = Queue::new(SessionId(1));
        let (entry, first) = loaded(
            queue
                .command(Command::StartContext {
                    media: vec![media],
                    start: 0,
                    order: Order::Sequential,
                })
                .unwrap(),
        );
        let mut counter = Counter::default();
        counter.bind(&entry, first);
        assert!(!counter.observe(first, 480_000, 48_000)); // Ten seconds actually delivered.
        assert!(!counter.observe(first, 480_000, 48_000)); // Paused or buffering: no frames.
        let (_, seek) = loaded(queue.command(Command::Seek(200_000)).unwrap());
        counter.bind(&entry, seek);
        assert!(!counter.observe(first, u64::MAX, 48_000)); // Retired callback.
        assert!(!counter.observe(seek, 0, 48_000)); // The 200 s seek offset contributes nothing.
        assert!(!counter.observe(seek, 19 * 48_000, 48_000));
        assert!(counter.observe(seek, 20 * 48_000, 48_000));
        assert!(!counter.observe(seek, 30 * 48_000, 48_000));
        counter.reset();
        counter.bind(&entry, seek); // A new listen after EOF/stop.
        assert!(counter.observe(seek, 30 * 48_000, 48_000));
        counter.reset();
        counter.bind(&entry, seek);
        counter.suppress_current();
        assert!(!counter.observe(seek, u64::MAX, 48_000));
    }
}
