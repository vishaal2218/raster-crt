//! Every Effect Controls entry, generated from panel/params.json at build time.
//!
//! The generated part (`Control`, `SPECS`, `MODULES`) comes from build/params_codegen.rs. This file
//! registers it with the host, reads it back each frame, and tests the hard rules: ids are
//! permanent, names fit Premiere, defaults sit inside their ranges.

use after_effects as ae;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Float { min: f64, max: f64, default: f64, precision: i16 },
    /// Premiere's angle control has no limits, so the range is enforced when reading.
    Angle { min: f64, max: f64, default: f64 },
    /// `default` is the 0-based option index (Premiere counts popups from 1).
    Popup { options: &'static [&'static str], default: u16 },
    Checkbox { default: bool },
}

#[cfg_attr(not(test), allow(dead_code))] // read by tests now, by the GPU pipeline next
#[derive(Debug)]
pub struct Spec {
    pub p: Control,
    pub id: u16,
    pub key: &'static str,
    pub label: &'static str,
    pub module: usize,
    pub kind: Kind,
    pub keyframe: bool,
    pub master: bool,
}

#[derive(Debug)]
pub struct Module {
    pub label: &'static str,
    pub start: Control,
    pub end: Control,
    pub collapsed: bool,
}

include!(concat!(env!("OUT_DIR"), "/params_gen.rs"));

impl std::fmt::Debug for Control {
    // The toolkit hashes this text into the id Premiere saves clip values under.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.stable_name())
    }
}

#[cfg_attr(not(test), allow(dead_code))] // read by tests now, by the GPU pipeline next
impl Kind {
    /// The default as the number the renderer sees (popup index, checkbox 0/1, degrees).
    pub fn default_value(self) -> f32 {
        match self {
            Kind::Float { default, .. } | Kind::Angle { default, .. } => default as f32,
            Kind::Popup { default, .. } => f32::from(default),
            Kind::Checkbox { default } => f32::from(u8::from(default)),
        }
    }
}

/// One frame's control values, indexed like `SPECS` (id - 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Values([f32; PARAM_COUNT]);

#[cfg_attr(not(test), allow(dead_code))] // read by tests now, by the GPU pipeline next
impl Values {
    pub fn defaults() -> Self {
        Self(std::array::from_fn(|i| SPECS[i].kind.default_value()))
    }

    pub fn get(&self, p: Control) -> f32 {
        SPECS.iter().position(|s| s.p == p).map_or(0.0, |i| self.0[i])
    }

    /// By params.json key, for the engine's settings lookup. Unknown keys read as 0.
    pub fn get_by_key(&self, key: &str) -> f32 {
        SPECS.iter().position(|s| s.key == key).map_or(0.0, |i| self.0[i])
    }
}

fn flags_for(spec: &Spec) -> ae::ParamFlag {
    let mut flags = ae::ParamFlag::empty();
    if !spec.keyframe {
        flags |= ae::ParamFlag::CANNOT_TIME_VARY;
    }
    if spec.key == "preset" {
        // Choosing a preset rewrites the other controls (UserChangedParam).
        flags |= ae::ParamFlag::SUPERVISE;
    }
    flags
}

fn add_spec(params: &mut ae::Parameters<Control>, spec: &Spec) -> Result<(), ae::Error> {
    let flags = flags_for(spec);
    let ui = ae::ParamUIFlags::empty();
    match spec.kind {
        Kind::Float { min, max, default, precision } => params.add_with_flags(
            spec.p,
            spec.label,
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(min as f32);
                f.set_valid_max(max as f32);
                f.set_slider_min(min as f32);
                f.set_slider_max(max as f32);
                f.set_default(default);
                f.set_value(default);
                f.set_precision(precision);
            }),
            flags,
            ui,
        ),
        Kind::Angle { default, .. } => params.add_with_flags(
            spec.p,
            spec.label,
            ae::AngleDef::setup(|a| {
                a.set_default(default as f32);
                a.set_value(default as f32);
            }),
            flags,
            ui,
        ),
        Kind::Popup { options, default } => params.add_with_flags(
            spec.p,
            spec.label,
            ae::PopupDef::setup(|p| {
                p.set_options(options);
                p.set_default(i32::from(default) + 1);
                p.set_value(i32::from(default) + 1);
            }),
            flags,
            ui,
        ),
        Kind::Checkbox { default } => params.add_with_flags(
            spec.p,
            spec.label,
            ae::CheckBoxDef::setup(|c| {
                c.set_default(default);
                c.set_value(default);
                // Premiere's scripting names a checkbox by this text, so it must be the label.
                c.set_label(spec.label);
            }),
            flags,
            ui,
        ),
    }
}

