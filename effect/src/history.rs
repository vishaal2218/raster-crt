//! The effect's own input at earlier frames, for the persistence trail
//! (raster_engine::persistence). Frames come from the host's checkout callback; a frame it
//! won't give, such as one before the clip starts, ends the list early and is logged once.

use crate::frames::{self, Layout};
use crate::{depth_of, layout_of, log};
use after_effects as ae;
use raster_engine::Image;

/// Parameter 0 is always the layer the effect is applied to.
const INPUT_LAYER: i32 = 0;

/// Up to `count` earlier frames, newest first, all the size of `current` (this frame's layout).
pub fn earlier_frames(in_data: &ae::InData, current: Layout, count: usize) -> Vec<Image> {
    // time_step is negative on reversed clips, so subtracting it still steps to the frame
    // that was on screen just before this one
    let step = in_data.time_step();
    if step == 0 {
        log::once("persistence: the host gave no frame duration, so there is no trail");
        return Vec::new();
    }
    let mut found = Vec::with_capacity(count);
    for back in 1..=count as i32 {
        let Some(time) = step.checked_mul(back).and_then(|offset| in_data.current_time().checked_sub(offset)) else {
            break;
        };
        match frame_at(in_data, time, step, current) {
            Ok(frame) => found.push(frame),
            Err(reason) => {
                log::once(format!("persistence: no input {back} frame(s) back: {reason}"));
                break;
            }
        }
    }
    if found.len() == count {
        log::once("persistence: the host is giving earlier frames");
    }
    found
}

fn frame_at(in_data: &ae::InData, time: i32, step: i32, current: Layout) -> Result<Image, String> {
    let interact = in_data.interact();
    let mut def = interact
        .checkout_param(INPUT_LAYER, time, step, in_data.time_scale())
        .map_err(|e| format!("the host refused the checkout ({e:?})"))?;
    let frame = read_layer(in_data, &mut def, current);
    if let Err(e) = interact.checkin_param(&def) {
        log::once(format!("persistence: the host refused a checkin ({e:?})"));
    }
    frame
}

fn read_layer(in_data: &ae::InData, def: &mut ae::sys::PF_ParamDef, current: Layout) -> Result<Image, String> {
    // Read the layer member directly: hosts don't all fill in param_type for a checked-out
    // input. Parameter 0 is the input layer, so `ld` is the member the host wrote, and it
    // stays valid until the caller checks the parameter back in.
    let layer_def = &raw mut def.u.ld;
    // SAFETY: `layer_def` points into `def`, which is alive and initialised by the host
    if unsafe { (*layer_def).data.is_null() } {
        return Err("the host gave an empty frame".into());
    }
    let layer = ae::Layer::from_raw(layer_def, in_data, None);
    let depth = if in_data.is_premiere() {
        // Premiere hands earlier frames back as ARGB, not the BGRA the effect asked for (seen
        // on 26.5); a frame that won't say is read like the current one
        match layer.pr_pixel_format() {
            Ok(format) => depth_of(format).ok_or_else(|| format!("its pixel format {format:?} isn't one the engine reads"))?,
            Err(_) => current.depth,
        }
    } else {
        current.depth
    };
    let layout = layout_of(&layer, depth);
    if (layout.width, layout.height) != (current.width, current.height) {
        return Err(format!("it is {}x{}, not {}x{}", layout.width, layout.height, current.width, current.height));
    }
    let pixels = frames::read(layer.buffer(), layout).ok_or("it is smaller than its size says")?;
    Image::new(layout.width as u32, layout.height as u32, pixels).map_err(|e| e.to_string())
}
