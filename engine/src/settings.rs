//! What one frame renders with, already scaled by each module's amount knob.
//!
//! Masters blend their module in from neutral: an amount (0 = off) becomes amount x master,
//! a control with a neutral point other than 0 (Signal Gain, Chroma Level) moves from that
//! neutral toward its setting. Sizes, shapes and types (mask scale, frequency, mask type...)
//! are never scaled; at master 0 the whole module does nothing.

use bytemuck::{Pod, Zeroable};

/// Processing raster heights for Quality SD and HI. FULL keeps the source height.
const SD_LINES: u32 = 480;
const HI_LINES: u32 = 720;
/// Mask triad width as a fraction of frame height, at Mask Scale 0 and 1.
const MASK_PITCH_FINE: f32 = 1.0 / 540.0;
const MASK_PITCH_COARSE: f32 = 1.0 / 135.0;
/// Beam profile width (in raster lines) at Beam Softness 0 and 1.
const BEAM_SIGMA_SHARP: f32 = 0.22;
const BEAM_SIGMA_SOFT: f32 = 0.55;
/// The raster softening fades in over the first part of a module's amount, so turning a
/// module up from 0 never jumps.
const RASTER_FADE_IN: f32 = 0.2;
/// Scanlines need about two output pixels per raster line to draw without moire.
const SCAN_MIN_PX: f32 = 1.0;
const SCAN_FULL_PX: f32 = 2.0;
/// Glow works at a quarter of the frame size; its radius is a fraction of frame height.
const GLOW_DIVISOR: u32 = 4;
const GLOW_RADIUS_OF_HEIGHT: f32 = 0.06;
const FLICKER_DEPTH: f32 = 0.08;
/// NTSC: a 52.6 us active line carrying a 3.579545 MHz colour subcarrier.
const ACTIVE_LINE_US: f32 = 52.6;
const SUBCARRIER_MHZ: f32 = 3.579_545;
/// Highest frequency a raster sample can carry without folding back (cycles per sample).
const MAX_CYCLES_PER_SAMPLE: f32 = 0.45;
const MIN_CYCLES_PER_SAMPLE: f32 = 0.002;
/// The composite subcarrier stays clearly below Nyquist even on tiny previews.
const MAX_SUBCARRIER_CYCLES: f32 = 0.42;
const IRE_PER_UNIT: f32 = 100.0;
/// Capture bands keep this much contrast even when the shutter matches the refresh.
const SHUTTER_FLATTEN: f32 = 0.7;
/// A hard enough resonance rings and locks onto the subcarrier, tinting the whole picture.
const RING_BOOST_DB: (f32, f32) = (4.0, 24.0);
const RING_Q: (f32, f32) = (2.0, 12.0);

/// Every control the engine reads, by params.json key (Preset is the panel's business).
pub const ENGINE_KEYS: [&str; 79] = [
    "mix", "brightness", "contrast", "clean_signal", "quality",
    "colour_master", "gain", "chroma_level", "hue_drift", "chroma_bleed", "rgb_split", "sharpen", "signal_glow", "halation",
    "bend_master", "bend", "bend_motion", "frequency_mhz", "resonance_q", "boost_db", "clip_low_ire", "clip_high_ire", "feedback", "feedback_delay_us",
    "damage_master", "shake", "tear", "roll", "flashes", "dropout", "dropout_style", "hum_bar", "echo", "head_switch", "top_line",
    "noise_master", "rf_specks", "speck_size", "tape_grain",
    "waves_master", "wave_mix", "wave_count", "wave_react", "wave_smooth", "wave_orientation", "wave_rgb", "wave_backward", "wave_blend",
    "screen_master", "mask_type", "mask_scale", "phosphor", "refresh_hz", "persistence", "scan_sync", "supply_ripple", "flicker", "beam_softness", "phosphor_bloom", "overscan",
    "capture_master", "shutter", "pulse_contrast", "band_angle", "sensor_readout", "moire", "rainbow",
    "convergence_master", "beam_convergence", "convergence_angle", "phosphor_convergence",
    "glass_master", "glow", "spectral_bloom", "glass_curve", "line_stretch", "fisheye", "outside", "extend_edges",
];

const SIGNAL_MASTERS: [&str; 5] = ["colour_master", "bend_master", "damage_master", "noise_master", "waves_master"];
const ALL_MASTERS: [&str; 9] = [
    "colour_master", "bend_master", "damage_master", "noise_master", "waves_master",
    "screen_master", "capture_master", "convergence_master", "glass_master",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Sd,
    Hi,
    Full,
}

