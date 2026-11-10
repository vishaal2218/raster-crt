//! Runs the real shaders on this machine's GPU.

use raster_engine::{Engine, Image, Settings};
use std::collections::HashMap;

/// params.json defaults (the single source of truth), with some controls overridden.
fn settings(overrides: &[(&str, f32)]) -> Settings {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../panel/params.json")).expect("params.json");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid params.json");
    let mut values: HashMap<String, f32> = doc["params"]
        .as_array()
        .expect("params list")
        .iter()
        .map(|p| {
            let d = &p["default"];
            let v = d.as_f64().map(|n| n as f32).unwrap_or_else(|| f32::from(u8::from(d.as_bool().unwrap_or(false))));
            (p["key"].as_str().expect("key").to_string(), v)
        })
        .collect();
    overrides.iter().filter(|(k, _)| *k != "time").for_each(|(k, v)| {
        assert!(values.contains_key(*k), "unknown control {k}");
        values.insert((*k).to_string(), *v);
    });
    Settings::from_lookup(|key| values[key], time_of(overrides))
}

/// A test can pass ("time", seconds) among its overrides to render another frame time.
fn time_of(overrides: &[(&str, f32)]) -> f32 {
    overrides.iter().find(|(k, _)| *k == "time").map_or(0.0, |(_, v)| *v)
}

fn flat(width: u32, height: u32, v: f32) -> Image {
    Image::new(width, height, vec![[v, v, v, 1.0]; (width * height) as usize]).unwrap()
}

fn max_diff(a: &Image, b: &Image) -> f32 {
    a.pixels().iter().zip(b.pixels()).flat_map(|(p, q)| (0..4).map(move |i| (p[i] - q[i]).abs())).fold(0.0, f32::max)
}

fn engine() -> Engine {
    Engine::new().expect("this test needs a GPU")
}

#[test]
fn all_modules_off_passes_the_picture_through() {
    let src = Image::test_pattern(320, 180);
    let out = engine().render(&src, &settings(&[])).unwrap();
    // float rounding on 1-pixel black/white lines; 1e-4 is a fortieth of an 8-bit step
    assert!(max_diff(&src, &out) < 1e-4, "diff {}", max_diff(&src, &out));
}

#[test]
fn the_same_frame_renders_the_same_bytes_every_time() {
    let mut e = engine();
    let s = settings(&[("screen_master", 1.0), ("glass_master", 1.0), ("glow", 0.6), ("spectral_bloom", 0.5)]);
    let src = Image::test_pattern(320, 240);
    let a = e.render(&src, &s).unwrap();
    let _other_size = e.render(&Image::test_pattern(200, 100), &s).unwrap();
    let b = e.render(&src, &s).unwrap();
    assert_eq!(a, b);
}

#[test]
fn curved_glass_blacks_out_the_corners() {
    let mut e = engine();
    let src = flat(320, 240, 0.8);
    let curved = settings(&[("glass_master", 1.0), ("glass_curve", 1.0), ("glow", 0.0)]);
    let out = e.render(&src, &curved).unwrap();
    assert_eq!(out.pixel(0, 0), [0.0, 0.0, 0.0, 1.0]);
    assert!(out.pixel(160, 120)[0] > 0.7, "centre stays lit");

    let transparent = settings(&[("glass_master", 1.0), ("glass_curve", 1.0), ("glow", 0.0), ("outside", 1.0)]);
    assert_eq!(e.render(&src, &transparent).unwrap().pixel(0, 0)[3], 0.0);

    let extended = settings(&[("glass_master", 1.0), ("glass_curve", 1.0), ("glow", 0.0), ("extend_edges", 1.0)]);
    assert!(e.render(&src, &extended).unwrap().pixel(0, 0)[3] > 0.99);
}

