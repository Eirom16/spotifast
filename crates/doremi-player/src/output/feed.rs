//! The file decoder owns disk reads/resampling. The callback owns only a PCM consumer.
use super::{CallbackState, EngineError};
use crate::{
    buffer,
    wav_stream::{FrameReader, LinearResampler},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(super) const CAPACITY: usize = 8192;
pub(super) struct Feed {
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Feed {
    pub fn start<S: FrameReader + Send + 'static>(
        input: S,
        rate: u32,
        position: u64,
        state: &CallbackState,
    ) -> Result<(Self, buffer::Consumer), EngineError> {
        // Initial demux/index seek belongs to opening the source, just like
        // the factory. The PCM prebuffer deadline starts after that seek;
        // lazy MP4 indexing must not consume the entire prebuffer budget.
        let mut source =
            LinearResampler::new(input, rate, position).map_err(EngineError::Decode)?;
        let (mut sender, consumer) = buffer::channel(CAPACITY).unwrap();
        let cancel = state.io_cancel.clone();
        let stop = cancel.clone();
        let control = state.control.clone();
        let stats = state.stats.clone();
        let epoch = state.epoch;
        let started = state.started;
        let worker = thread::Builder::new()
            .name("doremi-decode".into())
            .spawn(move || {
                let mut first_pcm = true;
                let mut produced = 0_u64;
                let mut last_production = Instant::now();
                loop {
                    if stop.load(Ordering::Acquire)
                        || control.epoch.load(Ordering::Acquire) != epoch
                        || sender.is_closed()
                    {
                        return;
                    }
                    let frame = match source.read_frame() {
                        Ok(Some(frame)) => frame,
                        Ok(None) => {
                            stats.produced_frames.store(produced, Ordering::Release);
                            sender.finish();
                            return;
                        }
                        Err(error) => {
                            // Preserve an explicit refusal across the PCM
                            // channel, before publishing its terminal state.
                            stats.input_denied.store(
                                error == crate::DecodeError::AccessDenied,
                                Ordering::Release,
                            );
                            sender.fail();
                            return;
                        }
                    };
                    if first_pcm {
                        stats.first_pcm.store(
                            started.elapsed().as_micros().max(1) as u64,
                            Ordering::Release,
                        );
                        first_pcm = false;
                    }
                    while sender.try_push(frame).is_err() {
                        stats
                            .producer_backpressure_waits
                            .fetch_add(1, Ordering::Relaxed);
                        if stop.load(Ordering::Acquire)
                            || control.epoch.load(Ordering::Acquire) != epoch
                            || sender.is_closed()
                        {
                            return;
                        }
                        if control.playing.load(Ordering::Acquire) {
                            thread::park_timeout(Duration::from_millis(2));
                        } else {
                            thread::park();
                        }
                    }
                    produced += 1;
                    if produced.is_multiple_of(256) {
                        let now = Instant::now();
                        stats.produced_frames.store(produced, Ordering::Relaxed);
                        stats.producer_max_gap_us.fetch_max(
                            now.duration_since(last_production).as_micros() as u64,
                            Ordering::Relaxed,
                        );
                        last_production = now;
                    }
                }
            })
            .map_err(|_| EngineError::Closed)?;
        let feed = Self {
            cancel,
            worker: Some(worker),
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        while consumer.buffered() < CAPACITY / 2 && !consumer.is_finished() {
            if state.control.epoch.load(Ordering::Acquire) != state.epoch {
                return Err(EngineError::Closed);
            }
            if Instant::now() >= deadline {
                return Err(EngineError::Timeout);
            }
            thread::sleep(Duration::from_millis(1));
        }
        if consumer.is_failed() {
            return Err(state.stats.input_error());
        }
        Ok((feed, consumer))
    }
    pub fn wake(&self) {
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }
}
impl Drop for Feed {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.wake();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
