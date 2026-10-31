//! The file decoder owns disk reads/resampling. The callback owns only a PCM consumer.
use super::{CallbackState, EngineError};
use crate::{
    buffer,
    wav_stream::{Resampler, Wav},
};
use std::{
    fs::File,
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
    pub fn start(
        wav: Wav<File>,
        rate: u32,
        position: u64,
        state: &CallbackState,
    ) -> Result<(Self, buffer::Consumer), EngineError> {
        let (mut sender, consumer) = buffer::channel(CAPACITY).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let control = state.control.clone();
        let stats = state.stats.clone();
        let epoch = state.epoch;
        let started = state.started;
        let worker = thread::Builder::new()
            .name("doremi-decode".into())
            .spawn(move || {
                let mut source = match Resampler::new(wav, rate, position) {
                    Ok(source) => source,
                    Err(_) => {
                        sender.fail();
                        return;
                    }
                };
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
                            sender.finish();
                            return;
                        }
                        Err(_) => {
                            sender.fail();
                            return;
                        }
                    };
                    let _ = stats.first_pcm.compare_exchange(
                        0,
                        started.elapsed().as_micros().max(1) as u64,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                    while sender.try_push(frame).is_err() {
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
            return Err(EngineError::Input);
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