/// Adds the 10 twirl-down groups and their controls, in params.json order.
pub fn register(params: &mut ae::Parameters<Control>) -> Result<(), ae::Error> {
    for (index, module) in MODULES.iter().enumerate() {
        params.add_group(module.start, module.end, module.label, module.collapsed, |params| {
            SPECS.iter().filter(|s| s.module == index).try_for_each(|s| add_spec(params, s))
        })?;
    }
    Ok(())
}

fn read_one(params: &ae::Parameters<Control>, spec: &Spec) -> Result<f32, ae::Error> {
    let def = params.get(spec.p)?;
    Ok(match spec.kind {
        Kind::Float { min, max, .. } => (def.as_float_slider()?.value()).clamp(min, max) as f32,
        Kind::Angle { min, max, .. } => f64::from(def.as_angle()?.value()).clamp(min, max) as f32,
        Kind::Popup { options, .. } => {
            let last = options.len().saturating_sub(1) as i32;
            (def.as_popup()?.value() - 1).clamp(0, last) as f32
        }
        Kind::Checkbox { .. } => f32::from(u8::from(def.as_checkbox()?.value())),
    })
}

/// The value a factory look gives a control (its default when the look doesn't list it).
pub fn preset_value(look: &[(&str, f64)], spec: &Spec) -> f64 {
    look.iter().find(|(k, _)| *k == spec.key).map_or_else(|| f64::from(spec.kind.default_value()), |(_, v)| *v)
}

/// Sets every control to factory look `index` (the Preset popup's 0-based value; 0 is
/// Custom and changes nothing). Returns whether anything was applied.
pub fn apply_preset(params: &mut ae::Parameters<Control>, index: usize) -> Result<bool, ae::Error> {
    let Some(look) = index.checked_sub(1).and_then(|i| FACTORY.get(i)) else {
        return Ok(false);
    };
    for spec in SPECS.iter().filter(|s| s.key != "preset") {
        let v = preset_value(look, spec);
        let mut def = params.get_mut(spec.p)?;
        match spec.kind {
            Kind::Float { min, max, .. } => {
                def.as_float_slider_mut()?.set_value(v.clamp(min, max));
            }
            Kind::Angle { min, max, .. } => {
                def.as_angle_mut()?.set_value(v.clamp(min, max) as f32);
            }
            Kind::Popup { options, .. } => {
                def.as_popup_mut()?.set_value(v.round().clamp(0.0, (options.len() - 1) as f64) as i32 + 1);
            }
            Kind::Checkbox { .. } => {
                def.as_checkbox_mut()?.set_value(v >= 0.5);
            }
        }
    }
    Ok(true)
}

