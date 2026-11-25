//! Adapted from Spotifast src/eq.rs, copyright Carmine Paolino (MIT).

/// The centre frequencies, Winamp's, in hertz.
pub const BANDS: [f32; 10] = [
    60.0, 170.0, 310.0, 600.0, 1000.0, 3000.0, 6000.0, 12000.0, 14000.0, 16000.0,
];
/// How far a band goes either way, in decibels.
pub const RANGE_DB: f32 = 12.0;
/// Bands closer to flat than this are skipped rather than run for nothing.
const FLAT: f32 = 0.05;
/// How far the solved gains may go past the sliders while making the
/// combined response meet them; a guard, not a target.
const SOLVED_LIMIT: f64 = 36.0;

/// Persisted settings shared by desktop and Android.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct EqSettings {
    pub on: bool,
    /// Twelve decibels either way, like the bands.
    pub preamp_db: f32,
    pub bands_db: [f32; 10],
}

impl EqSettings {
    pub fn clamped(mut self) -> Self {
        self.preamp_db = valid_db(self.preamp_db);
        self.bands_db = self.bands_db.map(valid_db);
        self
    }
}

fn valid_db(db: f32) -> f32 {
    if db.is_finite() {
        db.clamp(-RANGE_DB, RANGE_DB)
    } else {
        0.0
    }
}

/// Band widths in octaves, based on the midpoint between adjacent bands.
///
/// The bands are unevenly spaced. A shared width makes the high bands overlap
/// and overboost presets such as Full Treble. The outer bands extend three
/// octaves below 60 Hz and one octave above 16 kHz, matching Winamp.
fn band_widths() -> [f64; 10] {
    let octaves = BANDS.map(|hz| f64::from(hz).log2());
    let mut widths = [0.0; 10];
    for (index, width) in widths.iter_mut().enumerate() {
        let below = index
            .checked_sub(1)
            .map_or(octaves[index] - 3.0, |i| octaves[i]);
        let above = octaves
            .get(index + 1)
            .copied()
            .unwrap_or(octaves[index] + 1.0);
        *width = (octaves[index] - below) / 2.0 + (above - octaves[index]) / 2.0;
    }
    widths
}

/// Builds filters whose combined response matches each slider at its centre.
/// Adjacent filters overlap, so their individual gains are solved together.
fn chain(bands_db: &[f32; 10], rate: f64) -> Vec<Biquad> {
    let widths = band_widths();
    // Keep the cookbook bandwidth term away from its Nyquist singularity.
    let count = BANDS.partition_point(|hz| f64::from(*hz) < rate * 0.46);
    let mut target = bands_db.map(f64::from);
    target[count..].fill(0.0);
    if target.iter().all(|gain| gain.abs() <= f64::from(FLAT)) {
        return Vec::new();
    }
    // How much each band moves every centre, per decibel it is given.
    let mut unit = [[0.0; 10]; 10];
    for (i, row) in unit.iter_mut().enumerate().skip(count) {
        row[i] = 1.0;
    }
    for (j, (hz, width)) in BANDS.iter().zip(widths).take(count).enumerate() {
        let filter = Biquad::peaking(f64::from(*hz), width, 1.0, rate);
        for (i, centre) in BANDS.iter().take(count).enumerate() {
            unit[i][j] = filter.gain_db_at(f64::from(*centre), rate);
        }
    }
    let mut gains = target;
    for _ in 0..6 {
        let filters: Vec<(usize, Biquad)> = filters_for(&gains, &widths, rate);
        let mut residual = [0.0; 10];
        for (i, centre) in BANDS.iter().take(count).enumerate() {
            let played: f64 = filters
                .iter()
                .map(|(_, filter)| filter.gain_db_at(f64::from(*centre), rate))
                .sum();
            residual[i] = played - target[i];
        }
        if residual.iter().all(|r| r.abs() < 0.01) {
            break;
        }
        let correction = solve(unit, residual);
        for (gain, step) in gains.iter_mut().zip(correction) {
            *gain = (*gain - step).clamp(-SOLVED_LIMIT, SOLVED_LIMIT);
        }
    }
    filters_for(&gains, &widths, rate)
        .into_iter()
        .map(|(_, filter)| filter)
        .collect()
}

