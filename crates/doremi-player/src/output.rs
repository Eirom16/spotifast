//! Native output actor. Only the worker opens/closes devices or decodes PCM.
//! The callback reads a predecoded buffer (at most MAX_SAMPLES floats), atomics
//! and its own cursor. It performs no allocation, I/O, locks or event delivery.
use super::{Builtin, Cursor, DecodeError, decode_wav};
use cpal::{
    FromSample, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use doremi_core::queue::RequestId;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineError {
    Decode(DecodeError),
    NoDevice,
    UnsupportedFormat,
    Output,
    Busy,
    Closed,
}
#[derive(Clone, Copy, Debug)]
pub enum EngineEvent {
    Ready(RequestId),
    Failed(RequestId, EngineError),
    Ended(RequestId),
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    pub position_ms: u64,
    pub frames: u64,
    pub callbacks: u64,
    pub output_rate: u32,
    pub pcm_bytes: u64,
    pub first_pcm_us: Option<u64>,
    pub first_callback_us: Option<u64>,
    pub reported_xruns: u64,
}
#[derive(Default)]
struct Stats {
    position: AtomicU64,
    frames: AtomicU64,
    callbacks: AtomicU64,
    rate: AtomicU32,
    bytes: AtomicU64,
    first_pcm: AtomicU64,
    first_callback: AtomicU64,
    xruns: AtomicU64,
    ended: AtomicBool,
    failed: AtomicBool,
}
struct Control {
    epoch: AtomicU64,
    playing: AtomicBool,
    gain: AtomicU32,
}
struct Load {
    epoch: u64,
    request: RequestId,
    builtin: Builtin,
    position: u64,
    stats: Arc<Stats>,
    started: Instant,
}
enum Command {
    Load(Load),
    Wake,
}
struct Current {
    request: RequestId,
    stats: Arc<Stats>,
}
struct CallbackState {
    control: Arc<Control>,
    stats: Arc<Stats>,
    epoch: u64,
    started: Instant,
}
struct Active {
    stream: cpal::Stream,
    epoch: u64,
    request: RequestId,
    stats: Arc<Stats>,
    running: bool,
    ended_sent: bool,
}

pub struct Engine {
    sender: Option<mpsc::SyncSender<Command>>,
    worker: Option<std::thread::JoinHandle<()>>,
    control: Arc<Control>,
    events: Arc<Mutex<Option<EngineEvent>>>,
    current: Option<Current>,
}
impl Engine {
    pub fn new() -> Result<Self, EngineError> {
        let (sender, receiver) = mpsc::sync_channel(8);
        let control = Arc::new(Control {
            epoch: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            gain: AtomicU32::new(0.7_f32.to_bits()),
        });
        let events = Arc::new(Mutex::new(None));
        let worker_control = control.clone();
        let output = events.clone();
        let worker = std::thread::Builder::new()
            .name("doremi-audio".into())
            .spawn(move || worker(receiver, worker_control, output))
            .map_err(|_| EngineError::Closed)?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
            control,
            events,
            current: None,
        })
    }
    pub fn load(
        &mut self,
        request: RequestId,
        builtin: Builtin,
        position: u64,
    ) -> Result<(), EngineError> {
        self.control.playing.store(false, Ordering::Release);
        let epoch = self
            .control
            .epoch
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| EngineError::Closed)?
            + 1;
        let stats = Arc::new(Stats::default());
        stats.position.store(position, Ordering::Release);
        self.current = Some(Current {
            request,
            stats: stats.clone(),
        });
        let load = Load {
            epoch,
            request,
            builtin,
            position,
            stats,
            started: Instant::now(),
        };
        self.sender
            .as_ref()
            .ok_or(EngineError::Closed)?
            .try_send(Command::Load(load))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => EngineError::Busy,
                mpsc::TrySendError::Disconnected(_) => EngineError::Closed,
            })
    }
    pub fn cancel(&mut self) {
        self.control.playing.store(false, Ordering::Release);
        let _ = self
            .control
            .epoch
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1));
        self.current = None;
        self.wake();
    }
    fn wake(&self) {
        if let Some(sender) = &self.sender {
            let _ = sender.try_send(Command::Wake);
        }
    }
    pub fn set_playing(&self, playing: bool) {
        self.control
            .playing
            .store(playing && self.current.is_some(), Ordering::Release);
        self.wake();
    }
    pub fn set_volume(&self, gain: f32) {
        let gain = if gain.is_finite() {
            gain.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.control.gain.store(gain.to_bits(), Ordering::Release);
    }
    pub fn current_request(&self) -> Option<RequestId> {
        self.current.as_ref().map(|c| c.request)
    }
    pub fn poll(&self) -> Option<EngineEvent> {
        self.events
            .try_lock()
            .ok()
            .and_then(|mut event| event.take())
    }
    pub fn metrics(&self) -> Option<Metrics> {
        let s = &self.current.as_ref()?.stats;
        let optional = |value: u64| (value > 0).then_some(value);
        Some(Metrics {
            position_ms: s.position.load(Ordering::Acquire),
            frames: s.frames.load(Ordering::Acquire),
            callbacks: s.callbacks.load(Ordering::Acquire),
            output_rate: s.rate.load(Ordering::Acquire),
            pcm_bytes: s.bytes.load(Ordering::Acquire),
            first_pcm_us: optional(s.first_pcm.load(Ordering::Acquire)),
            first_callback_us: optional(s.first_callback.load(Ordering::Acquire)),
            reported_xruns: s.xruns.load(Ordering::Acquire),
        })
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        // The app retains this actor across Activity lifetimes, and desktop
        // drops it only after leaving the UI loop. No callback joins threads.
        self.control.playing.store(false, Ordering::Release);
        let _ = self
            .control
            .epoch
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1));
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn publish(events: &Mutex<Option<EngineEvent>>, control: &Control, epoch: u64, event: EngineEvent) {
    if control.epoch.load(Ordering::Acquire) == epoch
        && let Ok(mut slot) = events.lock()
    {
        *slot = Some(event);
    }
}
fn worker(
    receiver: mpsc::Receiver<Command>,
    control: Arc<Control>,
    events: Arc<Mutex<Option<EngineEvent>>>,
) {
    let mut active: Option<Active> = None;
    loop {
        let command = if active.as_ref().is_some_and(|a| a.running) {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(command) => Some(command),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match receiver.recv() {
                Ok(command) => Some(command),
                Err(_) => break,
            }
        };
        let mut pending = None;
        for command in command.into_iter().chain(receiver.try_iter()) {
            if let Command::Load(load) = command {
                pending = Some(load);
            }
        }
        if let Some(load) = pending {
            active.take();
            if load.epoch == control.epoch.load(Ordering::Acquire) {
                let epoch = load.epoch;
                let request = load.request;
                match prepare(load, control.clone()) {
                    Ok(next) if epoch == control.epoch.load(Ordering::Acquire) => {
                        active = Some(next);
                        publish(&events, &control, epoch, EngineEvent::Ready(request));
                    }
                    Ok(_) => {}
                    Err(error) => publish(
                        &events,
                        &control,
                        epoch,
                        EngineEvent::Failed(request, error),
                    ),
                }
            }
        }
        if active
            .as_ref()
            .is_some_and(|a| a.epoch != control.epoch.load(Ordering::Acquire))
        {
            active.take();
        }
        if let Some(current) = active.as_mut() {
            if current.stats.failed.load(Ordering::Acquire) {
                let epoch = current.epoch;
                let request = current.request;
                active.take();
                control.playing.store(false, Ordering::Release);
                publish(
                    &events,
                    &control,
                    epoch,
                    EngineEvent::Failed(request, EngineError::Output),
                );
                continue;
            }
            if current.stats.ended.load(Ordering::Acquire) && !current.ended_sent {
                current.ended_sent = true;
                control.playing.store(false, Ordering::Release);
                publish(
                    &events,
                    &control,
                    current.epoch,
                    EngineEvent::Ended(current.request),
                );
            }
            let playing = control.playing.load(Ordering::Acquire);
            if playing != current.running {
                let result = if playing {
                    current.stream.play()
                } else {
                    current.stream.pause()
                };
                if result.is_err() {
                    current.stats.failed.store(true, Ordering::Release);
                    publish(
                        &events,
                        &control,
                        current.epoch,
                        EngineEvent::Failed(current.request, EngineError::Output),
                    );
                }
                current.running = playing && result.is_ok();
            }
        }
    }
    // The stream and its immutable PCM are destroyed on this worker.
    drop(active);
}
fn device() -> Result<(cpal::Device, cpal::SupportedStreamConfig), EngineError> {
    #[cfg(target_os = "linux")]
    {
        for id in [
            cpal::HostId::PipeWire,
            cpal::HostId::PulseAudio,
            cpal::HostId::Alsa,
        ] {
            if let Ok(host) = cpal::host_from_id(id)
                && let Some(device) = host.default_output_device()
                && let Ok(config) = device.default_output_config()
            {
                return Ok((device, config));
            }
        }
        Err(EngineError::NoDevice)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(EngineError::NoDevice)?;
        let config = device
            .default_output_config()
            .map_err(|_| EngineError::NoDevice)?;
        Ok((device, config))
    }
}
fn prepare(load: Load, control: Arc<Control>) -> Result<Active, EngineError> {
    let pcm = decode_wav(load.builtin.bytes()).map_err(EngineError::Decode)?;
    load.stats.first_pcm.store(
        load.started.elapsed().as_micros().max(1) as u64,
        Ordering::Release,
    );
    load.stats
        .bytes
        .store(pcm.byte_len() as u64, Ordering::Release);
    if load.epoch != control.epoch.load(Ordering::Acquire) {
        return Err(EngineError::Closed);
    }
    let (device, supported) = device()?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    if !(1..=8).contains(&config.channels) {
        return Err(EngineError::UnsupportedFormat);
    }
    let cursor =
        Cursor::new(&pcm, config.sample_rate, load.position).map_err(EngineError::Decode)?;
    load.stats.rate.store(config.sample_rate, Ordering::Release);
    macro_rules! build {
        ($sample:ty) => {
            stream::<$sample>(
                &device,
                config,
                pcm,
                cursor,
                CallbackState {
                    control,
                    stats: load.stats.clone(),
                    epoch: load.epoch,
                    started: load.started,
                },
            )
        };
    }
    let stream = match format {
        cpal::SampleFormat::F32 => build!(f32),
        cpal::SampleFormat::I16 => build!(i16),
        cpal::SampleFormat::U16 => build!(u16),
        cpal::SampleFormat::I32 => build!(i32),
        cpal::SampleFormat::I24 => build!(cpal::I24),
        cpal::SampleFormat::F64 => build!(f64),
        _ => return Err(EngineError::UnsupportedFormat),
    }?;
    Ok(Active {
        stream,
        epoch: load.epoch,
        request: load.request,
        stats: load.stats,
        running: false,
        ended_sent: false,
    })
}
fn stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    pcm: super::Pcm,
    mut cursor: Cursor,
    state: CallbackState,
) -> Result<cpal::Stream, EngineError> {
    let CallbackState {
        control,
        stats,
        epoch,
        started,
    } = state;
    let channels = usize::from(config.channels);
    let errors = stats.clone();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                if control.epoch.load(Ordering::Acquire) != epoch
                    || !control.playing.load(Ordering::Acquire)
                {
                    data.fill(T::from_sample(0.0));
                    return;
                }
                let gain = f32::from_bits(control.gain.load(Ordering::Acquire));
                let mut delivered = 0_u64;
                for frame in data.chunks_mut(channels) {
                    if let Some(sample) = cursor.next_frame(&pcm, gain) {
                        for (channel, out) in frame.iter_mut().enumerate() {
                            let value = if channels == 1 {
                                (sample[0] + sample[1]) * 0.5
                            } else {
                                match channel {
                                    0 => sample[0],
                                    1 => sample[1],
                                    _ => 0.0,
                                }
                            };
                            *out = T::from_sample(value);
                        }
                        delivered += 1;
                    } else {
                        frame.fill(T::from_sample(0.0));
                        stats.ended.store(true, Ordering::Release);
                    }
                }
                if delivered > 0 {
                    let _ = stats.first_callback.compare_exchange(
                        0,
                        started.elapsed().as_micros().max(1) as u64,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                }
                stats.frames.fetch_add(delivered, Ordering::Relaxed);
                stats.callbacks.fetch_add(1, Ordering::Relaxed);
                stats
                    .position
                    .store(cursor.position_ms(&pcm), Ordering::Release);
            },
            move |error: cpal::Error| match error.kind() {
                cpal::ErrorKind::Xrun => {
                    errors.xruns.fetch_add(1, Ordering::Relaxed);
                }
                cpal::ErrorKind::RealtimeDenied => {}
                _ => errors.failed.store(true, Ordering::Release),
            },
            None,
        )
        .map_err(|_| EngineError::Output)
}
