//! Pure audio processing for speech: no devices, no I/O.

use std::ops::Range;

use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

pub const TARGET_RATE: u32 = 16_000;
pub const FRAME_MS: u32 = 20;
/// A 20 ms frame louder than this (≈ −40 dBFS) counts as voice.
pub const VOICE_RMS: f32 = 0.01;
pub const PAD_MS: u32 = 150;
const MAX_GAIN: f32 = 10.0;

pub fn downmix(interleaved: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch == 1 {
        return interleaved.to_vec();
    }
    interleaved.chunks_exact(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect()
}

pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let params = SincInterpolationParameters {
        sinc_len: 128,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    let ratio = to as f64 / from as f64;
    let chunk = 1024;
    let Ok(mut r) = SincFixedIn::<f32>::new(ratio, 1.0, params, chunk, 1) else {
        return linear_resample(samples, from, to);
    };
    let delay = r.output_delay();
    let expected = (samples.len() as f64 * ratio).round() as usize;
    let mut out = Vec::with_capacity(expected + delay + chunk);
    let mut pos = 0;
    while pos + chunk <= samples.len() {
        match r.process(&[&samples[pos..pos + chunk]], None) {
            Ok(v) => out.extend_from_slice(&v[0]),
            Err(_) => return linear_resample(samples, from, to),
        }
        pos += chunk;
    }
    if pos < samples.len() {
        match r.process_partial(Some(&[&samples[pos..]]), None) {
            Ok(v) => out.extend_from_slice(&v[0]),
            Err(_) => return linear_resample(samples, from, to),
        }
    }
    // Flush the filter so the tail is not lost.
    while out.len() < expected + delay {
        match r.process_partial::<&[f32]>(None, None) {
            Ok(v) if !v[0].is_empty() => out.extend_from_slice(&v[0]),
            _ => break,
        }
    }
    let end = (delay + expected).min(out.len());
    out[delay.min(end)..end].to_vec()
}

fn linear_resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    let n = (samples.len() as f64 * to as f64 / from as f64).round() as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * from as f64 / to as f64;
            let j = x.floor() as usize;
            let t = (x - j as f64) as f32;
            let a = samples[j.min(samples.len() - 1)];
            let b = samples[(j + 1).min(samples.len() - 1)];
            a + (b - a) * t
        })
        .collect()
}

/// One-pole RC high-pass filter, in place.
pub fn high_pass(samples: &mut [f32], rate: u32, cutoff_hz: f32) {
    let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
    let dt = 1.0 / rate as f32;
    let a = rc / (rc + dt);
    let mut prev_x = samples.first().copied().unwrap_or(0.0);
    let mut prev_y = 0.0f32;
    for s in samples.iter_mut() {
        let x = *s;
        let y = a * (prev_y + x - prev_x);
        prev_x = x;
        prev_y = y;
        *s = y;
    }
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Less voiced sound than this in a whole recording is a click, a breath or a bump — not
/// speech (the shortest word is longer). Below it nothing is sent, so the model cannot
/// invent text from noise.
pub const MIN_VOICED_MS: u32 = 250;

/// Loudness facts about a recording, for the log (tuning the speech gate on real data).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceStats {
    pub voiced_ms: u32,
    pub peak_rms: f32,
}

pub fn voice_stats(samples: &[f32], rate: u32) -> VoiceStats {
    let frame = (rate * FRAME_MS / 1000).max(1) as usize;
    let levels: Vec<f32> = samples.chunks(frame).map(rms).collect();
    let voiced = levels.iter().filter(|&&l| l > VOICE_RMS).count() as u32;
    VoiceStats { voiced_ms: voiced * FRAME_MS, peak_rms: levels.iter().fold(0.0, |m, &l| m.max(l)) }
}

/// Whether the recording holds enough voiced sound to be speech (`MIN_VOICED_MS`).
pub fn has_speech(stats: VoiceStats) -> bool {
    stats.voiced_ms >= MIN_VOICED_MS
}

/// The span from the first to the last voiced frame, padded by `PAD_MS`.
pub fn voiced_range(samples: &[f32], rate: u32) -> Option<Range<usize>> {
    let frame = (rate * FRAME_MS / 1000).max(1) as usize;
    let pad = (rate * PAD_MS / 1000) as usize;
    let voiced = |(_, f): &(usize, &[f32])| rms(f) > VOICE_RMS;
    let first = samples.chunks(frame).enumerate().find(voiced)?.0;
    let last = samples.chunks(frame).enumerate().rev().find(voiced)?.0;
    let start = (first * frame).saturating_sub(pad);
    let end = ((last + 1) * frame + pad).min(samples.len());
    Some(start..end)
}

/// Scales so the peak reaches `target_dbfs`, never amplifying more than 10×.
pub fn normalize_peak(samples: &mut [f32], target_dbfs: f32) {
    let peak = samples.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak <= f32::EPSILON {
        return;
    }
    let target = 10f32.powf(target_dbfs / 20.0);
    let gain = (target / peak).min(MAX_GAIN);
    for s in samples.iter_mut() {
        *s *= gain;
    }
}

