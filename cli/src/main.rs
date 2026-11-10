//! raster-cli: renders PNG frames through the Raster CRT engine, headless.
//!
//!   raster-cli card <out.png> [WIDTHxHEIGHT]
//!   raster-cli scene <out.png> [WIDTHxHEIGHT]
//!   raster-cli render <in.png> <out.png> [--params FILE] [--preset FILE] [--time SECONDS] [key=value ...]
//!   raster-cli presets <in.png> <factory.json> <out-dir> [--params FILE] [--time SECONDS]
//!   raster-cli bench <in.png> [FRAMES] [key=value ...]     (steady-state time per frame)
//!
//! Control values start at the params.json defaults, then the preset's values, then each
//! key=value on the command line (params.json keys; popups 0-based, checkboxes 0/1).

mod pngio;
mod scene;

use raster_engine::{persistence, Engine, Image, Settings};
use serde_json::Value;
use std::collections::HashMap;
use std::process::ExitCode;
use std::time::Instant;

const USAGE: &str = "usage:\n  raster-cli card <out.png> [WIDTHxHEIGHT]\n  raster-cli scene <out.png> [WIDTHxHEIGHT]\n  raster-cli render <in.png> <out.png> [--params FILE] [--preset FILE] [--time SECONDS] [key=value ...]\n  raster-cli presets <in.png> <factory.json> <out-dir> [--params FILE] [--time SECONDS]\n  raster-cli bench <in.png> [FRAMES] [key=value ...]";
const DEFAULT_PARAMS: &str = "panel/params.json";
const DEFAULT_CARD: (u32, u32) = (1920, 1080);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("card") => card(&args[1..]),
        Some("scene") => scene_cmd(&args[1..]),
        Some("render") => render(&args[1..]),
        Some("presets") => presets(&args[1..]),
        Some("bench") => bench(&args[1..]),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("raster-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let bad = || format!("size must look like 1920x1080, got '{text}'");
    let (w, h) = text.split_once('x').ok_or_else(bad)?;
    let (w, h) = (w.parse::<u32>().map_err(|_| bad())?, h.parse::<u32>().map_err(|_| bad())?);
    if w == 0 || h == 0 {
        return Err(bad());
    }
    Ok((w, h))
}

fn card(args: &[String]) -> Result<(), String> {
    let out = args.first().ok_or(USAGE)?;
    let (w, h) = args.get(1).map(|s| parse_size(s)).transpose()?.unwrap_or(DEFAULT_CARD);
    pngio::write(out, &Image::test_pattern(w, h))
}

fn scene_cmd(args: &[String]) -> Result<(), String> {
    let out = args.first().ok_or(USAGE)?;
    let (w, h) = args.get(1).map(|s| parse_size(s)).transpose()?.unwrap_or(DEFAULT_CARD);
    pngio::write(out, &scene::scene(w, h))
}

/// File-name-safe version of a preset name ("Birthday Tape II" -> "birthday-tape-ii").
fn slug(name: &str) -> String {
    name.to_lowercase().split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join("-")
}

/// Renders every factory preset over one picture, for thumbnails and side-by-side tuning.
fn presets(args: &[String]) -> Result<(), String> {
    let (input, factory_path, out_dir) = match args {
        [a, b, c, ..] => (a, b, c),
        _ => return Err(USAGE.to_string()),
    };
    let mut params = DEFAULT_PARAMS.to_string();
    let mut time = 0.0;
    let mut rest = args[3..].iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--params" => params = value.clone(),
            "--time" => time = value.parse().map_err(|_| "--time needs seconds".to_string())?,
            other => return Err(format!("unknown option {other}")),
        }
    }
    let defaults = load_values(&params, None)?;
    let factory = read_json(factory_path)?;
    let list = factory["presets"].as_array().ok_or_else(|| format!("{factory_path} has no presets list"))?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("can't create {out_dir}: {e}"))?;
    let source = pngio::read(input)?;
    let mut engine = Engine::new().map_err(|e| e.to_string())?;
    for preset in list {
        let name = preset["name"].as_str().ok_or("a preset has no name")?;
        let mut values = defaults.clone();
        for (key, v) in preset["values"].as_object().ok_or_else(|| format!("{name} has no values"))? {
            let n = number(v).ok_or_else(|| format!("{name}: '{key}' isn't a number"))?;
            if values.insert(key.clone(), n).is_none() {
                return Err(format!("{name}: unknown control '{key}'"));
            }
        }
        let settings = Settings::from_lookup(|key| values.get(key).copied().unwrap_or(0.0), time);
        let out = engine.render(&source, &settings).map_err(|e| format!("{name}: {e}"))?;
        let path = format!("{out_dir}/{}.png", slug(name));
        pngio::write(&path, &out)?;
        eprintln!("{name} -> {path}");
    }
    Ok(())
}

const BENCH_WARMUP: usize = 3;
const DEFAULT_BENCH_FRAMES: usize = 60;

