pub mod dsp;

/// Raw capture straight from the input device (interleaved).
#[derive(Clone, Debug, Default)]
pub struct Captured {
    pub samples: Vec<f32>,
    pub rate: u32,
    pub channels: u16,
}

/// Audio ready to upload.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub wav: Vec<u8>,
    pub audio_secs: f32,
}

/// Turns a capture into clean 16 kHz mono WAV. `None` means nothing was said.
pub fn prepare(c: Captured) -> Option<Prepared> {
    let mut mono = dsp::downmix(&c.samples, c.channels);
    dsp::high_pass(&mut mono, c.rate, 80.0);
    let speech = dsp::resample(&mono, c.rate, dsp::TARGET_RATE);
    let range = dsp::voiced_range(&speech, dsp::TARGET_RATE)?;
    let mut voiced = speech[range].to_vec();
    dsp::normalize_peak(&mut voiced, -1.0);
    Some(Prepared {
        audio_secs: voiced.len() as f32 / dsp::TARGET_RATE as f32,
        wav: dsp::encode_wav(&voiced, dsp::TARGET_RATE),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stereo_speech(rate: u32) -> Vec<f32> {
        // 1 s silence, 2 s tone, 1 s silence, interleaved stereo.
        let mut mono = vec![0.0f32; rate as usize];
        mono.extend((0..rate as usize * 2).map(|i| {
            0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / rate as f32).sin()
        }));
        mono.extend(vec![0.0f32; rate as usize]);
        mono.iter().flat_map(|x| [*x, *x]).collect()
    }

    #[test]
    fn prepare_returns_none_for_silence() {
        let c = Captured { samples: vec![0.0; 96_000], rate: 48_000, channels: 2 };
        assert!(prepare(c).is_none());
    }

    #[test]
    fn prepare_reports_trimmed_duration() {
        let c = Captured { samples: stereo_speech(48_000), rate: 48_000, channels: 2 };
        let p = prepare(c).unwrap();
        // 2 s of voice plus 150 ms of padding on each side.
        assert!((p.audio_secs - 2.3).abs() < 0.05, "secs {}", p.audio_secs);
        let r = hound::WavReader::new(std::io::Cursor::new(p.wav)).unwrap();
        assert_eq!(r.spec().sample_rate, 16_000);
        assert_eq!(r.spec().channels, 1);
    }

    #[test]
    #[ignore = "benchmark: prints prepare() time for 10 s of 48 kHz stereo"]
    fn bench_prepare_10s() {
        let c = Captured { samples: stereo_speech(48_000).repeat(3), rate: 48_000, channels: 2 };
        let t = std::time::Instant::now();
        let p = prepare(c).unwrap();
        println!("prepare 12 s @48k stereo: {} ms (audio {:.1} s)", t.elapsed().as_millis(), p.audio_secs);
    }
}
