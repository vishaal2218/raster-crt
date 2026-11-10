//! Frames as the engine sees them: RGBA, f32 per channel, rows top to bottom,
//! display-referred (0..1 nominal; 32-bit float sources may go past 1).

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageError {
    Empty,
    SizeMismatch { expected: usize, got: usize },
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::Empty => f.write_str("an image needs at least one pixel"),
            ImageError::SizeMismatch { expected, got } => write!(f, "expected {expected} pixels, got {got}"),
        }
    }
}

impl std::error::Error for ImageError {}

impl Image {
    pub fn new(width: u32, height: u32, pixels: Vec<[f32; 4]>) -> Result<Self, ImageError> {
        let expected = width as usize * height as usize;
        if expected == 0 {
            return Err(ImageError::Empty);
        }
        if pixels.len() != expected {
            return Err(ImageError::SizeMismatch { expected, got: pixels.len() });
        }
        Ok(Self { width, height, pixels })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[[f32; 4]] {
        &self.pixels
    }

    pub fn into_pixels(self) -> Vec<[f32; 4]> {
        self.pixels
    }

    pub fn pixel(&self, x: u32, y: u32) -> [f32; 4] {
        self.pixels[y as usize * self.width as usize + x as usize]
    }

    /// A repeatable test card: colour bars on top, a grey ramp, then fine black/white lines.
    pub fn test_pattern(width: u32, height: u32) -> Self {
        const BARS: [[f32; 3]; 7] = [
            [0.75, 0.75, 0.75], [0.75, 0.75, 0.0], [0.0, 0.75, 0.75], [0.0, 0.75, 0.0],
            [0.75, 0.0, 0.75], [0.75, 0.0, 0.0], [0.0, 0.0, 0.75],
        ];
        let pixels = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| {
                let fx = (x as f32 + 0.5) / width as f32;
                let fy = (y as f32 + 0.5) / height as f32;
                let rgb = if fy < 0.6 {
                    BARS[((fx * 7.0) as usize).min(6)]
                } else if fy < 0.8 {
                    [fx, fx, fx]
                } else if (x / 2) % 2 == 0 {
                    [1.0, 1.0, 1.0]
                } else {
                    [0.0, 0.0, 0.0]
                };
                [rgb[0], rgb[1], rgb[2], 1.0]
            })
            .collect();
        Self { width, height, pixels }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_mismatched_buffers() {
        assert_eq!(Image::new(0, 4, vec![]), Err(ImageError::Empty));
        assert_eq!(Image::new(2, 2, vec![[0.0; 4]; 3]), Err(ImageError::SizeMismatch { expected: 4, got: 3 }));
    }

    #[test]
    fn test_pattern_is_repeatable_and_opaque() {
        let a = Image::test_pattern(64, 36);
        assert_eq!(a, Image::test_pattern(64, 36));
        assert!(a.pixels().iter().all(|p| p[3] == 1.0));
        assert_eq!(a.pixel(0, 0), [0.75, 0.75, 0.75, 1.0]);
    }
}
