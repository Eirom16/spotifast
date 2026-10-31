//! Bounded single-producer/single-consumer stereo PCM, using only atomics.
//! Endpoints are not clonable; the callback owns its consumer exclusively.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
};

struct Frame([AtomicU32; 2]);
struct Shared {
    frames: Box<[Frame]>,
    read: AtomicU64,
    written: AtomicU64,
    end: AtomicU8,
    closed: AtomicBool,
}
pub struct Producer {
    shared: Arc<Shared>,
    written: u64,
    finished: bool,
}
pub struct Consumer {
    shared: Arc<Shared>,
    read: u64,
}
#[derive(Debug, PartialEq)]
pub enum Read {
    Frame([f32; 2]),
    Pending,
    End,
    Failed,
}
/// Allocate once before playback. No endpoint operation allocates or locks.
pub fn channel(capacity: usize) -> Option<(Producer, Consumer)> {
    if !(2..=65536).contains(&capacity) || !capacity.is_power_of_two() {
        return None;
    }
    let shared = Arc::new(Shared {
        frames: (0..capacity)
            .map(|_| Frame([AtomicU32::new(0), AtomicU32::new(0)]))
            .collect(),
        read: AtomicU64::new(0),
        written: AtomicU64::new(0),
        end: AtomicU8::new(0),
        closed: AtomicBool::new(false),
    });
    Some((
        Producer {
            shared: shared.clone(),
            written: 0,
            finished: false,
        },
        Consumer { shared, read: 0 },
    ))
}
impl Producer {
    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }
    pub fn buffered(&self) -> usize {
        self.written
            .wrapping_sub(self.shared.read.load(Ordering::Acquire)) as usize
    }
    pub fn try_push(&mut self, frame: [f32; 2]) -> Result<(), [f32; 2]> {
        if self.is_closed() || self.buffered() == self.shared.frames.len() {
            return Err(frame);
        }
        let slot = &self.shared.frames[self.written as usize & (self.shared.frames.len() - 1)];
        slot.0[0].store(frame[0].to_bits(), Ordering::Relaxed);
        slot.0[1].store(frame[1].to_bits(), Ordering::Relaxed);
        self.written = self.written.wrapping_add(1);
        self.shared.written.store(self.written, Ordering::Release);
        Ok(())
    }
    pub fn finish(mut self) {
        self.finished = true;
        self.shared.end.store(1, Ordering::Release);
    }
    pub fn fail(mut self) {
        self.finished = true;
        self.shared.end.store(2, Ordering::Release);
    }
}
impl Drop for Producer {
    fn drop(&mut self) {
        if !self.finished {
            self.shared.end.store(2, Ordering::Release);
        }
    }
}
impl Consumer {
    pub fn capacity(&self) -> usize {
        self.shared.frames.len()
    }
    pub fn buffered(&self) -> usize {
        self.shared
            .written
            .load(Ordering::Acquire)
            .wrapping_sub(self.read) as usize
    }
    pub fn read(&mut self) -> Read {
        let end = self.shared.end.load(Ordering::Acquire);
        if end == 2 {
            return Read::Failed;
        }
        if self.read == self.shared.written.load(Ordering::Acquire) {
            return if end == 1 { Read::End } else { Read::Pending };
        }
        let slot = &self.shared.frames[self.read as usize & (self.shared.frames.len() - 1)];
        let frame = [
            f32::from_bits(slot.0[0].load(Ordering::Relaxed)),
            f32::from_bits(slot.0[1].load(Ordering::Relaxed)),
        ];
        self.read = self.read.wrapping_add(1);
        self.shared.read.store(self.read, Ordering::Release);
        Read::Frame(frame)
    }
}
impl Drop for Consumer {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_is_not_eof_and_full_never_overwrites_unread_pcm() {
        let (mut tx, mut rx) = channel(2).unwrap();
        assert_eq!(rx.read(), Read::Pending);
        tx.try_push([1.0, 2.0]).unwrap();
        tx.try_push([3.0, 4.0]).unwrap();
        assert!(tx.try_push([5.0, 6.0]).is_err());
        tx.finish();
        assert_eq!(rx.read(), Read::Frame([1.0, 2.0]));
        assert_eq!(rx.read(), Read::Frame([3.0, 4.0]));
        assert_eq!(rx.read(), Read::End);
    }
    #[test]
    fn failure_and_endpoint_drop_do_not_leave_an_infinite_pending_consumer() {
        let (tx, mut rx) = channel(4).unwrap();
        tx.fail();
        assert_eq!(rx.read(), Read::Failed);
        let (tx, mut rx) = channel(4).unwrap();
        drop(tx);
        assert_eq!(rx.read(), Read::Failed);
        let (tx, rx) = channel(4).unwrap();
        drop(rx);
        assert!(tx.is_closed());
        assert!(channel(3).is_none());
    }
    #[test]
    fn concurrent_wraparound_preserves_every_frame_in_order() {
        let (mut tx, mut rx) = channel(64).unwrap();
        let start = u64::MAX - 31;
        tx.written = start;
        rx.read = start;
        tx.shared.read.store(start, Ordering::Relaxed);
        tx.shared.written.store(start, Ordering::Relaxed);
        let producer = std::thread::spawn(move || {
            for i in 0..50000 {
                while tx.try_push([i as f32, -(i as f32)]).is_err() {
                    std::thread::yield_now();
                }
            }
            tx.finish();
        });
        for i in 0..50000 {
            loop {
                match rx.read() {
                    Read::Frame(frame) => {
                        assert_eq!(frame, [i as f32, -(i as f32)]);
                        break;
                    }
                    Read::Pending => std::thread::yield_now(),
                    other => panic!("Unexpected premature state: {other:?}"),
                }
            }
        }
        producer.join().unwrap();
        assert_eq!(rx.read(), Read::End);
    }
}