/// The bands worth running, with the filter for each.
fn filters_for(gains: &[f64; 10], widths: &[f64; 10], rate: f64) -> Vec<(usize, Biquad)> {
    BANDS
        .iter()
        .zip(widths)
        .zip(gains)
        .enumerate()
        .filter(|(_, (_, gain))| gain.abs() > f64::from(FLAT))
        .map(|(index, ((hz, width), gain))| {
            (index, Biquad::peaking(f64::from(*hz), *width, *gain, rate))
        })
        .collect()
}

/// Gaussian elimination with partial pivoting, for the ten-by-ten
/// interaction of the bands.
fn solve(mut matrix: [[f64; 10]; 10], mut rhs: [f64; 10]) -> [f64; 10] {
    let n = 10;
    for column in 0..n {
        let pivot = (column..n)
            .max_by(|a, b| {
                matrix[*a][column]
                    .abs()
                    .total_cmp(&matrix[*b][column].abs())
            })
            .unwrap_or(column);
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        let lead = matrix[column][column];
        if lead.abs() < 1e-12 {
            continue;
        }
        let pivot_row = matrix[column];
        let pivot_rhs = rhs[column];
        for row in column + 1..n {
            let factor = matrix[row][column] / lead;
            if factor == 0.0 {
                continue;
            }
            for (cell, above) in matrix[row][column..].iter_mut().zip(&pivot_row[column..]) {
                *cell -= factor * above;
            }
            rhs[row] -= factor * pivot_rhs;
        }
    }
    let mut solution = [0.0; 10];
    for row in (0..n).rev() {
        let mut sum = rhs[row];
        for k in row + 1..n {
            sum -= matrix[row][k] * solution[k];
        }
        let lead = matrix[row][row];
        solution[row] = if lead.abs() < 1e-12 { 0.0 } else { sum / lead };
    }
    solution
}

/// A named set of band gains, as Winamp shipped them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    pub bands_db: [f32; 10],
}

/// Winamp presets followed by additional Spotifast presets.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Flat",
        bands_db: [0.0; 10],
    },
    Preset {
        name: "Classical",
        bands_db: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -7.2, -7.2, -7.2, -9.6],
    },
    Preset {
        name: "Club",
        bands_db: [0.0, 0.0, 8.0, 5.6, 5.6, 5.6, 3.2, 0.0, 0.0, 0.0],
    },
    Preset {
        name: "Dance",
        bands_db: [9.6, 7.2, 2.4, 0.0, 0.0, -5.6, -7.2, -7.2, 0.0, 0.0],
    },
    Preset {
        name: "Full Bass",
        bands_db: [-8.0, 9.6, 9.6, 5.6, 1.6, -4.0, -8.0, -10.4, -11.2, -11.2],
    },
    Preset {
        name: "Full Bass & Treble",
        bands_db: [7.2, 5.6, 0.0, -7.2, -4.8, 1.6, 8.0, 11.2, 12.0, 12.0],
    },
    Preset {
        name: "Full Treble",
        bands_db: [-9.6, -9.6, -9.6, -4.0, 2.4, 11.2, 12.0, 12.0, 12.0, 12.0],
    },
    Preset {
        name: "Laptop Speakers / Headphones",
        bands_db: [4.8, 11.2, 5.6, -3.2, -2.4, 1.6, 4.8, 9.6, 12.0, 12.0],
    },
    Preset {
        name: "Large Hall",
        bands_db: [10.4, 10.4, 5.6, 5.6, 0.0, -4.8, -4.8, -4.8, 0.0, 0.0],
    },
    Preset {
        name: "Live",
        bands_db: [-4.8, 0.0, 4.0, 5.6, 5.6, 5.6, 4.0, 2.4, 2.4, 2.4],
    },
    Preset {
        name: "Party",
        bands_db: [7.2, 7.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 7.2, 7.2],
    },
    Preset {
        name: "Pop",
        bands_db: [-1.6, 4.8, 7.2, 8.0, 5.6, 0.0, -2.4, -2.4, -1.6, -1.6],
    },
    Preset {
        name: "Reggae",
        bands_db: [0.0, 0.0, 0.0, -5.6, 0.0, 6.4, 6.4, 0.0, 0.0, 0.0],
    },
    Preset {
        name: "Rock",
        bands_db: [8.0, 4.8, -5.6, -8.0, -3.2, 4.0, 8.8, 11.2, 11.2, 11.2],
    },
    Preset {
        name: "Ska",
        bands_db: [-2.4, -4.8, -4.0, 0.0, 4.0, 5.6, 8.8, 9.6, 11.2, 9.6],
    },
    Preset {
        name: "Soft",
        bands_db: [4.8, 1.6, 0.0, -2.4, 0.0, 4.0, 8.0, 9.6, 11.2, 12.0],
    },
    Preset {
        name: "Soft Rock",
        bands_db: [4.0, 4.0, 2.4, 0.0, -4.0, -5.6, -3.2, 0.0, 2.4, 8.8],
    },
    Preset {
        name: "Techno",
        bands_db: [8.0, 5.6, 0.0, -5.6, -4.8, 0.0, 8.0, 9.6, 9.6, 8.8],
    },
    Preset {
        name: "Bass Booster",
        bands_db: [8.8, 7.2, 5.6, 3.2, 0.8, 0.0, 0.0, 0.0, 0.0, 0.0],
    },
    Preset {
        name: "Bass Reducer",
        bands_db: [-8.8, -7.2, -5.6, -3.2, -0.8, 0.0, 0.0, 0.0, 0.0, 0.0],
    },
    Preset {
        name: "Treble Booster",
        bands_db: [0.0, 0.0, 0.0, 0.0, 0.0, 0.8, 3.2, 5.6, 7.2, 8.8],
    },
    Preset {
        name: "Vocal Booster",
        bands_db: [-2.4, -4.8, -4.8, 1.6, 5.6, 5.6, 4.0, 1.6, 0.0, -2.4],
    },
    Preset {
        name: "Small Speakers",
        bands_db: [-8.0, -6.4, -4.0, -1.6, 1.6, 3.2, 4.8, 5.6, 5.6, 5.6],
    },
    Preset {
        name: "Spoken Word",
        bands_db: [-3.2, -0.8, 0.0, 0.8, 4.0, 5.6, 4.8, 2.4, 0.8, 0.0],
    },
    Preset {
        name: "Loudness",
        bands_db: [9.6, 6.4, 0.0, 0.0, -2.4, 0.0, -1.6, 0.0, 8.0, 1.6],
    },
    Preset {
        name: "Night Listening",
        bands_db: [-4.8, -3.2, -1.6, 0.8, 2.4, 3.2, 2.4, 0.8, -1.6, -3.2],
    },
];

