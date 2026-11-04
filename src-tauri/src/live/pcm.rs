//! Microphone audio → 16 kHz mono 16-bit PCM in 100 ms chunks for the Live API.

use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

use crate::audio::dsp::{downmix, TARGET_RATE};

/// 100 ms at 16 kHz.
pub const CHUNK: usize = 1_600;

/// Keeps resampler state between pushes, so the stream has no seams every 100 ms; the
/// filter's delay (~1 ms) is trimmed at the start and drained at the flush.
pub struct Chunker {
    channels: u16,
    ratio: f64,
    /// Input frames per resampler call: 100 ms at the input rate.
    block: usize,
    resampler: Option<SincFixedIn<f32>>,
    /// Output samples still to drop at the start (the filter's delay).
    skip: usize,
    input: Vec<f32>,
    out: Vec<f32>,
    frames_in: usize,
    samples_out: usize,
}

impl Chunker {
    pub fn new(rate: u32, channels: u16) -> Self {
        let rate = rate.max(1);
        let ratio = TARGET_RATE as f64 / rate as f64;
        let block = (rate as usize / 10).max(1);
        let resampler = (rate != TARGET_RATE)
            .then(|| {
                let params = SincInterpolationParameters {
                    sinc_len: 128,
                    f_cutoff: 0.95,
                    interpolation: SincInterpolationType::Linear,
                    oversampling_factor: 128,
                    window: WindowFunction::BlackmanHarris2,
                };
                SincFixedIn::<f32>::new(ratio, 1.0, params, block, 1).ok()
            })
            .flatten();
        let skip = resampler.as_ref().map_or(0, |r| r.output_delay());
        Self { channels, ratio, block, resampler, skip, input: Vec::new(), out: Vec::new(), frames_in: 0, samples_out: 0 }
    }

    fn emit(&mut self, samples: &[f32]) {
        let n = self.skip.min(samples.len());
        self.skip -= n;
        self.out.extend_from_slice(&samples[n..]);
    }

    fn take(&mut self, n: usize) -> Vec<i16> {
        self.samples_out += n;
        self.out.drain(..n).map(to_i16).collect()
    }

    /// Adds interleaved samples; returns every complete 100 ms chunk.
    pub fn push(&mut self, interleaved: &[f32]) -> Vec<Vec<i16>> {
        let mono = downmix(interleaved, self.channels);
        self.frames_in += mono.len();
        match self.resampler.as_mut() {
            None => self.out.extend_from_slice(&mono),
            Some(_) => {
                self.input.extend_from_slice(&mono);
                while self.input.len() >= self.block {
                    let block: Vec<f32> = self.input.drain(..self.block).collect();
                    let r = self.resampler.as_mut().expect("checked above");
                    if let Ok(v) = r.process(&[&block], None) {
                        let v = v.into_iter().next().unwrap_or_default();
                        self.emit(&v);
                    }
                }
            }
        }
        let mut chunks = Vec::new();
        while self.out.len() >= CHUNK {
            chunks.push(self.take(CHUNK));
        }
        chunks
    }

    /// The last, shorter chunk at the end of the recording.
    pub fn flush(&mut self) -> Option<Vec<i16>> {
        let expected = (self.frames_in as f64 * self.ratio).round() as usize;
        if let Some(r) = self.resampler.as_mut() {
            let rest = std::mem::take(&mut self.input);
            let mut produced = Vec::new();
            if !rest.is_empty() {
                if let Ok(v) = r.process_partial(Some(&[&rest]), None) {
                    produced.extend(v.into_iter().next().unwrap_or_default());
                }
            }
            // Drain the filter so the tail is not lost.
            for _ in 0..8 {
                if self.samples_out + self.out.len() + produced.len().saturating_sub(self.skip) >= expected {
                    break;
                }
                match r.process_partial::<&[f32]>(None, None) {
                    Ok(v) if !v[0].is_empty() => produced.extend_from_slice(&v[0]),
                    _ => break,
                }
            }
            self.emit(&produced);
        }
        let n = expected.saturating_sub(self.samples_out).min(self.out.len());
        self.out.truncate(n);
        (n > 0).then(|| self.take(n))
    }
}

fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, channels: u16, secs: f32) -> Vec<f32> {
        let frames = (rate as f32 * secs) as usize;
        (0..frames)
            .flat_map(|i| {
                let v = (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.5;
                std::iter::repeat_n(v, channels as usize)
            })
            .collect()
    }

    #[test]
    fn chunker_48k_stereo_makes_100ms_chunks() {
        let mut c = Chunker::new(48_000, 2);
        let chunks = c.push(&tone(48_000, 2, 0.35));
        // The filter holds back ~1 ms, so the third chunk completes at the flush.
        assert_eq!(chunks.len(), 2);
        assert!(chunks.iter().all(|ch| ch.len() == CHUNK));
        assert_eq!(c.flush().map(|t| t.len()), Some(2_400));
        assert_eq!(c.flush(), None);
    }

    #[test]
    fn chunker_lag_is_under_one_chunk() {
        let mut c = Chunker::new(48_000, 1);
        let n: usize = (0..10).map(|_| c.push(&tone(48_000, 1, 0.1)).len()).sum();
        assert!(n >= 9, "{n} chunks from 1 s");
    }

    #[test]
    fn chunker_keeps_remainder_between_pushes() {
        let mut c = Chunker::new(48_000, 1);
        let audio = tone(48_000, 1, 0.3);
        let (a, b) = audio.split_at(2_400);
        assert!(c.push(a).is_empty(), "50 ms is not a chunk yet");
        let chunks = c.push(b);
        assert_eq!(chunks.len(), 2);
        assert_eq!(c.flush().map(|t| t.len()), Some(1_600));
    }

    #[test]
    fn chunker_16k_mono_passes_through() {
        let mut c = Chunker::new(16_000, 1);
        let chunks = c.push(&[0.5; 1_700]);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0][0], (0.5 * i16::MAX as f32).round() as i16);
        assert_eq!(c.flush().map(|t| t.len()), Some(100));
    }

    #[test]
    fn chunker_keeps_the_tone() {
        // A resampled 440 Hz tone stays loud (not lost to filter edges).
        let mut c = Chunker::new(44_100, 1);
        let mut all: Vec<i16> = c.push(&tone(44_100, 1, 1.0)).concat();
        all.extend(c.flush().unwrap_or_default());
        assert_eq!(all.len(), 16_000);
        let rms = (all[2_000..14_000].iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / 12_000.0).sqrt();
        assert!((rms / i16::MAX as f64 - 0.5 / 2f64.sqrt()).abs() < 0.03, "rms {rms}");
    }
}
