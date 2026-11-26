//! Spectrum and waveform adapted from Spotifast src/vis.rs (Carmine Paolino, MIT).
use std::time::{Duration, Instant};

const FFT_SAMPLES: usize = 512;
const SPECTRUM_BINS: usize = FFT_SAMPLES / 2;
pub const SCOPE_SAMPLES: usize = 576;
const COLUMNS: usize = 75;
const MAX_HEIGHT: f32 = 15.0;
const FALLOFF: f32 = 12.0 / 16.0;
const STEP: Duration = Duration::from_micros(16_667);
const CHANNEL_SUM: f32 = 2.0;
const SPEC_SCALE: f32 = 0.5;

/// The classic analyser's FFT, as Winamp's own source has it: 512
/// samples under a Hann window, 256 magnitudes out, each halved. The
/// player bar's analyser runs the same transform over more samples.
struct Fft {
    bit_reversed: Vec<usize>,
    envelope: Vec<f32>,
    twiddles: Vec<(f32, f32)>,
    real: Vec<f32>,
    imaginary: Vec<f32>,
}

impl Fft {
    fn new(n: usize) -> Self {
        let mut bit_reversed: Vec<usize> = (0..n).collect();
        let mut j = 0;
        for i in 0..n {
            if j > i {
                bit_reversed.swap(i, j);
            }
            let mut m = n >> 1;
            while m >= 1 && j >= m {
                j -= m;
                m >>= 1;
            }
            j += m;
        }
        let envelope = (0..n)
            .map(|i| {
                let phase = i as f32 / n as f32 * std::f32::consts::TAU;
                0.5 + 0.5 * (phase - std::f32::consts::FRAC_PI_2).sin()
            })
            .collect();
        let mut twiddles = Vec::new();
        let mut size = 2;
        while size <= n {
            let theta = -std::f32::consts::TAU / size as f32;
            twiddles.push((theta.cos(), theta.sin()));
            size <<= 1;
        }
        Self {
            bit_reversed,
            envelope,
            twiddles,
            real: vec![0.0; n],
            imaginary: vec![0.0; n],
        }
    }

    fn spectrum(&mut self, wave: &[f32], out: &mut [f32]) {
        let n = self.real.len();
        for i in 0..n {
            let from = self.bit_reversed[i];
            self.real[i] = wave.get(from).copied().unwrap_or(0.0) * self.envelope[from];
            self.imaginary[i] = 0.0;
        }
        let mut size = 2;
        let mut stage = 0;
        while size <= n {
            let (wpr, wpi) = self.twiddles[stage];
            let (mut wr, mut wi) = (1.0f32, 0.0f32);
            let half = size >> 1;
            for m in 0..half {
                let mut i = m;
                while i < n {
                    let j = i + half;
                    let tr = wr * self.real[j] - wi * self.imaginary[j];
                    let ti = wr * self.imaginary[j] + wi * self.real[j];
                    self.real[j] = self.real[i] - tr;
                    self.imaginary[j] = self.imaginary[i] - ti;
                    self.real[i] += tr;
                    self.imaginary[i] += ti;
                    i += size;
                }
                let previous = wr;
                wr = wr * wpr - wi * wpi;
                wi = wi * wpr + previous * wpi;
            }
            size <<= 1;
            stage += 1;
        }
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = (self.real[i] * self.real[i] + self.imaginary[i] * self.imaginary[i]).sqrt()
                * SPEC_SCALE;
        }
    }
}

/// Winamp's bands over a 256-bin spectrum; with fractional bin edges.
fn bands(spectrum: &[f32; SPECTRUM_BINS]) -> [f32; COLUMNS + 1] {
    let bla = 255.0 / 2f32.powf(75.0 / 12.0);
    let warp = |x: f32| (2f32.powf(x / 12.0) - 1.0) * bla;
    let sample = |index: usize| spectrum.get(index).copied().unwrap_or(0.0);
    let hermite = |x: f32, y0: f32, y1: f32, y2: f32, y3: f32| {
        let c1 = 0.5 * (y2 - y0);
        let c3 = 1.5 * (y1 - y2) + 0.5 * (y3 - y0);
        let c2 = y0 - y1 + c1 - c3;
        ((c3 * x + c2) * x + c1) * x + y1
    };
    let mut columns = [0.0; COLUMNS + 1];
    let mut next = warp(0.0) + 1.0;
    for (x, column) in columns.iter_mut().take(COLUMNS).enumerate() {
        let low = next;
        next = warp(x as f32 + 1.0) + 1.0;
        let mut value = 0.0f32;
        let mut bin = low.floor() as usize;
        let end = (next.floor() as usize).min(SPECTRUM_BINS - 1);
        let mut fraction = low;
        let mut mult = (bin as f32 + 1.0) - low;
        let mut herm = true;
        loop {
            if bin == end {
                mult = next - fraction;
                herm = true;
            }
            if herm {
                value += hermite(
                    fraction - bin as f32,
                    sample(bin.saturating_sub(1)),
                    sample(bin),
                    sample(bin + 1),
                    sample(bin + 2),
                ) * mult;
            } else {
                value += sample(bin);
            }
            herm = false;
            bin += 1;
            if bin > end {
                break;
            }
            fraction = bin as f32;
        }
        *column = value.min(255.0);
    }
    columns
}

