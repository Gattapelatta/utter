//! Where to cut a long recording into pieces that are processed while the user
//! is still speaking (spec §4.2 speed decision 2026-10-08).

use super::dsp::{rms, FRAME_MS, VOICE_RMS};

/// Pieces shorter than this are not sent on their own.
pub const MIN_PIECE_SECS: f32 = 10.0;
/// Without any pause, a piece is cut at its quietest moment once it is this long.
pub const MAX_PIECE_SECS: f32 = 25.0;
/// Only a pause at least this long counts as a place to cut.
const PAUSE_MS: u32 = 400;
/// The forced cut looks for the quietest frame within this many seconds before the maximum.
const FORCED_WINDOW_SECS: f32 = 5.0;

/// Mono sample index to cut `mono` at, or `None` to keep listening.
pub fn find_cut(mono: &[f32], rate: u32) -> Option<usize> {
    let frame = (rate * FRAME_MS / 1000).max(1) as usize;
    let levels: Vec<f32> = mono.chunks(frame).map(rms).collect();
    let min_frame = (MIN_PIECE_SECS * 1000.0 / FRAME_MS as f32) as usize;
    let pause_frames = (PAUSE_MS / FRAME_MS) as usize;

    // The first long enough pause that starts after the minimum length.
    let mut run_start = None;
    for (i, &l) in levels.iter().enumerate() {
        if l < VOICE_RMS {
            let start = *run_start.get_or_insert(i);
            if start >= min_frame && i + 1 - start >= pause_frames {
                // Cut in the middle of the pause seen so far.
                return Some((start + i).div_ceil(2) * frame);
            }
        } else {
            run_start = None;
        }
    }

    let max_frame = (MAX_PIECE_SECS * 1000.0 / FRAME_MS as f32) as usize;
    if levels.len() < max_frame {
        return None;
    }
    let from = max_frame.saturating_sub((FORCED_WINDOW_SECS * 1000.0 / FRAME_MS as f32) as usize);
    let quietest = (from..max_frame).min_by(|&a, &b| levels[a].total_cmp(&levels[b]))?;
    Some(quietest * frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 16_000;

    fn tone(secs: f32) -> Vec<f32> {
        (0..(RATE as f32 * secs) as usize)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / RATE as f32).sin())
            .collect()
    }

    fn silence(secs: f32) -> Vec<f32> {
        vec![0.0; (RATE as f32 * secs) as usize]
    }

    fn join(parts: &[Vec<f32>]) -> Vec<f32> {
        parts.concat()
    }

    #[test]
    fn no_cut_before_the_minimum_length() {
        let a = join(&[tone(5.0), silence(1.0), tone(3.0)]);
        assert_eq!(find_cut(&a, RATE), None);
    }

    #[test]
    fn cuts_in_the_middle_of_the_first_pause_after_ten_seconds() {
        let a = join(&[tone(4.0), silence(0.6), tone(7.0), silence(0.6), tone(2.0)]);
        let cut = find_cut(&a, RATE).expect("a cut");
        let secs = cut as f32 / RATE as f32;
        assert!((11.8..=12.4).contains(&secs), "cut at {secs} s");
    }

    #[test]
    fn short_pauses_are_not_cut() {
        let a = join(&[tone(11.0), silence(0.2), tone(5.0)]);
        assert_eq!(find_cut(&a, RATE), None);
    }

    #[test]
    fn long_speech_without_pauses_is_cut_at_the_quietest_moment() {
        let mut a = tone(26.0);
        // A quieter (but not silent) dip at 22 s.
        let dip = (22.0 * RATE as f32) as usize;
        for s in &mut a[dip..dip + RATE as usize / 10] {
            *s *= 0.05;
        }
        let cut = find_cut(&a, RATE).expect("forced cut");
        let secs = cut as f32 / RATE as f32;
        assert!((21.9..=22.15).contains(&secs), "cut at {secs} s");
    }

    #[test]
    fn no_forced_cut_before_the_maximum() {
        assert_eq!(find_cut(&tone(24.0), RATE), None);
    }
}