/// Renders the same picture many times at advancing frame times and reports the steady-state
/// time per frame (upload, every pass and read-back), after a few warm-up frames.
fn bench(args: &[String]) -> Result<(), String> {
    let input = args.first().ok_or(USAGE)?;
    let mut rest = &args[1..];
    let frames = match rest.first().and_then(|a| a.parse::<usize>().ok()) {
        Some(n) => {
            rest = &rest[1..];
            n.max(1)
        }
        None => DEFAULT_BENCH_FRAMES,
    };
    let mut values = load_values(DEFAULT_PARAMS, None)?;
    for pair in rest {
        let (key, v) = pair.split_once('=').ok_or_else(|| format!("expected key=value, got '{pair}'"))?;
        let v: f32 = v.parse().map_err(|_| format!("'{key}' needs a number"))?;
        if values.insert(key.to_string(), v).is_none() {
            return Err(format!("unknown control '{key}'"));
        }
    }
    let source = pngio::read(input)?;
    let mut engine = Engine::new().map_err(|e| e.to_string())?;
    let mut times = Vec::with_capacity(frames);
    for i in 0..BENCH_WARMUP + frames {
        let settings = Settings::from_lookup(|key| values.get(key).copied().unwrap_or(0.0), i as f32 / 30.0);
        let started = Instant::now();
        // with Persistence on, the effect also reads earlier frames and blends them in; copies
        // of this frame stand in for them (the cost doesn't depend on what they show)
        let trail = settings.persistence_trail();
        let trailed = match persistence::frames_back(trail) {
            0 => None,
            back => Some(persistence::with_trail(&source, &vec![source.clone(); back], trail)),
        };
        engine.render(trailed.as_ref().unwrap_or(&source), &settings).map_err(|e| e.to_string())?;
        if i >= BENCH_WARMUP {
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!("{}x{} on {}: mean {:.1} ms, median {:.1} ms, slowest {:.1} ms over {} frames",
        source.width(), source.height(), engine.adapter_name(), mean, times[times.len() / 2], times[times.len() - 1], times.len());
    Ok(())
}

struct RenderArgs {
    input: String,
    output: String,
    params: String,
    preset: Option<String>,
    time: f32,
    overrides: Vec<(String, f32)>,
}

fn parse_render_args(args: &[String]) -> Result<RenderArgs, String> {
    let mut it = args.iter();
    let input = it.next().ok_or(USAGE)?.clone();
    let output = it.next().ok_or(USAGE)?.clone();
    let mut parsed = RenderArgs { input, output, params: DEFAULT_PARAMS.into(), preset: None, time: 0.0, overrides: vec![] };
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--params" => parsed.params = value()?,
            "--preset" => parsed.preset = Some(value()?),
            "--time" => parsed.time = value()?.parse().map_err(|_| "--time needs seconds".to_string())?,
            pair => {
                let (key, v) = pair.split_once('=').ok_or_else(|| format!("expected key=value, got '{pair}'"))?;
                let v: f32 = v.parse().map_err(|_| format!("'{key}' needs a number, got '{v}'"))?;
                parsed.overrides.push((key.to_string(), v));
            }
        }
    }
    Ok(parsed)
}

fn number(v: &Value) -> Option<f32> {
    v.as_f64().map(|n| n as f32).or_else(|| v.as_bool().map(|b| f32::from(u8::from(b))))
}

fn read_json(path: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("can't read {path}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{path} is not valid JSON: {e}"))
}

/// params.json defaults, then the preset's values (unknown keys are ignored with a warning).
fn load_values(params_path: &str, preset_path: Option<&str>) -> Result<HashMap<String, f32>, String> {
    let params = read_json(params_path)?;
    let mut values: HashMap<String, f32> = params["params"]
        .as_array()
        .ok_or_else(|| format!("{params_path} has no params list"))?
        .iter()
        .filter_map(|p| Some((p["key"].as_str()?.to_string(), number(&p["default"])?)))
        .collect();
    if let Some(path) = preset_path {
        let preset = read_json(path)?;
        let entries = preset["values"].as_object().ok_or_else(|| format!("{path} has no values object"))?;
        for (key, v) in entries {
            match (values.contains_key(key), number(v)) {
                (true, Some(n)) => {
                    values.insert(key.clone(), n);
                }
                _ => eprintln!("raster-cli: ignoring preset value '{key}'"),
            }
        }
    }
    Ok(values)
}

fn render(args: &[String]) -> Result<(), String> {
    let a = parse_render_args(args)?;
    let mut values = load_values(&a.params, a.preset.as_deref())?;
    for (key, v) in a.overrides {
        if !values.contains_key(&key) {
            return Err(format!("unknown control '{key}' (use params.json keys)"));
        }
        values.insert(key, v);
    }
    let settings = Settings::from_lookup(|key| values.get(key).copied().unwrap_or(0.0), a.time);
    let source = pngio::read(&a.input)?;
    let mut engine = Engine::new().map_err(|e| e.to_string())?;
    let started = Instant::now();
    let out = engine.render(&source, &settings).map_err(|e| e.to_string())?;
    eprintln!("rendered {}x{} on {} in {:.1} ms", out.width(), out.height(), engine.adapter_name(), started.elapsed().as_secs_f64() * 1000.0);
    pngio::write(&a.output, &out)
}