/// Samples in one spectrum of the player bar's analyser: Winamp's own.
/// The player bar's bands: every one of Winamp's semitone columns as a bar
/// of its own, where the skin groups them four to a bar.
pub const WIDE_BANDS: usize = COLUMNS;
/// How fast a bar falls, in heights per second: Winamp's twelve
/// sixteenths of a row each sixtieth of a second, over fifteen rows.
const WIDE_FALL: f32 = FALLOFF / MAX_HEIGHT / 0.016_667;
/// How long a peak cap hangs above its band before it drops, and how fast
/// it then falls, in heights per second squared.
const PEAK_HOLD: f32 = 0.35;
const PEAK_GRAVITY: f32 = 2.4;

/// The player bar's spectrum analyser: Winamp's classic analyser, with
/// each of its seventy-five columns as a bar, heights from 0 to 1 instead
/// of whole rows, and falls timed by the clock, so they move smoothly at
/// any frame rate.
pub struct WideAnalyser {
    fft: Fft,
    wave: [f32; FFT_SAMPLES],
    spectrum: [f32; SPECTRUM_BINS],
    levels: [f32; WIDE_BANDS],
    /// Each band's peak cap, how long it has hung there, and how fast it
    /// is falling.
    peaks: [f32; WIDE_BANDS],
    held: [f32; WIDE_BANDS],
    speed: [f32; WIDE_BANDS],
    last: Option<Instant>,
}

impl Default for WideAnalyser {
    fn default() -> Self {
        Self {
            fft: Fft::new(FFT_SAMPLES),
            wave: [0.0; FFT_SAMPLES],
            spectrum: [0.0; SPECTRUM_BINS],
            levels: [0.0; WIDE_BANDS],
            peaks: [0.0; WIDE_BANDS],
            held: [0.0; WIDE_BANDS],
            speed: [0.0; WIDE_BANDS],
            last: None,
        }
    }
}

impl WideAnalyser {
    /// Moves the bars towards the spectrum of `samples` (mono, -1 to 1,
    /// 512 of them): up at once, as Winamp's, and down at its
    /// rate for the time since the last step.
    pub fn step(&mut self, samples: &[f32], now: Instant) -> [f32; WIDE_BANDS] {
        let elapsed = self
            .last
            .map_or(STEP, |last| now.saturating_duration_since(last))
            .min(Duration::from_millis(250))
            .as_secs_f32();
        self.last = Some(now);
        self.wave.fill(0.0);
        for (slot, sample) in self.wave.iter_mut().zip(samples) {
            *slot = sample * CHANNEL_SUM;
        }
        self.fft.spectrum(&self.wave, &mut self.spectrum);
        let columns = bands(&self.spectrum);
        for (level, column) in self.levels.iter_mut().zip(columns) {
            let target = column.min(MAX_HEIGHT) / MAX_HEIGHT;
            *level = (*level - WIDE_FALL * elapsed).max(target);
            // Too low to see: at rest.
            if *level < 0.01 {
                *level = 0.0;
            }
        }
        for band in 0..WIDE_BANDS {
            let level = self.levels[band];
            if level >= self.peaks[band] {
                self.peaks[band] = level;
                self.held[band] = 0.0;
                self.speed[band] = 0.0;
            } else if self.held[band] < PEAK_HOLD {
                self.held[band] += elapsed;
            } else {
                self.speed[band] += PEAK_GRAVITY * elapsed;
                self.peaks[band] = (self.peaks[band] - self.speed[band] * elapsed).max(level);
            }
            if self.peaks[band] < 0.01 {
                self.peaks[band] = 0.0;
            }
        }
        self.levels
    }

    /// Where each band's peak cap hangs, from 0 to 1.
    pub fn peaks(&self) -> [f32; WIDE_BANDS] {
        self.peaks
    }
}

