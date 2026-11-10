//! Raster CRT effect for Premiere Pro and After Effects.
//!
//! All 80 controls from panel/params.json in 10 twirl-down groups (see params.rs).
//! In Premiere and After Effects, frames with any module turned up render through the GPU
//! engine (raster_engine); with every module off only the CPU finish runs, bit-exact.
//! With Persistence on and Capture engaged, up to three earlier frames are fetched from the
//! host for the phosphor trail.

use after_effects as ae;
use rayon::prelude::*;
use raster_engine::{persistence, Engine, Image, Settings};
use std::sync::{Mutex, OnceLock};

mod frames;
mod grade;
mod history;
mod log;
mod params;
use frames::Depth;
use grade::Look;
use params::Control;

/// One GPU engine for the whole process; Premiere renders on several threads, so it sits
/// behind a lock. A failed start is remembered and the CPU finish is used instead.
static ENGINE: OnceLock<Result<Mutex<Engine>, String>> = OnceLock::new();

fn engine() -> Result<&'static Mutex<Engine>, String> {
    ENGINE
        .get_or_init(|| Engine::new().map(Mutex::new).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(Clone::clone)
}

const BYTES_PER_PIXEL_8U: usize = 4;
const BYTES_PER_PIXEL_32F: usize = 16;

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Control);

fn declare_premiere_formats(in_data: &ae::InData) -> Result<(), ae::Error> {
    let suite = ae::pf::suites::PixelFormat::new()?;
    suite.clear_supported_pixel_formats(in_data.effect_ref())?;
    for format in [ae::pr::PixelFormat::Bgra4444_32f, ae::pr::PixelFormat::Bgra4444_8u] {
        suite.add_supported_pixel_format(in_data.effect_ref(), format)?;
    }
    Ok(())
}

fn depth_of(format: ae::pr::PixelFormat) -> Option<Depth> {
    match format {
        ae::pr::PixelFormat::Bgra4444_8u => Some(Depth::U8),
        ae::pr::PixelFormat::Bgra4444_32f => Some(Depth::F32),
        // earlier frames for Persistence arrive in ARGB, though the effect asks for BGRA
        ae::pr::PixelFormat::Argb4444_8u => Some(Depth::Ae8),
        ae::pr::PixelFormat::Argb4444_16u => Some(Depth::Ae16),
        ae::pr::PixelFormat::Argb4444_32f => Some(Depth::Argb32),
        _ => None,
    }
}

fn layout_of(layer: &ae::Layer, depth: Depth) -> frames::Layout {
    frames::Layout {
        stride: layer.buffer_stride(),
        width: layer.width() as usize,
        height: layer.height() as usize,
        depth,
        bottom_up: layer.row_bytes() < 0,
    }
}

/// The pixel layout of a frame, in either host. None for formats the engine doesn't take.
fn depth_for(in_data: &ae::InData, layer: &ae::Layer) -> Option<Depth> {
    if in_data.is_premiere() {
        return layer.pr_pixel_format().ok().and_then(depth_of);
    }
    match layer.bit_depth() {
        8 => Some(Depth::Ae8),
        16 => Some(Depth::Ae16),
        _ => None,
    }
}

/// The CRT look through the GPU engine. Any failure is returned so the caller can fall back.
fn render_gpu(in_data: &ae::InData, in_layer: &ae::Layer, out_layer: &mut ae::Layer, settings: &Settings) -> Result<(), String> {
    if (in_layer.width(), in_layer.height()) != (out_layer.width(), out_layer.height()) {
        return Err("input and output frames differ in size".into());
    }
    let depth = depth_for(in_data, out_layer).ok_or("unsupported pixel format")?;
    let (input, output) = (layout_of(in_layer, depth), layout_of(out_layer, depth));
    let pixels = frames::read(in_layer.buffer(), input).ok_or("input frame is smaller than expected")?;
    let source = Image::new(input.width as u32, input.height as u32, pixels).map_err(|e| e.to_string())?;
    // earlier frames are fetched before taking the engine lock: fetching one can render
    // another Raster CRT further up the same clip
    let trail = settings.persistence_trail();
    let source = match persistence::frames_back(trail) {
        0 => source,
        back => persistence::with_trail(&source, &history::earlier_frames(in_data, input, back), trail),
    };
    let rendered = {
        let mut engine = engine()?.lock().map_err(|_| "GPU engine lock was poisoned".to_string())?;
        engine.render(&source, settings).map_err(|e| e.to_string())?
    };
    if !frames::write(rendered.pixels(), out_layer.buffer_mut(), output) {
        return Err("output frame is smaller than expected".into());
    }
    Ok(())
}