/// A second-order section in direct form I, one per band and channel.
#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    /// A peaking filter after the Audio EQ Cookbook, `width` octaves wide
    /// between its half-gain points and normalised by `a0`. The width is
    /// taken the cookbook's digital way, with its `w0 / sin(w0)` term: near
    /// the top of the band the plain analog Q would have come out three
    /// times too narrow, which is what rippled the treble.
    fn peaking(hz: f64, width: f64, gain_db: f64, rate: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = std::f64::consts::TAU * hz / rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin * ((std::f64::consts::LN_2 / 2.0) * width * w0 / sin).sinh();
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    /// The filter's gain at a frequency, in decibels, from its transfer
    /// function on the unit circle: what is played, including the bend
    /// the bilinear transform puts near the top of the band.
    fn gain_db_at(&self, hz: f64, rate: f64) -> f64 {
        let w = std::f64::consts::TAU * hz / rate;
        let (sin1, cos1) = w.sin_cos();
        let (sin2, cos2) = (2.0 * w).sin_cos();
        let numerator = (self.b0 + self.b1 * cos1 + self.b2 * cos2).powi(2)
            + (self.b1 * sin1 + self.b2 * sin2).powi(2);
        let denominator = (1.0 + self.a1 * cos1 + self.a2 * cos2).powi(2)
            + (self.a1 * sin1 + self.a2 * sin2).powi(2);
        10.0 * (numerator / denominator).log10()
    }

    fn coefficients(self) -> [f64; 5] {
        [self.b0, self.b1, self.b2, self.a1, self.a2]
    }
}
/// The output worker writes a complete design. The callback tries one read
/// per block and keeps its current design if the worker is publishing.
pub(crate) mod realtime {
    use super::{EqSettings, chain};
    use std::sync::atomic::{AtomicU64, Ordering};

    const COEFFICIENTS: usize = 51;
    type Design = [f64; COEFFICIENTS];

    pub struct Published<const N: usize> {
        sequence: AtomicU64,
        data: [AtomicU64; N],
    }

    impl<const N: usize> Published<N> {
        pub fn new(values: [f64; N]) -> Self {
            Self {
                sequence: AtomicU64::new(2),
                data: values.map(|value| AtomicU64::new(value.to_bits())),
            }
        }

