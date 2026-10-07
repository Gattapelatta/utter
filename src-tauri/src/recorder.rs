//! Microphone capture on a dedicated thread (cpal streams are not `Send`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio::{dsp, Captured};

const LEVEL_INTERVAL: Duration = Duration::from_millis(33);

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum RecorderError {
    #[error("no input device")]
    NoDevice,
    #[error("stream: {0}")]
    Stream(String),
    #[error("not recording")]
    NotRecording,
}

/// Anything that can record speech: the real microphone or a test fake.
pub trait AudioSource: Send + Sync {
    fn start(&self) -> Result<(), RecorderError>;
    fn stop(&self) -> Result<Captured, RecorderError>;
    fn cancel(&self);
}

pub enum SampleChunk<'a> {
    F32(&'a [f32]),
    I16(&'a [i16]),
    U16(&'a [u16]),
}

pub fn to_f32(chunk: SampleChunk) -> Vec<f32> {
    match chunk {
        SampleChunk::F32(s) => s.to_vec(),
        SampleChunk::I16(s) => s.iter().map(|&x| x as f32 / 32768.0).collect(),
        SampleChunk::U16(s) => s.iter().map(|&x| (x as f32 - 32768.0) / 32768.0).collect(),
    }
}

/// Joins segments recorded on different devices into one mono capture at the
/// first segment's sample rate.
pub fn merge_segments(segments: Vec<Captured>) -> Option<Captured> {
    let segments: Vec<Captured> = segments.into_iter().filter(|s| !s.samples.is_empty()).collect();
    let rate = segments.first()?.rate;
    let mut samples = Vec::new();
    for s in segments {
        let mono = dsp::downmix(&s.samples, s.channels);
        samples.extend(dsp::resample(&mono, s.rate, rate));
    }
    Some(Captured { samples, rate, channels: 1 })
}

enum Cmd {
    Start(mpsc::Sender<Result<(), RecorderError>>),
    Stop(mpsc::Sender<Result<Captured, RecorderError>>),
    Cancel,
}

pub struct Recorder {
    cmd: mpsc::Sender<Cmd>,
}

impl Recorder {
    pub fn new(levels: tokio::sync::watch::Sender<f32>) -> Self {
        let (cmd, rx) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("utter-recorder".into())
            .spawn(move || run(rx, levels));
        Self { cmd }
    }
}

impl AudioSource for Recorder {
    fn start(&self) -> Result<(), RecorderError> {
        let (tx, rx) = mpsc::channel();
        self.cmd.send(Cmd::Start(tx)).map_err(|_| RecorderError::Stream("recorder stopped".into()))?;
        rx.recv().unwrap_or_else(|_| Err(RecorderError::Stream("recorder stopped".into())))
    }

    fn stop(&self) -> Result<Captured, RecorderError> {
        let (tx, rx) = mpsc::channel();
        self.cmd.send(Cmd::Stop(tx)).map_err(|_| RecorderError::NotRecording)?;
        rx.recv().unwrap_or(Err(RecorderError::NotRecording))
    }

    fn cancel(&self) {
        let _ = self.cmd.send(Cmd::Cancel);
    }
}

struct Live {
    _stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    failed: Arc<AtomicBool>,
    rate: u32,
    channels: u16,
}

impl Live {
    fn take(self) -> Captured {
        let samples = self.buf.lock().map(|mut b| std::mem::take(&mut *b)).unwrap_or_default();
        Captured { samples, rate: self.rate, channels: self.channels }
    }

    fn level(&self) -> f32 {
        let window = (self.rate as f32 * LEVEL_INTERVAL.as_secs_f32()) as usize * self.channels as usize;
        self.buf
            .lock()
            .map(|b| dsp::rms(&b[b.len().saturating_sub(window)..]))
            .unwrap_or(0.0)
    }
}

fn open() -> Result<Live, RecorderError> {
    let device = cpal::default_host().default_input_device().ok_or(RecorderError::NoDevice)?;
    let config = device.default_input_config().map_err(|e| RecorderError::Stream(e.to_string()))?;
    let rate = config.sample_rate().0;
    let channels = config.channels();
    let buf = Arc::new(Mutex::new(Vec::<f32>::with_capacity(rate as usize * channels as usize * 30)));
    let failed = Arc::new(AtomicBool::new(false));
    let on_err = {
        let failed = failed.clone();
        move |e: cpal::StreamError| {
            tracing::warn!("input stream error: {e}");
            failed.store(true, Ordering::SeqCst);
        }
    };
    let b = buf.clone();
    let stream_config = config.config();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |d: &[f32], _: &_| push(&b, to_f32(SampleChunk::F32(d))),
            on_err,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |d: &[i16], _: &_| push(&b, to_f32(SampleChunk::I16(d))),
            on_err,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            move |d: &[u16], _: &_| push(&b, to_f32(SampleChunk::U16(d))),
            on_err,
            None,
        ),
        other => return Err(RecorderError::Stream(format!("unsupported sample format {other:?}"))),
    }
    .map_err(|e| RecorderError::Stream(e.to_string()))?;
    stream.play().map_err(|e| RecorderError::Stream(e.to_string()))?;
    Ok(Live { _stream: stream, buf, failed, rate, channels })
}

