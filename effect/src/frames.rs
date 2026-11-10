//! Host pixel rows to and from the engine's RGBA floats: Premiere's BGRA (8-bit or 32-bit
//! float) and After Effects' ARGB (8-bit or 16-bit, where 16-bit tops out at 32768). Premiere
//! hands earlier frames for Persistence over as ARGB too (checked live on 26.5), also in float.

use rayon::prelude::*;

pub const BYTES_8U: usize = 4;
pub const BYTES_16U: usize = 8;
pub const BYTES_32F: usize = 16;
const MAX_8U: f32 = 255.0;
const MAX_16U: f32 = 32768.0;
const FLOAT_BYTES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    /// Premiere 8-bit, stored B G R A
    U8,
    /// Premiere 32-bit float, stored B G R A
    F32,
    /// After Effects 8-bit (and Premiere's ARGB 8-bit), stored A R G B
    Ae8,
    /// After Effects 16-bit (and Premiere's ARGB 16-bit), stored A R G B, 0..32768
    Ae16,
    /// Premiere's ARGB 32-bit float, stored A R G B (earlier frames come this way)
    Argb32,
}

impl Depth {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Depth::U8 | Depth::Ae8 => BYTES_8U,
            Depth::Ae16 => BYTES_16U,
            Depth::F32 | Depth::Argb32 => BYTES_32F,
        }
    }
}

/// How a host buffer is laid out. Rows may be stored bottom-up (a negative row stride).
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub stride: usize,
    pub width: usize,
    pub height: usize,
    pub depth: Depth,
    pub bottom_up: bool,
}

impl Layout {
    fn row_start(&self, y: usize) -> usize {
        let stored = if self.bottom_up { self.height - 1 - y } else { y };
        stored * self.stride
    }

    fn row_bytes(&self) -> usize {
        self.width * self.depth.bytes_per_pixel()
    }

    fn fits(&self, len: usize) -> bool {
        self.stride >= self.row_bytes() && len >= self.stride * self.height.saturating_sub(1) + self.row_bytes()
    }
}

fn float_at(bytes: &[u8], i: usize) -> f32 {
    let at = i * FLOAT_BYTES;
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn u16_at(bytes: &[u8], i: usize) -> f32 {
    f32::from(u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]])) / MAX_16U
}

fn pixel_in(depth: Depth, px: &[u8]) -> [f32; 4] {
    match depth {
        Depth::U8 => [px[2], px[1], px[0], px[3]].map(|v| f32::from(v) / MAX_8U),
        Depth::F32 => [float_at(px, 2), float_at(px, 1), float_at(px, 0), float_at(px, 3)],
        Depth::Ae8 => [px[1], px[2], px[3], px[0]].map(|v| f32::from(v) / MAX_8U),
        Depth::Ae16 => [u16_at(px, 1), u16_at(px, 2), u16_at(px, 3), u16_at(px, 0)],
        Depth::Argb32 => [float_at(px, 1), float_at(px, 2), float_at(px, 3), float_at(px, 0)],
    }
}

fn pixel_out(depth: Depth, rgba: [f32; 4], px: &mut [u8]) {
    let to_8u = |v: f32| (v.clamp(0.0, 1.0) * MAX_8U).round() as u8;
    match depth {
        Depth::U8 => [rgba[2], rgba[1], rgba[0], rgba[3]].iter().zip(px.iter_mut()).for_each(|(v, b)| *b = to_8u(*v)),
        Depth::F32 => [rgba[2], rgba[1], rgba[0], rgba[3]]
            .iter()
            .enumerate()
            .for_each(|(i, v)| px[i * FLOAT_BYTES..(i + 1) * FLOAT_BYTES].copy_from_slice(&v.to_le_bytes())),
        Depth::Ae8 => [rgba[3], rgba[0], rgba[1], rgba[2]].iter().zip(px.iter_mut()).for_each(|(v, b)| *b = to_8u(*v)),
        Depth::Ae16 => [rgba[3], rgba[0], rgba[1], rgba[2]].iter().enumerate().for_each(|(i, v)| {
            let n = (v.clamp(0.0, 1.0) * MAX_16U).round() as u16;
            px[i * 2..i * 2 + 2].copy_from_slice(&n.to_le_bytes());
        }),
        Depth::Argb32 => [rgba[3], rgba[0], rgba[1], rgba[2]]
            .iter()
            .enumerate()
            .for_each(|(i, v)| px[i * FLOAT_BYTES..(i + 1) * FLOAT_BYTES].copy_from_slice(&v.to_le_bytes())),
    }
}

/// Reads a host buffer into RGBA floats, top row first. None if the buffer is too small.
pub fn read(buffer: &[u8], layout: Layout) -> Option<Vec<[f32; 4]>> {
    if !layout.fits(buffer.len()) {
        return None;
    }
    let (width, row_bytes, bpp) = (layout.width, layout.row_bytes(), layout.depth.bytes_per_pixel());
    let mut pixels = vec![[0.0; 4]; width * layout.height];
    pixels.par_chunks_mut(width).enumerate().for_each(|(y, out)| {
        let start = layout.row_start(y);
        buffer[start..start + row_bytes].chunks_exact(bpp).zip(out.iter_mut()).for_each(|(px, o)| *o = pixel_in(layout.depth, px));
    });
    Some(pixels)
}