/// Premiere hands us BGRA rows (8-bit or 32-bit float); grade them in parallel.
fn render_premiere(in_layer: &ae::Layer, out_layer: &mut ae::Layer, look: Look) -> Result<(), ae::Error> {
    let width = out_layer.width() as usize;
    let height = out_layer.height() as usize;
    let format = out_layer.pr_pixel_format()?;
    let (bytes, row_fn): (usize, fn(&[u8], &mut [u8], Look)) = match format {
        ae::pr::PixelFormat::Bgra4444_8u => (BYTES_PER_PIXEL_8U, grade::row_8u),
        ae::pr::PixelFormat::Bgra4444_32f => (BYTES_PER_PIXEL_32F, grade::row_32f),
        _ => return Err(ae::Error::InvalidParms),
    };
    let row_bytes = width * bytes;
    let in_stride = in_layer.buffer_stride();
    let src = in_layer.buffer();
    let out_stride = out_layer.buffer_stride();
    let dst = out_layer.buffer_mut();

    dst.par_chunks_mut(out_stride).take(height).enumerate().for_each(|(y, out_row)| {
        let start = y * in_stride;
        if let (Some(inp), Some(out)) = (src.get(start..start + row_bytes), out_row.get_mut(..row_bytes)) {
            row_fn(inp, out, look);
        }
    });
    Ok(())
}

/// After Effects path (8 and 16 bits per channel) through the host's pixel iterator.
fn render_after_effects(in_data: &ae::InData, in_layer: &ae::Layer, out_layer: &mut ae::Layer, look: Look) -> Result<(), ae::Error> {
    let extent = in_data.extent_hint();
    in_layer.iterate_with(out_layer, 0, extent.height(), Some(extent), |_x: i32, _y: i32, pixel: ae::GenericPixel, out_pixel: ae::GenericPixelMut| -> Result<(), ae::Error> {
        match (pixel, out_pixel) {
            (ae::GenericPixel::Pixel8(p), ae::GenericPixelMut::Pixel8(o)) => {
                o.alpha = p.alpha;
                o.red = grade::channel_8u(p.red, look);
                o.green = grade::channel_8u(p.green, look);
                o.blue = grade::channel_8u(p.blue, look);
            }
            (ae::GenericPixel::Pixel16(p), ae::GenericPixelMut::Pixel16(o)) => {
                o.alpha = p.alpha;
                o.red = grade::channel_16u(p.red, look);
                o.green = grade::channel_16u(p.green, look);
                o.blue = grade::channel_16u(p.blue, look);
            }
            _ => return Err(ae::Error::BadCallbackParameter),
        }
        Ok(())
    })
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, params: &mut ae::Parameters<Control>, _: ae::InData, _: ae::OutData) -> Result<(), Error> {
        params::register(params)
    }

    fn handle_command(&mut self, cmd: ae::Command, in_data: ae::InData, mut out_data: ae::OutData, params: &mut ae::Parameters<Control>) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg("Raster CRT 0.1.0\rA free analog video and CRT screen effect.");
            }
            ae::Command::GlobalSetup => {
                if in_data.is_premiere() {
                    declare_premiere_formats(&in_data)?;
                }
            }
            ae::Command::UserChangedParam { param_index } => {
                // choosing a factory look in the Preset popup sets every other control to it
                if params.index(Control::Preset) == Some(param_index) {
                    let chosen = params.get(Control::Preset)?.as_popup()?.value() - 1;
                    if params::apply_preset(params, usize::try_from(chosen).unwrap_or(0))? {
                        out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
                    }
                }
            }
            ae::Command::Render { in_layer, mut out_layer } => {
                let values = params::read_values(params)?;
                let look = Look {
                    mix: values.get(Control::Mix),
                    brightness: values.get(Control::Brightness),
                    contrast: values.get(Control::Contrast),
                };
                let time = in_data.current_time() as f32 / in_data.time_scale().max(1) as f32;
                let settings = Settings::from_lookup(|key| values.get_by_key(key), time);
                let gpu = if settings.modules_off() { Err(String::new()) } else { render_gpu(&in_data, &in_layer, &mut out_layer, &settings) };
                if let Err(reason) = gpu {
                    if !reason.is_empty() {
                        log::once(format!("GPU render failed, using the CPU finish: {reason}"));
                    }
                    if in_data.is_premiere() {
                        render_premiere(&in_layer, &mut out_layer, look)?;
                    } else {
                        render_after_effects(&in_data, &in_layer, &mut out_layer, look)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