fn push(buf: &Arc<Mutex<Vec<f32>>>, samples: Vec<f32>) {
    if let Ok(mut b) = buf.lock() {
        b.extend(samples);
    }
}

fn run(rx: mpsc::Receiver<Cmd>, levels: tokio::sync::watch::Sender<f32>) {
    let mut live: Option<Live> = None;
    let mut segments: Vec<Captured> = Vec::new();
    loop {
        match rx.recv_timeout(LEVEL_INTERVAL) {
            Ok(Cmd::Start(reply)) => {
                segments.clear();
                live = None;
                let result = open().map(|l| live = Some(l));
                let _ = reply.send(result);
            }
            Ok(Cmd::Stop(reply)) => {
                if let Some(l) = live.take() {
                    segments.push(l.take());
                }
                let _ = levels.send(0.0);
                let merged = merge_segments(std::mem::take(&mut segments));
                let _ = reply.send(merged.ok_or(RecorderError::NoDevice));
            }
            Ok(Cmd::Cancel) => {
                live = None;
                segments.clear();
                let _ = levels.send(0.0);
            }
            Err(RecvTimeoutError::Timeout) => {
                let Some(l) = live.as_ref() else { continue };
                if l.failed.load(Ordering::SeqCst) {
                    // Device vanished mid-recording: keep what we have, continue on the new default.
                    if let Some(l) = live.take() {
                        segments.push(l.take());
                    }
                    match open() {
                        Ok(l) => live = Some(l),
                        Err(e) => tracing::warn!("cannot reopen input: {e}"),
                    }
                    continue;
                }
                let _ = levels.send(l.level());
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_f32_converts_i16_and_u16() {
        assert_eq!(to_f32(SampleChunk::I16(&[i16::MIN, 0])), vec![-1.0, 0.0]);
        assert!((to_f32(SampleChunk::I16(&[i16::MAX]))[0] - 1.0).abs() < 1e-4);
        assert_eq!(to_f32(SampleChunk::U16(&[32768])), vec![0.0]);
        assert_eq!(to_f32(SampleChunk::U16(&[0])), vec![-1.0]);
        assert_eq!(to_f32(SampleChunk::F32(&[0.25])), vec![0.25]);
    }

    #[test]
    fn merge_segments_resamples_to_first_rate() {
        let a = Captured { samples: vec![0.5; 4_800 * 2], rate: 48_000, channels: 2 };
        let b = Captured { samples: vec![0.5; 1_600], rate: 16_000, channels: 1 };
        let m = merge_segments(vec![a, b]).unwrap();
        assert_eq!((m.rate, m.channels), (48_000, 1));
        assert!((m.samples.len() as i64 - 9_600).abs() <= 64, "len {}", m.samples.len());
    }

    #[test]
    fn merge_of_nothing_is_none() {
        assert!(merge_segments(vec![]).is_none());
        assert!(merge_segments(vec![Captured { samples: vec![], rate: 48_000, channels: 1 }]).is_none());
    }

    #[test]
    #[ignore = "uses the real microphone"]
    fn records_from_default_mic() {
        let (tx, _rx) = tokio::sync::watch::channel(0.0f32);
        let r = Recorder::new(tx);
        r.start().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let c = r.stop().unwrap();
        assert!(!c.samples.is_empty());
    }
}