#[test]
fn screen_draws_scanlines_on_a_flat_field() {
    let mut e = engine();
    // SD raster on a 960-line frame: two output pixels per raster line
    let src = flat(1280, 960, 0.5);
    let on = e.render(&src, &settings(&[("screen_master", 1.0), ("quality", 0.0), ("phosphor", 0.0)])).unwrap();
    let column: Vec<f32> = (100..140).map(|y| on.pixel(640, y)[1]).collect();
    let spread = column.iter().cloned().fold(f32::MIN, f32::max) - column.iter().cloned().fold(f32::MAX, f32::min);
    assert!(spread > 0.1, "scanlines should modulate rows, spread {spread}");
    let mean = column.iter().sum::<f32>() / column.len() as f32;
    assert!((mean - 0.5).abs() < 0.08, "scanlines keep overall brightness, mean {mean}");
}

#[test]
fn the_mask_paints_rgb_triads_across_a_white_field() {
    let mut e = engine();
    let src = flat(1280, 720, 0.8);
    let s = settings(&[("screen_master", 1.0), ("quality", 2.0), ("mask_type", 1.0), ("phosphor", 1.0), ("mask_scale", 1.0)]);
    let out = e.render(&src, &s).unwrap();
    let reds: Vec<f32> = (600..620).map(|x| out.pixel(x, 360)[0]).collect();
    let spread = reds.iter().cloned().fold(f32::MIN, f32::max) - reds.iter().cloned().fold(f32::MAX, f32::min);
    assert!(spread > 0.3, "aperture grille should stripe the red channel, spread {spread}");
}

#[test]
fn glow_spreads_light_around_a_bright_spot() {
    let mut e = engine();
    let (w, h) = (320, 240);
    let mut pixels = vec![[0.0, 0.0, 0.0, 1.0]; (w * h) as usize];
    for y in 115..125 {
        for x in 155..165 {
            pixels[(y * w + x) as usize] = [1.0, 1.0, 1.0, 1.0];
        }
    }
    let src = Image::new(w, h, pixels).unwrap();
    let glowing = e.render(&src, &settings(&[("glass_master", 1.0), ("glass_curve", 0.0), ("glow", 1.0)])).unwrap();
    assert!(glowing.pixel(175, 120)[0] > 0.01, "light should reach past the spot");
    let none = e.render(&src, &settings(&[("glass_master", 1.0), ("glass_curve", 0.0), ("glow", 0.0)])).unwrap();
    assert!(none.pixel(175, 120)[0] < 1e-6);
}

#[test]
fn dry_wet_zero_returns_the_source_with_every_module_on() {
    let src = Image::test_pattern(320, 180);
    let s = settings(&[("mix", 0.0), ("screen_master", 1.0), ("glass_master", 1.0), ("glow", 1.0)]);
    let out = engine().render(&src, &s).unwrap();
    assert!(max_diff(&src, &out) < 1e-6);
}


fn row_spread(img: &Image, x: u32, ys: std::ops::Range<u32>, channel: usize) -> f32 {
    let v: Vec<f32> = ys.map(|y| img.pixel(x, y)[channel]).collect();
    v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
}

fn all_finite(img: &Image) -> bool {
    img.pixels().iter().all(|p| p.iter().all(|v| v.is_finite()))
}

#[test]
fn colour_signal_can_drain_the_colour() {
    let src = Image::test_pattern(640, 360);
    let grey = engine().render(&src, &settings(&[("colour_master", 1.0), ("chroma_level", 0.0), ("clean_signal", 0.0), ("hue_drift", 0.0), ("chroma_bleed", 0.0), ("rgb_split", 0.0), ("halation", 0.0)])).unwrap();
    let p = grey.pixel(400, 100); // inside the magenta-ish bars
    assert!((p[0] - p[1]).abs() < 0.02 && (p[1] - p[2]).abs() < 0.02, "chroma 0 should be grey, got {p:?}");
}