impl Quality {
    fn from_index(i: f32) -> Self {
        match i.round() as i32 {
            0 => Quality::Sd,
            2 => Quality::Full,
            _ => Quality::Hi,
        }
    }

    /// Raster lines for a source this tall (never more than the source has).
    pub fn lines(self, source_height: u32) -> u32 {
        let lines = match self {
            Quality::Sd => SD_LINES,
            Quality::Hi => HI_LINES,
            Quality::Full => source_height,
        };
        lines.min(source_height).max(1)
    }
}

/// One frame's control values plus the frame time.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    values: [f32; ENGINE_KEYS.len()],
    time_seconds: f32,
}

impl Settings {
    /// `value(key)` returns a control's value by params.json key (popups as 0-based index,
    /// checkboxes as 0 or 1, angles in degrees).
    pub fn from_lookup(value: impl Fn(&str) -> f32, time_seconds: f32) -> Self {
        Self { values: ENGINE_KEYS.map(|k| value(k)), time_seconds }
    }

    fn raw(&self, key: &str) -> f32 {
        let i = ENGINE_KEYS.iter().position(|k| *k == key).unwrap_or_else(|| panic!("engine has no control '{key}'"));
        self.values[i]
    }

    fn master(&self, key: &str) -> f32 {
        self.raw(key).clamp(0.0, 1.0)
    }

    /// An amount (0 = off) scaled by its module's master.
    fn amount(&self, key: &str, master: &str) -> f32 {
        self.raw(key) * self.master(master)
    }

    /// A control with a neutral point, moved from neutral toward its setting by the master.
    fn toward(&self, key: &str, neutral: f32, master: &str) -> f32 {
        neutral + (self.raw(key) - neutral) * self.master(master)
    }

    fn flag(&self, key: &str) -> u32 {
        u32::from(self.raw(key) >= 0.5)
    }

    fn index(&self, key: &str) -> u32 {
        self.raw(key).round().max(0.0) as u32
    }

    /// True when no module is doing anything, so only the finish applies and the GPU can be
    /// skipped.
    pub fn modules_off(&self) -> bool {
        ALL_MASTERS.iter().all(|m| self.master(m) == 0.0)
    }

    pub fn quality(&self) -> Quality {
        Quality::from_index(self.raw("quality"))
    }