/// The player bar's waveform: Winamp's scope, every seventh sample with
/// nothing smoothed or lined up, so it trembles as Winamp's does, for
/// `points` columns instead of seventy-five. Each value runs from -1 to 1
/// across the bar's height; like Winamp's, whose sixteen rows span half of
/// full scale, anything louder than half full scale reaches the edge.
pub fn scope_line(samples: &[f32]) -> [f32; COLUMNS] {
    std::array::from_fn(|point| {
        let sample = samples.get(point * SCOPE_STEP).copied().unwrap_or(0.0);
        (sample * 2.0).clamp(-1.0, 1.0)
    })
}

/// Winamp's scope reads every seventh sample.
pub const SCOPE_STEP: usize = 7;

use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
pub(super) enum Mode {
    #[default]
    Off,
    Spectrum,
    Scope,
}

impl DoremiApp {
    pub(super) fn visualizer(&mut self, ui: &mut egui::Ui) {
        let model = self.model.as_ref().unwrap();
        let mode = model.prefs.visualizer;
        if mode == Mode::Off || !model.real_audio() {
            return;
        }
        let palette = doremi_ui::palette(model.prefs.light, model.prefs.green);
        let title = match mode {
            Mode::Spectrum => "Espectro",
            Mode::Scope => "Osciloscopio",
            Mode::Off => unreachable!(),
        };
        ui.label(title);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().min(520.0), 64.0),
            egui::Sense::hover(),
        );
        self.record("audio-visualizer", &response);
        if !ui.is_rect_visible(rect)
            || ui.ctx().input(|input| {
                input.viewport().minimized == Some(true) || input.viewport().occluded == Some(true)
            })
        {
            return;
        }
        let model = self.model.as_ref().unwrap();
        let mut samples = [0.0; SCOPE_SAMPLES];
        let running = model.queue.playback() == Playback::Playing;
        let ready = running
            && model
                .audio
                .as_ref()
                .is_some_and(|engine| engine.visual_samples(&mut samples));
        if !ready {
            samples.fill(0.0);
        } else if self.internal_ui {
            self.visual_rms = Some(
                (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32)
                    .sqrt(),
            );
        }
        if running {
            let frame_ms = if cfg!(target_os = "android") { 50 } else { 33 };
            ui.ctx()
                .request_repaint_after(Duration::from_millis(frame_ms));
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6, palette.surface);
        let area = rect.shrink(4.0);
        if mode == Mode::Scope {
            let wave = scope_line(&samples);
            let width = area.width() / (wave.len() - 1) as f32;
            let mut last = egui::pos2(area.left(), area.center().y - wave[0] * area.height() * 0.5);
            for (i, sample) in wave.into_iter().enumerate().skip(1) {
                let point = egui::pos2(
                    area.left() + i as f32 * width,
                    area.center().y - sample * area.height() * 0.5,
                );
                painter.line_segment([last, point], egui::Stroke::new(1.2, palette.accent));
                last = point;
            }
        } else if ready {
            let analyser = self.spectrum.get_or_insert_with(WideAnalyser::default);
            let levels = analyser.step(&samples, Instant::now());
            let width = area.width() / WIDE_BANDS as f32;
            for (i, (level, peak)) in levels.into_iter().zip(analyser.peaks()).enumerate() {
                let x = area.left() + i as f32 * width;
                if level > 0.0 {
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(x, area.bottom() - level * area.height()),
                            egui::pos2(x + (width - 1.0).max(1.0), area.bottom()),
                        ),
                        0,
                        palette.accent,
                    );
                }
                if peak > 0.0 {
                    let y = area.bottom() - peak * area.height();
                    painter.line_segment(
                        [egui::pos2(x, y), egui::pos2(x + (width - 1.0).max(1.0), y)],
                        egui::Stroke::new(1.0, palette.secondary),
                    );
                }
            }
        } else {
            self.spectrum = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectrum_and_scope_follow_the_input() {
        let mut analyser = WideAnalyser::default();
        let now = Instant::now();
        assert_eq!(analyser.step(&[0.0; FFT_SAMPLES], now), [0.0; WIDE_BANDS]);
        let peak = |bin| {
            let wave: [f32; SCOPE_SAMPLES] = std::array::from_fn(|i| {
                (std::f32::consts::TAU * i as f32 * bin as f32 / FFT_SAMPLES as f32).sin() * 0.2
            });
            assert!(scope_line(&wave).into_iter().any(|point| point > 0.2));
            WideAnalyser::default()
                .step(&wave, now)
                .into_iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| left.total_cmp(right))
                .unwrap()
                .0
        };
        assert!(peak(7) < peak(60));
    }
}
