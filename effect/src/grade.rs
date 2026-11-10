//! Per-channel grading for the M0 effect: contrast around mid-grey, a brightness offset,
//! then a Dry/Wet blend with the source. Values are 0-1, display-referred. The CRT
//! pipeline replaces this module in M1; the row helpers stay as the CPU reference.

pub const MAX_8U: f32 = 255.0;
/// After Effects' 16-bit channels top out at 32768, not 65535.
pub const MAX_16U: f32 = 32768.0;

const BYTES_8U: usize = 4;
const BYTES_32F: usize = 16;
const FLOAT_BYTES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub mix: f32,
    pub brightness: f32,
    pub contrast: f32,
}

impl Look {
    #[cfg(test)]
    pub const NEUTRAL: Look = Look { mix: 1.0, brightness: 0.0, contrast: 1.0 };

    /// True when the look can't change a pixel, so it must pass through bit for bit.
    pub fn is_identity(&self) -> bool {
        self.mix == 0.0 || (self.brightness == 0.0 && self.contrast == 1.0)
    }

    pub fn apply(&self, v: f32) -> f32 {
        if self.is_identity() {
            return v;
        }
        let graded = (v - 0.5) * self.contrast + 0.5 + self.brightness;
        v + (graded - v) * self.mix
    }
}

pub fn channel_8u(v: u8, look: Look) -> u8 {
    (look.apply(f32::from(v) / MAX_8U).clamp(0.0, 1.0) * MAX_8U).round() as u8
}

pub fn channel_16u(v: u16, look: Look) -> u16 {
    (look.apply(f32::from(v) / MAX_16U).clamp(0.0, 1.0) * MAX_16U).round() as u16
}

/// Copies the row unchanged when the look is neutral. Returns whether it did.
fn copy_if_identity(src: &[u8], dst: &mut [u8], look: Look) -> bool {
    if !look.is_identity() {
        return false;
    }
    let n = dst.len().min(src.len());
    dst[..n].copy_from_slice(&src[..n]);
    true
}

/// One row of 8-bit BGRA pixels. Alpha (byte 3) passes through untouched.
pub fn row_8u(src: &[u8], dst: &mut [u8], look: Look) {
    if copy_if_identity(src, dst, look) {
        return;
    }
    for (out, inp) in dst.chunks_exact_mut(BYTES_8U).zip(src.chunks_exact(BYTES_8U)) {
        out[0] = channel_8u(inp[0], look);
        out[1] = channel_8u(inp[1], look);
        out[2] = channel_8u(inp[2], look);
        out[3] = inp[3];
    }
}

/// One row of 32-bit float BGRA pixels (16 bytes each, native byte order).
/// Float values are not clamped, so super-whites survive.
pub fn row_32f(src: &[u8], dst: &mut [u8], look: Look) {
    if copy_if_identity(src, dst, look) {
        return;
    }
    for (out, inp) in dst.chunks_exact_mut(BYTES_32F).zip(src.chunks_exact(BYTES_32F)) {
        for channel in 0..3 {
            let k = channel * FLOAT_BYTES;
            let v = f32::from_ne_bytes([inp[k], inp[k + 1], inp[k + 2], inp[k + 3]]);
            out[k..k + FLOAT_BYTES].copy_from_slice(&look.apply(v).to_ne_bytes());
        }
        out[12..16].copy_from_slice(&inp[12..16]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-6;

    #[test]
    fn neutral_look_returns_input() {
        for v in [0.0, 0.25, 0.5, 1.0] {
            assert!((Look::NEUTRAL.apply(v) - v).abs() < EPS);
        }
    }

    #[test]
    fn zero_mix_returns_input_whatever_the_grade() {
        let look = Look { mix: 0.0, brightness: 0.2, contrast: 2.0 };
        assert!((look.apply(0.3) - 0.3).abs() < EPS);
    }

    #[test]
    fn brightness_offsets_every_level() {
        let look = Look { brightness: 0.1, ..Look::NEUTRAL };
        assert!((look.apply(0.4) - 0.5).abs() < EPS);
    }

    #[test]
    fn contrast_pivots_around_mid_grey() {
        let look = Look { contrast: 2.0, ..Look::NEUTRAL };
        assert!((look.apply(0.5) - 0.5).abs() < EPS);
        assert!((look.apply(0.75) - 1.0).abs() < EPS);
    }

    #[test]
    fn half_mix_lands_halfway() {
        let look = Look { mix: 0.5, brightness: 0.2, contrast: 1.0 };
        assert!((look.apply(0.4) - 0.5).abs() < EPS);
    }

    #[test]
    fn eight_bit_clamps_instead_of_wrapping() {
        let look = Look { brightness: 0.25, ..Look::NEUTRAL };
        assert_eq!(channel_8u(250, look), 255);
        let dark = Look { brightness: -0.25, ..Look::NEUTRAL };
        assert_eq!(channel_8u(10, dark), 0);
    }

    #[test]
    fn sixteen_bit_uses_after_effects_range() {
        assert_eq!(channel_16u(32768, Look::NEUTRAL), 32768);
        assert_eq!(channel_16u(16384, Look::NEUTRAL), 16384);
    }

    #[test]
    fn row_8u_keeps_alpha_and_grades_colour() {
        let src = [100, 150, 200, 77];
        let mut dst = [0u8; 4];
        row_8u(&src, &mut dst, Look { brightness: 0.2, ..Look::NEUTRAL });
        assert_eq!(dst[3], 77);
        assert_eq!(dst[0], 151); // 100 + 0.2 × 255
    }

    #[test]
    fn row_32f_neutral_is_a_bit_exact_copy() {
        let pixel: Vec<u8> = [0.1f32, 0.6, 1.4, 0.5].iter().flat_map(|v| v.to_ne_bytes()).collect();
        let mut dst = vec![0u8; 16];
        row_32f(&pixel, &mut dst, Look::NEUTRAL);
        assert_eq!(dst, pixel);
    }
}
