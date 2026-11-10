//! PNG in and out (8 bits per channel), converted to and from the engine's float RGBA.

use raster_engine::Image;
use std::fs::File;
use std::io::{BufReader, BufWriter};

const MAX_8U: f32 = 255.0;

pub fn read(path: &str) -> Result<Image, String> {
    let file = File::open(path).map_err(|e| format!("can't open {path}: {e}"))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    // palettes and low bit depths expand to 8-bit channels; 16-bit drops to 8
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("{path} is not a readable PNG: {e}"))?;
    let size = reader.output_buffer_size().ok_or_else(|| format!("{path} is too large"))?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("can't decode {path}: {e}"))?;
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err(format!("{path}: palette PNG wasn't expanded")),
    };
    let pixels = buf[..info.buffer_size()]
        .chunks_exact(channels)
        .map(|c| {
            let v = |i: usize| f32::from(c[i]) / MAX_8U;
            match channels {
                1 => [v(0), v(0), v(0), 1.0],
                2 => [v(0), v(0), v(0), v(1)],
                3 => [v(0), v(1), v(2), 1.0],
                _ => [v(0), v(1), v(2), v(3)],
            }
        })
        .collect();
    Image::new(info.width, info.height, pixels).map_err(|e| format!("{path}: {e}"))
}

pub fn write(path: &str, image: &Image) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("can't create {path}: {e}"))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let bytes: Vec<u8> = image
        .pixels()
        .iter()
        .flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * MAX_8U).round() as u8))
        .collect();
    let mut writer = encoder.write_header().map_err(|e| format!("can't write {path}: {e}"))?;
    writer.write_image_data(&bytes).map_err(|e| format!("can't write {path}: {e}"))
}