#[test]
fn circuit_bend_changes_the_picture_without_breaking_it() {
    let mut e = engine();
    let src = Image::test_pattern(640, 360);
    let plain = e.render(&src, &settings(&[("bend_master", 1.0), ("bend", 0.0)])).unwrap();
    let bent = e.render(&src, &settings(&[("bend_master", 1.0), ("bend", 1.0), ("boost_db", 40.0), ("resonance_q", 20.0), ("frequency_mhz", 3.58), ("feedback", 0.2)])).unwrap();
    assert!(all_finite(&bent));
    assert!(max_diff(&plain, &bent) > 0.1, "a hard bend should visibly change the picture");
}

#[test]
fn damage_moves_with_time_but_repeats_for_the_same_frame() {
    let mut e = engine();
    let src = Image::test_pattern(640, 360);
    let look = [("damage_master", 1.0), ("shake", 1.0), ("tear", 1.0), ("dropout", 1.0), ("roll", 0.5)];
    let at = |e: &mut Engine, t: f32| {
        let mut o = look.to_vec();
        o.push(("time", t));
        e.render(&src, &settings(&o)).unwrap()
    };
    let a = at(&mut e, 1.0);
    let b = at(&mut e, 2.0);
    assert!(max_diff(&a, &b) > 0.1, "damage should change between frames");
    assert_eq!(a, at(&mut e, 1.0), "the same frame time renders the same picture");
}

#[test]
fn tape_grain_adds_texture_to_a_flat_field() {
    let out = engine().render(&flat(640, 480, 0.5), &settings(&[("noise_master", 1.0), ("tape_grain", 1.0), ("rf_specks", 0.0)])).unwrap();
    assert!(row_spread(&out, 300, 100..160, 1) > 0.02);
}

#[test]
fn waves_draw_bands_across_a_flat_field() {
    let out = engine().render(&flat(640, 480, 0.5), &settings(&[("waves_master", 1.0), ("wave_mix", 1.0), ("wave_count", 20.0), ("clean_signal", 0.0)])).unwrap();
    assert!(row_spread(&out, 300, 0..480, 1) > 0.5, "bands should swing between dark and light");
}

#[test]
fn capture_draws_a_scan_band() {
    let s = settings(&[("capture_master", 1.0), ("shutter", 500.0), ("pulse_contrast", 2.0), ("persistence", 0.0)]);
    let out = engine().render(&flat(640, 480, 0.5), &s).unwrap();
    assert!(row_spread(&out, 300, 0..480, 1) > 0.2);
}

#[test]
fn beam_misalignment_pulls_red_and_blue_apart_at_an_edge() {
    let (w, h) = (640u32, 480u32);
    let pixels = (0..w * h).map(|i| if i % w < w / 2 { [0.0, 0.0, 0.0, 1.0] } else { [1.0, 1.0, 1.0, 1.0] }).collect();
    let src = Image::new(w, h, pixels).unwrap();
    let out = engine().render(&src, &settings(&[("convergence_master", 1.0), ("beam_convergence", 1.0), ("convergence_angle", 0.0)])).unwrap();
    let edge: Vec<[f32; 4]> = (w / 2 - 6..w / 2 + 6).map(|x| out.pixel(x, 240)).collect();
    assert!(edge.iter().any(|p| (p[0] - p[2]).abs() > 0.3), "red and blue should split at the edge: {edge:?}");
}

#[test]
fn every_module_at_full_stays_finite() {
    let s = settings(&[
        ("colour_master", 1.0), ("bend_master", 1.0), ("damage_master", 1.0), ("noise_master", 1.0), ("waves_master", 1.0),
        ("screen_master", 1.0), ("capture_master", 1.0), ("convergence_master", 1.0), ("glass_master", 1.0),
        ("gain", 2.0), ("chroma_level", 2.5), ("boost_db", 48.0), ("resonance_q", 30.0), ("feedback", 0.25),
        ("echo", -1.0), ("fisheye", 2.0), ("spectral_bloom", 1.0), ("glow", 1.0), ("time", 3.3),
    ]);
    let out = engine().render(&Image::test_pattern(800, 450), &s).unwrap();
    assert!(all_finite(&out));
}