/// Writes RGBA floats (top row first) into a host buffer. False if the buffer is too small.
pub fn write(pixels: &[[f32; 4]], buffer: &mut [u8], layout: Layout) -> bool {
    if !layout.fits(buffer.len()) || pixels.len() < layout.width * layout.height {
        return false;
    }
    let (width, row_bytes, bpp) = (layout.width, layout.row_bytes(), layout.depth.bytes_per_pixel());
    buffer.par_chunks_mut(layout.stride).take(layout.height).enumerate().for_each(|(stored, row)| {
        let y = if layout.bottom_up { layout.height - 1 - stored } else { stored };
        let src = &pixels[y * width..(y + 1) * width];
        row[..row_bytes].chunks_exact_mut(bpp).zip(src).for_each(|(px, p)| pixel_out(layout.depth, *p, px));
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(stride: usize, width: usize, height: usize, depth: Depth) -> Layout {
        Layout { stride, width, height, depth, bottom_up: false }
    }

    #[test]
    fn eight_bit_round_trips_and_swaps_bgra_to_rgba() {
        let l = layout(3 * BYTES_8U, 2, 2, Depth::U8); // one spare pixel of padding per row
        let buffer: Vec<u8> = vec![10, 20, 30, 255, 1, 2, 3, 4, 0, 0, 0, 0, 40, 50, 60, 128, 5, 6, 7, 8, 0, 0, 0, 0];
        let px = read(&buffer, l).unwrap();
        assert_eq!(px[0], [30.0 / 255.0, 20.0 / 255.0, 10.0 / 255.0, 1.0]);
        let mut back = vec![0u8; buffer.len()];
        assert!(write(&px, &mut back, l));
        assert_eq!(&back[..8], &buffer[..8]);
        assert_eq!(&back[12..20], &buffer[12..20]);
    }

    #[test]
    fn float_keeps_values_past_one() {
        let rgba = [[1.5, -0.25, 0.5, 1.0]];
        let l = layout(BYTES_32F, 1, 1, Depth::F32);
        let mut buffer = vec![0u8; BYTES_32F];
        assert!(write(&rgba, &mut buffer, l));
        assert_eq!(read(&buffer, l).unwrap(), rgba);
    }

    #[test]
    fn after_effects_argb_round_trips_at_8_and_16_bits() {
        let rgba = [[0.2, 0.4, 0.6, 1.0]];
        for depth in [Depth::Ae8, Depth::Ae16] {
            let l = layout(depth.bytes_per_pixel(), 1, 1, depth);
            let mut buffer = vec![0u8; depth.bytes_per_pixel()];
            assert!(write(&rgba, &mut buffer, l));
            let back = read(&buffer, l).unwrap()[0];
            assert!(back.iter().zip(rgba[0]).all(|(a, b)| (a - b).abs() < 0.003), "{depth:?}: {back:?}");
        }
        let mut b8 = vec![0u8; 4];
        write(&rgba, &mut b8, layout(4, 1, 1, Depth::Ae8));
        assert_eq!(b8[0], 255, "alpha comes first in After Effects pixels");
    }

    #[test]
    fn bottom_up_buffers_read_top_row_first() {
        let l = Layout { stride: BYTES_8U, width: 1, height: 2, depth: Depth::U8, bottom_up: true };
        let buffer = vec![0, 0, 255, 255, 255, 0, 0, 255]; // stored: bottom row red, top row blue
        let px = read(&buffer, l).unwrap();
        assert_eq!(px[0], [0.0, 0.0, 1.0, 1.0], "the top row is the last one stored");
        let mut back = vec![0u8; 8];
        write(&px, &mut back, l);
        assert_eq!(back, buffer);
    }

    #[test]
    fn eight_bit_output_clamps_instead_of_wrapping() {
        let mut buffer = vec![0u8; BYTES_8U];
        write(&[[1.7, -0.2, 0.5, 1.0]], &mut buffer, layout(BYTES_8U, 1, 1, Depth::U8));
        assert_eq!(buffer, vec![128, 0, 255, 255]);
    }

    #[test]
    fn premiere_argb_float_round_trips_with_alpha_first() {
        let rgba = [[1.25, 0.5, -0.1, 0.75]];
        let l = layout(BYTES_32F, 1, 1, Depth::Argb32);
        let mut buffer = vec![0u8; BYTES_32F];
        assert!(write(&rgba, &mut buffer, l));
        assert_eq!(f32::from_le_bytes(buffer[..4].try_into().unwrap()), 0.75, "alpha is stored first");
        assert_eq!(read(&buffer, l).unwrap(), rgba);
    }

    #[test]
    fn refuses_buffers_that_are_too_small() {
        assert!(read(&[0u8; 7], layout(BYTES_8U, 2, 1, Depth::U8)).is_none());
        assert!(!write(&[[0.0; 4]; 2], &mut [0u8; 4], layout(BYTES_8U, 2, 1, Depth::U8)));
    }
}