    /// How strongly earlier frames glow behind this one (0 = off, see persistence.rs). Like the
    /// other tube timing controls, Persistence only acts while Capture is engaged.
    pub fn persistence_trail(&self) -> f32 {
        if self.master("capture_master") > 0.0 && !self.modules_off() {
            self.amount("persistence", "capture_master").clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn signal_on(&self) -> bool {
        SIGNAL_MASTERS.iter().any(|m| self.master(m) > 0.0)
    }

    /// Processing raster size for a source frame.
    pub fn raster_size(&self, width: u32, height: u32) -> (u32, u32) {
        let lines = self.quality().lines(height);
        let columns = ((lines as f64) * f64::from(width) / f64::from(height)).round() as u32;
        (columns.max(1), lines)
    }

    pub(crate) fn uniforms(&self, width: u32, height: u32) -> Uniforms {
        let (ras_w, ras_h) = self.raster_size(width, height);
        let (glow_w, glow_h) = glow_size(width, height);
        let raster_driver = SIGNAL_MASTERS.iter().chain(["screen_master"].iter()).map(|m| self.master(m)).fold(0.0, f32::max);
        let samples_per_us = ras_w as f32 / ACTIVE_LINE_US;
        let capture_on = self.master("capture_master") > 0.0;
        let gated = |key: &str| if capture_on { self.amount(key, "capture_master") } else { 0.0 };
        let band = self.raw("band_angle").to_radians();
        let conv = self.raw("convergence_angle").to_radians();
        Uniforms {
            src_w: width,
            src_h: height,
            ras_w,
            ras_h,
            out_w: width,
            out_h: height,
            glow_w,
            glow_h,
            time: self.time_seconds,
            seed: frame_seed(self.time_seconds),
            mix_amount: self.raw("mix"),
            brightness: self.raw("brightness"),
            contrast: self.raw("contrast"),
            clean_signal: self.raw("clean_signal"),
            signal_amount: smoothstep(0.0, RASTER_FADE_IN, raster_driver),
            signal_on: u32::from(self.signal_on()),

            gain: self.toward("gain", 1.0, "colour_master"),
            chroma: self.toward("chroma_level", 1.0, "colour_master"),
            hue: self.amount("hue_drift", "colour_master"),
            bleed: self.amount("chroma_bleed", "colour_master"),
            split: self.amount("rgb_split", "colour_master"),
            sharpen: self.amount("sharpen", "colour_master"),
            signal_glow: self.amount("signal_glow", "colour_master"),
            halation: self.amount("halation", "colour_master"),

            bend: self.amount("bend", "bend_master"),
            bend_motion: self.raw("bend_motion"),
            bend_freq: (self.raw("frequency_mhz") / samples_per_us).clamp(MIN_CYCLES_PER_SAMPLE, MAX_CYCLES_PER_SAMPLE),
            bend_q: self.raw("resonance_q").max(0.05),
            bend_boost: self.raw("boost_db"),
            clip_low: self.raw("clip_low_ire") / IRE_PER_UNIT,
            clip_high: self.raw("clip_high_ire") / IRE_PER_UNIT,
            feedback: self.amount("feedback", "bend_master"),
            feedback_delay: (self.raw("feedback_delay_us") * samples_per_us).max(1.0),
            subcarrier: (SUBCARRIER_MHZ / samples_per_us).min(MAX_SUBCARRIER_CYCLES),
            bend_detune: self.raw("frequency_mhz") - SUBCARRIER_MHZ,
            bend_ring: self.amount("bend", "bend_master")
                * smoothstep(RING_BOOST_DB.0, RING_BOOST_DB.1, self.raw("boost_db"))
                * smoothstep(RING_Q.0, RING_Q.1, self.raw("resonance_q")),

            shake: self.amount("shake", "damage_master"),
            tear: self.amount("tear", "damage_master"),
            roll: self.amount("roll", "damage_master"),
            flashes: self.amount("flashes", "damage_master"),
            dropout: self.amount("dropout", "damage_master"),
            dropout_style: self.index("dropout_style"),
            hum: self.amount("hum_bar", "damage_master"),
            echo: self.amount("echo", "damage_master"),
            head_switch: self.amount("head_switch", "damage_master"),
            top_line: self.amount("top_line", "damage_master"),

            specks: self.amount("rf_specks", "noise_master"),
            speck_size: self.raw("speck_size"),
            grain: self.amount("tape_grain", "noise_master"),

            wave_mix: self.amount("wave_mix", "waves_master"),
            wave_count: self.raw("wave_count"),
            wave_react: self.raw("wave_react"),
            wave_smooth: self.raw("wave_smooth"),
            wave_vertical: self.flag("wave_orientation"),
            wave_rgb: self.flag("wave_rgb"),
            wave_backward: self.flag("wave_backward"),
            wave_blend: self.index("wave_blend"),

            scan_depth: self.master("screen_master") * smoothstep(SCAN_MIN_PX, SCAN_FULL_PX, height as f32 / ras_h as f32),
            mask_type: self.index("mask_type"),
            mask_pitch: height as f32 * lerp(MASK_PITCH_FINE, MASK_PITCH_COARSE, self.raw("mask_scale").clamp(0.0, 1.0)),
            mask_strength: self.amount("phosphor", "screen_master"),
            beam_sigma: lerp(BEAM_SIGMA_SHARP, BEAM_SIGMA_SOFT, self.raw("beam_softness").clamp(0.0, 1.0)),
            bloom: self.amount("phosphor_bloom", "screen_master"),
            overscan: self.amount("overscan", "screen_master"),
            flicker_gain: 1.0 - self.amount("flicker", "screen_master") * FLICKER_DEPTH * frame_noise(self.time_seconds),

            capture: self.master("capture_master"),
            shutter_duty: (self.raw("refresh_hz") / self.raw("shutter").max(1.0)).clamp(0.0, 1.0) * SHUTTER_FLATTEN,
            pulse: self.amount("pulse_contrast", "capture_master"),
            band_cos: band.cos(),
            band_sin: band.sin(),
            readout: self.raw("sensor_readout"),
            moire: self.amount("moire", "capture_master"),
            rainbow: self.amount("rainbow", "capture_master"),
            refresh: self.raw("refresh_hz"),
            persistence: self.raw("persistence"),
            scan_sync: gated("scan_sync"),
            ripple: gated("supply_ripple"),

            conv_beam: self.amount("beam_convergence", "convergence_master"),
            conv_dir_x: conv.cos(),
            conv_dir_y: conv.sin(),
            conv_phosphor: self.amount("phosphor_convergence", "convergence_master"),

            curve: self.amount("glass_curve", "glass_master"),
            line_stretch: self.amount("line_stretch", "glass_master"),
            fisheye: self.amount("fisheye", "glass_master"),
            outside_transparent: self.flag("outside"),
            extend_edges: self.flag("extend_edges"),
            glow: self.amount("glow", "glass_master"),
            spectral: self.amount("spectral_bloom", "glass_master"),
            glow_radius: glow_h as f32 * GLOW_RADIUS_OF_HEIGHT,
            ..Uniforms::zeroed()
        }
    }
}

/// One list makes both the Rust uniform struct and its WGSL twin, so they can't drift.
macro_rules! uniforms {
    ($($name:ident: $ty:ident),* $(,)?) => {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, Pod, Zeroable)]
        pub(crate) struct Uniforms {
            $(pub $name: $ty,)*
        }

        pub(crate) const UNIFORMS_WGSL: &str =
            concat!("struct Uniforms {\n", $("    ", stringify!($name), ": ", stringify!($ty), ",\n",)* "}\n");
    };
}

