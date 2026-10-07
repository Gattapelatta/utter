//! Quiet start/stop cues, synthesised so no audio files ship with the app.

const RATE: u32 = 44_100;
const NOTE_MS: u32 = 60;
const FADE_MS: u32 = 5;
/// Peak amplitude, −20 dBFS.
const PEAK: f32 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Start,
    Stop,
}

/// Two short sine notes (rising for Start, falling for Stop) as a 16-bit mono WAV.
pub fn cue_wav(cue: Cue) -> Vec<u8> {
    let notes: [f32; 2] = match cue {
        Cue::Start => [660.0, 880.0],
        Cue::Stop => [880.0, 660.0],
    };
    let per_note = (RATE * NOTE_MS / 1000) as usize;
    let fade = (RATE * FADE_MS / 1000) as usize;
    let mut samples = Vec::with_capacity(per_note * notes.len());
    for freq in notes {
        for i in 0..per_note {
            // Linear fades to zero at both ends of every note avoid clicks.
            let env = (i.min(per_note - 1 - i) as f32 / fade as f32).min(1.0);
            let t = i as f32 / RATE as f32;
            let v = PEAK * env * (2.0 * std::f32::consts::PI * freq * t).sin();
            samples.push((v * i16::MAX as f32).round() as i16);
        }
    }
    let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut buf = std::io::Cursor::new(Vec::new());
    if let Ok(mut w) = hound::WavWriter::new(&mut buf, spec) {
        for s in &samples {
            let _ = w.write_sample(*s);
        }
        let _ = w.finalize();
    }
    buf.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(wav: Vec<u8>) -> (hound::WavSpec, Vec<i16>) {
        let r = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        let spec = r.spec();
        (spec, r.into_samples::<i16>().map(|s| s.unwrap()).collect())
    }

    #[test]
    fn cue_is_short_quiet_wav() {
        let (spec, s) = samples(cue_wav(Cue::Start));
        assert_eq!((spec.sample_rate, spec.channels, spec.bits_per_sample), (44_100, 1, 16));
        let ms = s.len() as f32 * 1000.0 / 44_100.0;
        assert!((115.0..=125.0).contains(&ms), "{ms} ms");
        let peak = s.iter().map(|v| v.unsigned_abs()).max().unwrap();
        assert!(peak as f32 <= 0.1 * i16::MAX as f32 + 1.0, "peak {peak}");
        assert!(peak > 1000, "audible");
    }

    #[test]
    fn cue_starts_and_ends_silent() {
        for cue in [Cue::Start, Cue::Stop] {
            let (_, s) = samples(cue_wav(cue));
            assert_eq!((s[0], s[s.len() - 1]), (0, 0));
        }
    }

    #[test]
    fn start_and_stop_differ() {
        assert_ne!(cue_wav(Cue::Start), cue_wav(Cue::Stop));
    }
}