/// Mono 16-bit PCM WAV.
pub fn encode_wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::with_capacity(44 + samples.len() * 2));
    if let Ok(mut w) = hound::WavWriter::new(&mut cursor, spec) {
        for s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            if w.write_sample(v).is_err() {
                break;
            }
        }
        let _ = w.finalize();
    }
    cursor.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, secs: f32, amp: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .map(|i| amp * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / rate as f32).sin())
            .collect()
    }

    fn peak(s: &[f32]) -> f32 {
        s.iter().fold(0.0f32, |m, x| m.max(x.abs()))
    }

    #[test]
    fn downmix_stereo_averages() {
        assert_eq!(downmix(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
        assert_eq!(downmix(&[0.1, 0.2], 1), vec![0.1, 0.2]);
    }

    #[test]
    fn resample_48k_to_16k_length() {
        let out = resample(&sine(48_000, 1.0, 0.5), 48_000, 16_000);
        assert!((out.len() as i64 - 16_000).abs() <= 32, "len {}", out.len());
    }

    #[test]
    fn resample_same_rate_is_copy() {
        let s = sine(16_000, 0.1, 0.5);
        assert_eq!(resample(&s, 16_000, 16_000), s);
    }

    #[test]
    fn resample_keeps_amplitude() {
        let out = resample(&sine(48_000, 1.0, 0.5), 48_000, 16_000);
        let mid = &out[4_000..12_000];
        assert!((peak(mid) - 0.5).abs() < 0.05, "peak {}", peak(mid));
    }

    #[test]
    fn high_pass_removes_dc() {
        let mut s = vec![0.5f32; 16_000];
        high_pass(&mut s, 16_000, 80.0);
        let tail = &s[8_000..];
        let mean = tail.iter().sum::<f32>() / tail.len() as f32;
        assert!(mean.abs() < 0.01, "mean {mean}");
    }

    #[test]
    fn a_click_or_a_breath_is_not_speech() {
        let mut s = vec![0.0f32; 16_000];
        s.extend(sine(16_000, 0.04, 0.5)); // 40 ms click
        s.extend(vec![0.0f32; 16_000]);
        let st = voice_stats(&s, 16_000);
        assert!(st.voiced_ms <= 60, "{st:?}");
        assert!(st.peak_rms > 0.3);
        assert!(!has_speech(st));
        assert!(!has_speech(voice_stats(&vec![0.0; 16_000], 16_000)));
    }

    #[test]
    fn a_short_word_is_speech() {
        let mut s = vec![0.0f32; 16_000];
        s.extend(sine(16_000, 0.3, 0.1)); // "да": 300 ms
        s.extend(vec![0.0f32; 16_000]);
        let st = voice_stats(&s, 16_000);
        assert!((st.voiced_ms as i64 - 300).abs() <= 40, "{st:?}");
        assert!(has_speech(st));
    }

    #[test]
    fn silence_has_no_voiced_range() {
        assert_eq!(voiced_range(&vec![0.0; 16_000], 16_000), None);
    }

    #[test]
    fn voiced_range_pads_150ms() {
        let mut s = vec![0.0f32; 16_000];
        s.extend(sine(16_000, 1.0, 0.3));
        s.extend(vec![0.0f32; 16_000]);
        let r = voiced_range(&s, 16_000).unwrap();
        assert!((r.start as i64 - (16_000 - 2_400)).abs() <= 320, "start {}", r.start);
        assert!((r.end as i64 - (32_000 + 2_400)).abs() <= 320, "end {}", r.end);
    }

    #[test]
    fn voiced_range_clamps_to_bounds() {
        let s = sine(16_000, 0.5, 0.3);
        assert_eq!(voiced_range(&s, 16_000), Some(0..s.len()));
    }

    #[test]
    fn normalize_hits_minus_1_dbfs() {
        let mut s = sine(16_000, 0.5, 0.2);
        normalize_peak(&mut s, -1.0);
        assert!((peak(&s) - 0.891).abs() < 0.005, "peak {}", peak(&s));
    }

    #[test]
    fn normalize_caps_gain_at_10x() {
        let mut s = sine(16_000, 0.5, 0.01);
        normalize_peak(&mut s, -1.0);
        assert!((peak(&s) - 0.1).abs() < 0.002, "peak {}", peak(&s));
    }

    #[test]
    fn normalize_silence_stays_silent() {
        let mut s = vec![0.0f32; 100];
        normalize_peak(&mut s, -1.0);
        assert!(s.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn wav_header_is_16k_mono_16bit() {
        let wav = encode_wav(&sine(16_000, 0.2, 0.5), 16_000);
        let r = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        let spec = r.spec();
        assert_eq!((spec.sample_rate, spec.channels, spec.bits_per_sample), (16_000, 1, 16));
        assert_eq!(r.len(), 3_200);
    }

    #[test]
    fn wav_clips_out_of_range_samples() {
        let wav = encode_wav(&[2.0, -2.0], 16_000);
        let mut r = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        let v: Vec<i16> = r.samples::<i16>().map(|x| x.unwrap()).collect();
        assert_eq!(v, vec![i16::MAX, -i16::MAX]);
    }

    #[test]
    fn ten_minutes_fit_groq_limit() {
        let wav = encode_wav(&vec![0.1f32; 600 * 16_000], 16_000);
        assert!(wav.len() < 25 * 1024 * 1024);
    }
}