uniforms! {
    src_w: u32, src_h: u32, ras_w: u32, ras_h: u32,
    out_w: u32, out_h: u32, glow_w: u32, glow_h: u32,
    time: f32, seed: u32, mix_amount: f32, brightness: f32,
    contrast: f32, clean_signal: f32, signal_amount: f32, signal_on: u32,
    gain: f32, chroma: f32, hue: f32, bleed: f32,
    split: f32, sharpen: f32, signal_glow: f32, halation: f32,
    bend: f32, bend_motion: f32, bend_freq: f32, bend_q: f32,
    bend_boost: f32, clip_low: f32, clip_high: f32, feedback: f32,
    feedback_delay: f32, subcarrier: f32, shake: f32, tear: f32,
    roll: f32, flashes: f32, dropout: f32, dropout_style: u32,
    hum: f32, echo: f32, head_switch: f32, top_line: f32,
    specks: f32, speck_size: f32, grain: f32, wave_mix: f32,
    wave_count: f32, wave_react: f32, wave_smooth: f32, wave_vertical: u32,
    wave_rgb: u32, wave_backward: u32, wave_blend: u32, scan_depth: f32,
    mask_type: u32, mask_pitch: f32, mask_strength: f32, beam_sigma: f32,
    bloom: f32, overscan: f32, flicker_gain: f32, capture: f32,
    shutter_duty: f32, pulse: f32, band_cos: f32, band_sin: f32,
    readout: f32, moire: f32, rainbow: f32, refresh: f32,
    persistence: f32, scan_sync: f32, ripple: f32, conv_beam: f32,
    conv_dir_x: f32, conv_dir_y: f32, conv_phosphor: f32, curve: f32,
    line_stretch: f32, fisheye: f32, outside_transparent: u32, extend_edges: u32,
    glow: f32, spectral: f32, glow_radius: f32, bend_detune: f32,
    bend_ring: f32, pad0: f32, pad1: f32, pad2: f32,
}

