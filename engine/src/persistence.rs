//! Phosphor persistence across frames. A tube's phosphors keep glowing after the beam has
//! passed, so a bright thing that moves leaves a fading trail. Earlier source frames are
//! dimmed by their age and the brightest of each pixel wins, so a still picture stays exactly
//! as it was. Red phosphor glows longest and blue shortest, so trails run warm.

use crate::image::Image;
use rayon::prelude::*;

/// Earlier frames the trail can reach back to; older ones are too faint to matter.
pub const MAX_FRAMES_BACK: usize = 3;
/// What is left of one frame's glow after a frame at full Persistence.
const DECAY_AT_FULL: f32 = 0.7;
/// Persistence 0..1 feels even when the decay follows its square root.
const DECAY_CURVE: f32 = 0.5;
/// A frame dimmer than this adds nothing you can see.
const FAINTEST: f32 = 0.03;
/// Each phosphor's decay as a power of green's: red lingers, blue fades fast (P22-like).
const CHANNEL_RATE: [f32; 3] = [0.75, 1.0, 1.6];
/// Below this strength the trail is off and no earlier frames are needed.
const MIN_STRENGTH: f32 = 0.005;

/// What is left of a frame's glow one frame later, for a trail strength 0..1.
pub fn decay(strength: f32) -> f32 {
    DECAY_AT_FULL * strength.clamp(0.0, 1.0).powf(DECAY_CURVE)
}

/// How many earlier frames a trail of this strength needs (0 when it is off).
pub fn frames_back(strength: f32) -> usize {
    if strength < MIN_STRENGTH {
        return 0;
    }
    let d = decay(strength);
    let needed = (FAINTEST.ln() / d.ln()).ceil();
    (needed as usize).clamp(1, MAX_FRAMES_BACK)
}

/// The current frame with earlier frames (newest first) glowing behind it. Earlier frames of
/// a different size are ignored, and alpha always comes from the current frame.
pub fn with_trail(current: &Image, earlier: &[Image], strength: f32) -> Image {
    let d = decay(strength);
    let layers: Vec<(&[[f32; 4]], [f32; 3])> = earlier
        .iter()
        .take(MAX_FRAMES_BACK)
        .enumerate()
        .filter(|(_, frame)| frame.width() == current.width() && frame.height() == current.height())
        .map(|(i, frame)| {
            let age = (i + 1) as f32;
            (frame.pixels(), CHANNEL_RATE.map(|rate| d.powf(age * rate)))
        })
        .collect();
    if layers.is_empty() || d <= 0.0 {
        return current.clone();
    }
    let pixels = current
        .pixels()
        .par_iter()
        .enumerate()
        .map(|(i, now)| {
            let mut out = *now;
            for (frame, weight) in &layers {
                for c in 0..3 {
                    out[c] = out[c].max(frame[i][c] * weight[c]);
                }
            }
            out
        })
        .collect();
    Image::new(current.width(), current.height(), pixels).expect("same size as the current frame")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(width: u32, height: u32, rgba: [f32; 4]) -> Image {
        Image::new(width, height, vec![rgba; (width * height) as usize]).unwrap()
    }

    /// A white dot on black at column `x` of a 4×1 strip.
    fn dot_at(x: usize) -> Image {
        let mut px = vec![[0.0, 0.0, 0.0, 1.0]; 4];
        px[x] = [1.0, 1.0, 1.0, 1.0];
        Image::new(4, 1, px).unwrap()
    }

    #[test]
    fn a_still_picture_is_unchanged() {
        let frame = flat(3, 2, [0.4, 0.5, 0.6, 1.0]);
        let out = with_trail(&frame, &[frame.clone(), frame.clone(), frame.clone()], 1.0);
        assert_eq!(out, frame);
    }

    #[test]
    fn a_moving_dot_leaves_a_fading_warm_trail() {
        // the dot moves right one column per frame: now at 3, before at 2, 1, 0
        let out = with_trail(&dot_at(3), &[dot_at(2), dot_at(1), dot_at(0)], 1.0);
        assert_eq!(out.pixel(3, 0), [1.0, 1.0, 1.0, 1.0], "the dot itself is untouched");
        let (one, two, three) = (out.pixel(2, 0), out.pixel(1, 0), out.pixel(0, 0));
        assert!(one[1] > two[1] && two[1] > three[1] && three[1] > 0.0, "older is fainter: {one:?} {two:?} {three:?}");
        assert!(one[0] > one[1] && one[1] > one[2], "red lingers longest, blue shortest: {one:?}");
        assert_eq!(one[3], 1.0, "alpha comes from the current frame");
    }

    #[test]
    fn strength_zero_or_no_history_returns_the_frame() {
        assert_eq!(with_trail(&dot_at(3), &[dot_at(2)], 0.0), dot_at(3));
        assert_eq!(with_trail(&dot_at(3), &[], 1.0), dot_at(3));
    }

    #[test]
    fn frames_of_another_size_are_ignored() {
        let out = with_trail(&dot_at(3), &[flat(2, 2, [1.0; 4])], 1.0);
        assert_eq!(out, dot_at(3));
    }

    #[test]
    fn stronger_persistence_reaches_further_back() {
        assert_eq!(frames_back(0.0), 0);
        assert!(frames_back(0.05) >= 1);
        assert!(frames_back(0.05) <= frames_back(0.5));
        assert_eq!(frames_back(1.0), MAX_FRAMES_BACK);
        assert!(decay(0.2) < decay(1.0) && decay(1.0) <= DECAY_AT_FULL);
    }
}