        pub fn update(&self, values: [f64; N]) {
            self.sequence.fetch_add(1, Ordering::SeqCst);
            for (cell, value) in self.data.iter().zip(values) {
                cell.store(value.to_bits(), Ordering::SeqCst);
            }
            self.sequence.fetch_add(1, Ordering::SeqCst);
        }

        pub fn read(&self, known: u64) -> Option<(u64, [f64; N])> {
            let sequence = self.sequence.load(Ordering::SeqCst);
            if sequence == known || sequence % 2 != 0 {
                return None;
            }
            let values =
                std::array::from_fn(|i| f64::from_bits(self.data[i].load(Ordering::SeqCst)));
            (sequence == self.sequence.load(Ordering::SeqCst)).then_some((sequence, values))
        }
    }

    pub type Coefficients = Published<COEFFICIENTS>;

    #[cfg(feature = "native-output")]
    pub fn settings_data(settings: EqSettings) -> [f64; 12] {
        let settings = settings.clamped();
        let mut values = [0.0; 12];
        values[0] = if settings.on { 1.0 } else { 0.0 };
        values[1] = f64::from(settings.preamp_db);
        values[2..].copy_from_slice(&settings.bands_db.map(f64::from));
        values
    }

    #[cfg(feature = "native-output")]
    pub fn settings_from(values: [f64; 12]) -> EqSettings {
        EqSettings {
            on: values[0] != 0.0,
            preamp_db: values[1] as f32,
            bands_db: std::array::from_fn(|i| values[i + 2] as f32),
        }
    }

    pub fn design(settings: EqSettings, rate: u32) -> Design {
        let mut values = [0.0; COEFFICIENTS];
        let settings = settings.clamped();
        if settings.on {
            values[0] = 10f64.powf(f64::from(settings.preamp_db) / 20.0);
            for (slot, filter) in values[1..]
                .chunks_exact_mut(5)
                .zip(chain(&settings.bands_db, f64::from(rate)))
            {
                slot.copy_from_slice(&filter.coefficients());
            }
        } else {
            values[0] = 1.0;
        }
        values
    }

    #[derive(Default)]
    struct Section {
        coefficients: [f64; 5],
        // The original Direct Form I, with independent history per channel.
        history: [[f64; 4]; 2],
    }

    impl Section {
        fn process(&mut self, input: [f64; 2]) -> [f64; 2] {
            let [b0, b1, b2, a1, a2] = self.coefficients;
            std::array::from_fn(|channel| {
                let x = input[channel];
                let [x1, x2, y1, y2] = self.history[channel];
                let y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
                self.history[channel] = [x, x1, y, y1];
                y
            })
        }
    }

    struct Bank {
        sections: [Section; 10],
        count: usize,
        gain: f64,
    }

    impl Bank {
        fn new(design: Design) -> Self {
            let mut bank = Self {
                sections: std::array::from_fn(|_| Section::default()),
                count: 0,
                gain: design[0],
            };
            for (section, coefficients) in bank.sections.iter_mut().zip(design[1..].chunks_exact(5))
            {
                if coefficients[0] == 0.0 {
                    break;
                }
                section.coefficients.copy_from_slice(coefficients);
                bank.count += 1;
            }
            bank
        }

        fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
            if self.count == 0 && self.gain == 1.0 {
                return frame;
            }
            let mut frame = frame.map(|sample| f64::from(sample) * self.gain);
            for section in &mut self.sections[..self.count] {
                frame = section.process(frame);
            }
            frame.map(|sample| sample as f32)
        }
    }

    pub struct Processor {
        version: u64,
        current: Bank,
        previous: Option<Bank>,
        remaining: u32,
        fade: u32,
        reduction: f32,
        release: f32,
    }

    impl Processor {
        pub fn new(publication: &Coefficients, rate: u32) -> Self {
            let (version, design) = publication.read(0).expect("initial EQ design");
            Self {
                version,
                current: Bank::new(design),
                previous: None,
                remaining: 0,
                fade: (rate / 200).max(1),
                reduction: 1.0,
                release: 1.0 - (-10.0 / rate as f32).exp(),
            }
        }

        pub fn refresh(&mut self, publication: &Coefficients) {
            if self.remaining == 0
                && let Some((version, design)) = publication.read(self.version)
            {
                self.version = version;
                self.previous = Some(std::mem::replace(&mut self.current, Bank::new(design)));
                self.remaining = self.fade;
            }
        }

        pub fn shape(&mut self, input: [f32; 2]) -> [f32; 2] {
            let current = self.current.process(input);
            if let Some(previous) = self.previous.as_mut() {
                let previous = previous.process(input);
                let mix = 1.0 - self.remaining as f32 / self.fade as f32;
                self.remaining -= 1;
                if self.remaining == 0 {
                    self.previous = None;
                }
                return std::array::from_fn(|i| previous[i] + mix * (current[i] - previous[i]));
            }
            current
        }

        /// Stereo-linked soft knee adapted from Spotifast src/limiter.rs.
        /// Instant attack avoids adding delay to the existing PCM clock.
        pub fn output(&mut self, input: [f32; 2], volume: f32) -> [f32; 2] {
            let frame = input.map(|sample| sample * volume);
            let peak = frame[0].abs().max(frame[1].abs());
            let active =
                self.current.gain != 1.0 || self.current.count != 0 || self.previous.is_some();
            if !active && self.reduction == 1.0 && peak <= 1.0 {
                return frame;
            }
            let target = if peak <= 0.794_328_2 {
                1.0
            } else if peak >= 1.0 {
                0.891_250_9 / peak
            } else {
                let above = 20.0 * peak.log10() + 2.0;
                10f32.powf(-above * above / 80.0)
            };
            if target < self.reduction {
                self.reduction = target;
            } else {
                self.reduction += (target - self.reduction) * self.release;
                if self.reduction > 0.999_99 {
                    self.reduction = 1.0;
                }
            }
            frame.map(|sample| sample * self.reduction)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn measured_gain(processor: &mut Processor, hz: f64, rate: u32) -> f64 {
            let mut before = 0.0;
            let mut after = 0.0;
            for i in 0..rate {
                let sample = (0.05
                    * (std::f64::consts::TAU * hz * f64::from(i) / f64::from(rate)).sin())
                    as f32;
                let frame = processor.shape([sample, -sample]);
                let output = processor.output(frame, 1.0);
                if i >= rate / 2 {
                    before += f64::from(sample).powi(2);
                    after += f64::from(output[0]).powi(2);
                }
            }
            10.0 * (after / before).log10()
        }

        #[test]
        fn live_bands_modify_pcm_at_the_native_rate_and_bypass_restores_it() {
            for rate in [22_050, 44_100, 48_000] {
                let mut settings = EqSettings::default();
                let coefficients = Coefficients::new(design(settings, rate));
                let mut processor = Processor::new(&coefficients, rate);
                assert_eq!(measured_gain(&mut processor, 1000.0, rate), 0.0);
                settings.on = true;
                settings.bands_db[4] = 12.0;
                coefficients.update(design(settings, rate));
                processor.refresh(&coefficients);
                let gain = measured_gain(&mut processor, 1000.0, rate);
                let remote = measured_gain(&mut processor, 60.0, rate);
                eprintln!("{rate} Hz sample rate: 1 kHz {gain:+.2} dB, 60 Hz {remote:+.2} dB");
                assert!((gain - 12.0).abs() < 0.3);
                assert!(remote.abs() < 0.3);
                coefficients.update(design(EqSettings::default(), rate));
                processor.refresh(&coefficients);
                assert_eq!(measured_gain(&mut processor, 1000.0, rate), 0.0);
            }
        }

        #[test]
        fn boosted_pcm_preserves_stereo_and_does_not_clip() {
            let rate = 48_000;
            let coefficients = Coefficients::new(design(
                EqSettings {
                    on: true,
                    preamp_db: 12.0,
                    bands_db: [12.0; 10],
                },
                rate,
            ));
            let mut processor = Processor::new(&coefficients, rate);
            let mut peak = 0.0_f32;
            for i in 0..rate {
                let x = (std::f32::consts::TAU * 1000.0 * i as f32 / rate as f32).sin();
                let frame = processor.shape([x, x / 2.0]);
                let [left, right] = processor.output(frame, 1.0);
                assert!((left - 2.0 * right).abs() < 1e-6);
                peak = peak.max(left.abs());
                assert!(left.is_finite() && left.abs() <= 0.9);
            }
            eprintln!("Peak with maximum preamp and all bands boosted: {peak:.4} FS");
            assert!(peak > 0.5);
        }
    }
}