/// The glow buffer's size for a frame: a quarter in each direction.
pub(crate) fn glow_size(width: u32, height: u32) -> (u32, u32) {
    ((width / GLOW_DIVISOR).max(1), (height / GLOW_DIVISOR).max(1))
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The same seed for the same frame time, so every random effect renders identically.
fn frame_seed(time_seconds: f32) -> u32 {
    ((time_seconds * 1000.0).round() as i64 as u64 & 0xFFFF_FFFF) as u32
}

/// A repeatable 0..1 value per frame time.
fn frame_noise(time_seconds: f32) -> f32 {
    let mut x = u64::from(frame_seed(time_seconds)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    pub(crate) fn defaults() -> HashMap<&'static str, f32> {
        [
            ("mix", 1.0), ("brightness", 0.0), ("contrast", 1.0), ("clean_signal", 0.3), ("quality", 1.0),
            ("colour_master", 0.0), ("gain", 1.0), ("chroma_level", 1.0), ("hue_drift", 0.1), ("chroma_bleed", 0.3),
            ("rgb_split", 0.1), ("sharpen", 0.2), ("signal_glow", 0.1), ("halation", 0.15),
            ("bend_master", 0.0), ("bend", 0.6), ("bend_motion", 0.0), ("frequency_mhz", 1.5), ("resonance_q", 4.0),
            ("boost_db", 12.0), ("clip_low_ire", -20.0), ("clip_high_ire", 120.0), ("feedback", 0.0), ("feedback_delay_us", 0.3),
            ("damage_master", 0.0), ("shake", 0.3), ("tear", 0.2), ("roll", 0.0), ("flashes", 0.1), ("dropout", 0.3),
            ("dropout_style", 0.0), ("hum_bar", 0.2), ("echo", 0.2), ("head_switch", 0.5), ("top_line", 0.3),
            ("noise_master", 0.0), ("rf_specks", 0.4), ("speck_size", 0.5), ("tape_grain", 0.4),
            ("waves_master", 0.0), ("wave_mix", 0.6), ("wave_count", 40.0), ("wave_react", 0.0), ("wave_smooth", 2.0),
            ("wave_orientation", 0.0), ("wave_rgb", 0.0), ("wave_backward", 0.0), ("wave_blend", 0.0),
            ("screen_master", 0.0), ("mask_type", 0.0), ("mask_scale", 0.4), ("phosphor", 0.5), ("refresh_hz", 59.94),
            ("persistence", 0.2), ("scan_sync", 0.0), ("supply_ripple", 0.0), ("flicker", 0.0), ("beam_softness", 0.3),
            ("phosphor_bloom", 0.2), ("overscan", 0.0),
            ("capture_master", 0.0), ("shutter", 60.0), ("pulse_contrast", 1.0), ("band_angle", 0.0), ("sensor_readout", 1.0),
            ("moire", 0.0), ("rainbow", 0.0),
            ("convergence_master", 0.0), ("beam_convergence", 0.4), ("convergence_angle", 45.0), ("phosphor_convergence", 0.0),
            ("glass_master", 0.0), ("glow", 0.2), ("spectral_bloom", 0.0), ("glass_curve", 0.2), ("line_stretch", 0.0),
            ("fisheye", 0.0), ("outside", 0.0), ("extend_edges", 0.0),
        ]
        .into_iter()
        .collect()
    }

    fn settings(overrides: &[(&str, f32)]) -> Settings {
        let mut values = defaults();
        overrides.iter().for_each(|(k, v)| {
            values.insert(k, *v);
        });
        Settings::from_lookup(|key| values[key], 0.0)
    }

    #[test]
    fn engine_keys_are_unique_and_all_have_test_defaults() {
        let set: std::collections::HashSet<_> = ENGINE_KEYS.iter().collect();
        assert_eq!(set.len(), ENGINE_KEYS.len());
        let d = defaults();
        assert!(ENGINE_KEYS.iter().all(|k| d.contains_key(k)));
    }

    #[test]
    fn masters_at_zero_switch_every_module_off() {
        let s = settings(&[]);
        assert!(s.modules_off());
        let u = s.uniforms(1920, 1080);
        assert_eq!((u.mask_strength, u.glow, u.curve, u.bend, u.grain, u.wave_mix), (0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
        assert_eq!((u.gain, u.chroma), (1.0, 1.0), "neutral controls sit at neutral");
        assert_eq!(u.signal_on, 0);
    }

    #[test]
    fn masters_blend_from_neutral_and_leave_shapes_alone() {
        let u = settings(&[("colour_master", 0.5), ("gain", 2.0), ("screen_master", 0.5), ("mask_type", 3.0)]).uniforms(1920, 1080);
        assert_eq!(u.gain, 1.5, "half way from neutral 1 to 2");
        assert_eq!(u.mask_strength, 0.25);
        assert_eq!(u.mask_type, 3);
        assert!((u.mask_pitch - 1080.0 * lerp(MASK_PITCH_FINE, MASK_PITCH_COARSE, 0.4)).abs() < 1e-4, "a size is not an amount");
        assert_eq!(u.signal_on, 1);
    }

    #[test]
    fn quality_sets_the_raster_and_never_upsamples() {
        assert_eq!(settings(&[]).raster_size(1920, 1080), (1280, 720));
        assert_eq!(settings(&[("quality", 0.0)]).raster_size(1920, 1080), (853, 480));
        assert_eq!(settings(&[("quality", 2.0)]).raster_size(1920, 1080), (1920, 1080));
        assert_eq!(settings(&[]).raster_size(640, 360), (640, 360));
    }

    #[test]
    fn sizes_follow_frame_height_so_previews_match_exports() {
        let s = settings(&[("screen_master", 1.0)]);
        let (full, half) = (s.uniforms(1920, 1080), s.uniforms(960, 540));
        assert!((full.mask_pitch / half.mask_pitch - 2.0).abs() < 1e-4);
        assert!((full.glow_radius / half.glow_radius - 2.0).abs() < 0.05);
    }

    #[test]
    fn raster_softening_fades_in_with_any_raster_module() {
        let at = |k: &str, m: f32| settings(&[(k, m)]).uniforms(1920, 1080).signal_amount;
        assert_eq!(at("screen_master", 0.0), 0.0);
        assert!(at("screen_master", 0.05) > 0.0 && at("screen_master", 0.05) < at("screen_master", 0.1));
        assert_eq!(at("damage_master", 0.5), 1.0);
        assert_eq!(at("glass_master", 1.0), 0.0, "glass alone keeps the full-resolution picture");
    }

    #[test]
    fn composite_timing_follows_the_raster_width() {
        let u = settings(&[("bend_master", 1.0), ("quality", 0.0)]).uniforms(1920, 1080);
        let fs = 853.0 / ACTIVE_LINE_US;
        assert!((u.subcarrier - SUBCARRIER_MHZ / fs).abs() < 1e-4);
        assert!((u.bend_freq - 1.5 / fs).abs() < 1e-4);
        assert_eq!(u.clip_high, 1.2);
        let tiny = settings(&[("bend_master", 1.0)]).uniforms(160, 90);
        assert!(tiny.subcarrier <= MAX_SUBCARRIER_CYCLES && tiny.bend_freq <= MAX_CYCLES_PER_SAMPLE);
    }

    #[test]
    fn only_a_hard_resonance_rings() {
        let soft = settings(&[("bend_master", 1.0)]).uniforms(1920, 1080);
        assert!(soft.bend_ring < 0.05, "default bend barely rings: {}", soft.bend_ring);
        let hard = settings(&[("bend_master", 1.0), ("bend", 1.0), ("boost_db", 48.0), ("resonance_q", 30.0), ("frequency_mhz", 3.58)]).uniforms(1920, 1080);
        assert_eq!(hard.bend_ring, 1.0);
        assert!(hard.bend_detune.abs() < 0.01);
    }

    #[test]
    fn screen_capture_controls_only_act_while_capture_is_engaged() {
        let off = settings(&[("screen_master", 1.0), ("scan_sync", 1.0), ("supply_ripple", 1.0)]).uniforms(100, 100);
        assert_eq!((off.scan_sync, off.ripple), (0.0, 0.0));
        let on = settings(&[("capture_master", 1.0), ("scan_sync", 1.0), ("supply_ripple", 1.0)]).uniforms(100, 100);
        assert_eq!((on.scan_sync, on.ripple), (1.0, 1.0));
    }

    #[test]
    fn the_persistence_trail_follows_capture() {
        assert_eq!(settings(&[("screen_master", 1.0), ("persistence", 1.0)]).persistence_trail(), 0.0);
        assert_eq!(settings(&[("capture_master", 0.5), ("persistence", 0.8)]).persistence_trail(), 0.4);
        assert_eq!(settings(&[("capture_master", 1.0), ("persistence", 0.0)]).persistence_trail(), 0.0);
    }

    #[test]
    fn random_effects_repeat_for_the_same_frame_time() {
        let s = settings(&[("screen_master", 1.0), ("flicker", 1.0)]);
        assert_eq!(s.uniforms(100, 100).seed, s.uniforms(100, 100).seed);
        let g = s.uniforms(100, 100).flicker_gain;
        assert!((1.0 - FLICKER_DEPTH..=1.0).contains(&g));
    }

    #[test]
    fn the_uniform_block_and_its_wgsl_twin_match() {
        assert_eq!(size_of::<Uniforms>() % 16, 0, "uniform blocks are 16-byte multiples");
        let fields = UNIFORMS_WGSL.lines().filter(|l| l.contains(':')).count();
        assert_eq!(fields * 4, size_of::<Uniforms>());
        assert!(UNIFORMS_WGSL.contains("    dropout_style: u32,\n"));
    }
}