/// Reads every control for the frame being rendered.
pub fn read_values(params: &ae::Parameters<Control>) -> Result<Values, ae::Error> {
    let mut out = [0.0; PARAM_COUNT];
    for (slot, spec) in out.iter_mut().zip(SPECS.iter()) {
        *slot = read_one(params, spec)?;
    }
    Ok(Values(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const PREMIERE_NAME_LIMIT: usize = 31;

    fn all_entries() -> Vec<Control> {
        SPECS.iter().map(|s| s.p).chain(MODULES.iter().flat_map(|m| [m.start, m.end])).collect()
    }

    #[test]
    fn has_eighty_controls_in_ten_groups() {
        assert_eq!(SPECS.len(), 80);
        assert_eq!(MODULES.len(), 10);
        assert!(MODULES.iter().enumerate().all(|(i, _)| SPECS.iter().any(|s| s.module == i)));
    }

    #[test]
    fn ids_run_in_order_from_one() {
        assert!(SPECS.iter().enumerate().all(|(i, s)| usize::from(s.id) == i + 1));
    }

    #[test]
    fn stable_names_are_unique() {
        let names: HashSet<_> = all_entries().iter().map(|p| p.stable_name()).collect();
        assert_eq!(names.len(), all_entries().len());
    }

    #[test]
    fn stable_name_comes_from_the_permanent_id() {
        assert!(SPECS.iter().all(|s| s.p.stable_name() == format!("rc{}", s.id)));
        assert_eq!(format!("{:?}", SPECS[0].p), "rc1");
    }

    #[test]
    fn shipped_ids_never_move_or_vanish() {
        let snapshot = include_str!("../shipped-ids.txt");
        for line in snapshot.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
            let mut parts = line.split_whitespace();
            let id: u16 = parts.next().and_then(|t| t.parse().ok()).expect("snapshot line starts with an id");
            let key = parts.next().expect("snapshot line has a key");
            let spec = SPECS.iter().find(|s| s.id == id).unwrap_or_else(|| panic!("shipped id {id} ({key}) vanished"));
            assert_eq!(spec.key, key, "shipped id {id} changed meaning");
        }
    }

    #[test]
    fn labels_fit_premiere() {
        for name in SPECS.iter().map(|s| s.label).chain(MODULES.iter().map(|m| m.label)) {
            assert!(!name.is_empty() && name.len() <= PREMIERE_NAME_LIMIT, "bad label {name:?}");
        }
    }

    #[test]
    fn defaults_sit_inside_their_ranges() {
        for s in &SPECS {
            match s.kind {
                Kind::Float { min, max, default, .. } | Kind::Angle { min, max, default } => {
                    assert!(min < max && (min..=max).contains(&default), "{} default {default} outside {min}..{max}", s.key);
                }
                Kind::Popup { options, default } => {
                    assert!(usize::from(default) < options.len(), "{} default past its options", s.key);
                    assert!(options.iter().all(|o| !o.is_empty() && !o.contains('|')), "{} has a bad option", s.key);
                }
                Kind::Checkbox { .. } => {}
            }
        }
    }

    #[test]
    fn masters_start_off_and_cannot_be_keyframed() {
        let masters: Vec<_> = SPECS.iter().filter(|s| s.master).collect();
        assert_eq!(masters.len(), 9, "one master per module except Global");
        for m in masters {
            assert!(!m.keyframe, "{} must not keyframe", m.key);
            assert_eq!(m.kind.default_value(), 0.0, "{} must default to 0", m.key);
        }
    }

    #[test]
    fn preset_popup_lists_custom_then_eighteen_factory_looks() {
        let preset = SPECS.iter().find(|s| s.key == "preset").expect("preset control");
        let Kind::Popup { options, default } = preset.kind else { panic!("preset is a popup") };
        assert_eq!(options.len(), 19);
        assert_eq!((options[0], default), ("Custom", 0));
        assert!(flags_for(preset).contains(ae::ParamFlag::SUPERVISE));
    }

    #[test]
    fn only_global_group_starts_open() {
        assert!(!MODULES[0].collapsed && MODULES[1..].iter().all(|m| m.collapsed));
    }

    #[test]
    fn non_keyframeable_controls_get_the_hold_flag() {
        for s in &SPECS {
            assert_eq!(flags_for(s).contains(ae::ParamFlag::CANNOT_TIME_VARY), !s.keyframe, "{}", s.key);
        }
    }

    #[test]
    fn every_key_the_engine_reads_exists() {
        let keys = raster_engine::ENGINE_KEYS;
        for key in keys {
            assert!(SPECS.iter().any(|s| s.key == key), "engine reads '{key}' but params.json has no such control");
        }
        assert_eq!(Values::defaults().get_by_key("mask_scale"), 0.4);
    }

    #[test]
    fn factory_looks_match_the_preset_popup() {
        let preset = SPECS.iter().find(|s| s.key == "preset").unwrap();
        let Kind::Popup { options, .. } = preset.kind else { panic!() };
        assert_eq!(FACTORY.len(), options.len() - 1, "one look per popup entry after Custom");
    }

    #[test]
    fn factory_values_sit_inside_their_ranges_and_switch_modules_on() {
        for (i, look) in FACTORY.iter().enumerate() {
            for spec in SPECS.iter() {
                let v = preset_value(look, spec);
                if let Kind::Float { min, max, .. } | Kind::Angle { min, max, .. } = spec.kind {
                    assert!((min..=max).contains(&v), "look {} sets {} = {v}", i + 1, spec.key);
                }
            }
            assert!(SPECS.iter().filter(|s| s.master).any(|s| preset_value(look, s) > 0.0), "look {} turns no module on", i + 1);
        }
    }

    #[test]
    fn a_control_a_look_leaves_out_takes_its_default() {
        let mask_scale = SPECS.iter().find(|s| s.key == "mask_scale").unwrap();
        assert_eq!(preset_value(&[], mask_scale), 0.4f32 as f64);
        assert_eq!(preset_value(&[("mask_scale", 0.9)], mask_scale), 0.9);
    }

    #[test]
    fn defaults_read_back_by_control() {
        let v = Values::defaults();
        assert_eq!(v.get(Control::Mix), 1.0);
        assert_eq!(v.get(Control::Contrast), 1.0);
        assert_eq!(v.get(Control::Quality), 1.0, "Quality defaults to HI (index 1)");
    }
}
