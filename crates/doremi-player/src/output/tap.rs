//! Bounded adaptation of Spotifast's post-EQ, pre-volume AudioTap (MIT).
use std::{
    sync::atomic::{AtomicU32, AtomicU64, Ordering, fence},
    time::Instant,
};

const CAPACITY: usize = 2048;

pub(super) struct AudioTap {
    created: Instant,
    expires_ms: AtomicU64,
    version: AtomicU32,
    end: AtomicU32,
    count: AtomicU32,
    samples: [AtomicU32; CAPACITY],
}

impl Default for AudioTap {
    fn default() -> Self {
        Self {
            created: Instant::now(),
            expires_ms: AtomicU64::new(0),
            version: AtomicU32::new(0),
            end: AtomicU32::new(0),
            count: AtomicU32::new(0),
            samples: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}

impl AudioTap {
    fn now_ms(&self) -> u64 {
        self.created.elapsed().as_millis() as u64
    }

    pub fn window(&self, target: &mut [f32]) -> bool {
        // Stop capture even if the Android surface or compositor never
        // schedules another UI frame once it goes away.
        self.expires_ms
            .store(self.now_ms() + 175, Ordering::Release);
        let before = self.version.load(Ordering::Acquire);
        if before % 2 != 0 || (self.count.load(Ordering::Relaxed) as usize) < target.len() {
            return false;
        }
        let end = self.end.load(Ordering::Relaxed);
        let start = end.wrapping_sub(target.len() as u32);
        for (i, value) in target.iter_mut().enumerate() {
            let index = start.wrapping_add(i as u32) as usize % CAPACITY;
            *value = f32::from_bits(self.samples[index].load(Ordering::Relaxed));
        }
        fence(Ordering::Acquire);
        self.version.load(Ordering::Relaxed) == before
    }

    pub fn stop(&self) {
        self.expires_ms.store(0, Ordering::Release);
    }

    #[cfg(test)]
    fn capture(&self, frames: &[[f32; 2]]) {
        if let Some(mut block) = self.block() {
            for &frame in frames {
                block.push(frame);
            }
        }
    }

    pub fn block(&self) -> Option<Block<'_>> {
        let until = self.expires_ms.load(Ordering::Acquire);
        if until == 0 || self.now_ms() >= until {
            if self.count.load(Ordering::Relaxed) != 0 {
                self.version.fetch_add(1, Ordering::AcqRel);
                self.count.store(0, Ordering::Relaxed);
                self.version.fetch_add(1, Ordering::Release);
            }
            return None;
        }
        self.version.fetch_add(1, Ordering::AcqRel);
        Some(Block {
            tap: self,
            end: self.end.load(Ordering::Relaxed),
            count: self.count.load(Ordering::Relaxed),
        })
    }
}

pub(super) struct Block<'a> {
    tap: &'a AudioTap,
    end: u32,
    count: u32,
}

impl Block<'_> {
    pub fn push(&mut self, stereo: [f32; 2]) {
        let mono = (stereo[0] + stereo[1]) * 0.5;
        self.tap.samples[self.end as usize % CAPACITY].store(mono.to_bits(), Ordering::Relaxed);
        self.end = self.end.wrapping_add(1);
        self.count = (self.count + 1).min(CAPACITY as u32);
    }
}

impl Drop for Block<'_> {
    fn drop(&mut self) {
        self.tap.end.store(self.end, Ordering::Relaxed);
        self.tap.count.store(self.count, Ordering::Relaxed);
        self.tap.version.fetch_add(1, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_is_demand_driven_and_reads_recent_mono_pcm() {
        let tap = AudioTap::default();
        let mut target = [0.0; 4];
        assert!(tap.block().is_none());
        assert!(!tap.window(&mut target));
        for i in 0..CAPACITY + 1 {
            tap.capture(&[[i as f32 / CAPACITY as f32, 0.0]]);
        }
        assert!(tap.window(&mut target));
        assert_eq!(target[3], 0.5);
        assert!(target.windows(2).all(|pair| pair[0] < pair[1]));
        tap.stop();
        assert!(tap.block().is_none());
        assert!(!tap.window(&mut target));
    }
}
