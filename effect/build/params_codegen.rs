// Turns panel/params.json (and the factory preset names) into Rust: the control enum,
// its permanent names, and the static tables params.rs registers with Premiere.
use serde_json::Value;
use std::fmt::Write as _;
use std::path::Path;

/// Premiere caps effect control names at 31 bytes.
const MAX_LABEL_BYTES: usize = 31;
/// Wide integer ranges (shutter 1/x, cycles) show whole numbers; everything else 2 decimals.
const WHOLE_NUMBER_MIN_RANGE: f64 = 10.0;

fn camel(key: &str) -> String {
    key.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        })
        .collect()
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("can't read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()))
}

fn num(p: &Value, field: &str) -> f64 {
    p[field].as_f64().unwrap_or_else(|| panic!("param {} needs a numeric '{field}'", p["key"]))
}

fn text<'a>(v: &'a Value, field: &str) -> &'a str {
    v[field].as_str().unwrap_or_else(|| panic!("{v} needs a text '{field}'"))
}

fn is_whole(v: f64) -> bool {
    v.fract() == 0.0
}

fn popup_options(p: &Value, factory_names: &[String]) -> Vec<String> {
    if text(p, "key") == "preset" {
        return std::iter::once("Custom".to_string()).chain(factory_names.iter().cloned()).collect();
    }
    p["options"]
        .as_array()
        .unwrap_or_else(|| panic!("popup {} needs an options list", p["key"]))
        .iter()
        .map(|o| o.as_str().expect("popup options are text").to_string())
        .collect()
}

fn kind_literal(p: &Value, factory_names: &[String]) -> String {
    match text(p, "type") {
        "float" => {
            let (min, max, default) = (num(p, "min"), num(p, "max"), num(p, "default"));
            let whole = max - min > WHOLE_NUMBER_MIN_RANGE && [min, max, default].into_iter().all(is_whole);
            let precision = if whole { 0 } else { 2 };
            format!("Kind::Float {{ min: {min:?}, max: {max:?}, default: {default:?}, precision: {precision} }}")
        }
        "angle" => format!(
            "Kind::Angle {{ min: {:?}, max: {:?}, default: {:?} }}",
            num(p, "min"), num(p, "max"), num(p, "default")
        ),
        "popup" => {
            let options = popup_options(p, factory_names);
            let list = options.iter().map(|o| format!("{o:?}")).collect::<Vec<_>>().join(", ");
            format!("Kind::Popup {{ options: &[{list}], default: {} }}", num(p, "default") as u16)
        }
        "checkbox" => format!("Kind::Checkbox {{ default: {} }}", p["default"].as_bool().expect("checkbox default is true/false")),
        other => panic!("param {} has unknown type '{other}'", p["key"]),
    }
}

/// Writes `params_gen.rs` into `out_dir`. Panics (failing the build) on malformed data.
pub fn generate(params_json: &Path, factory_json: &Path, out_dir: &Path) {
    let doc = read_json(params_json);
    let factory = read_json(factory_json);
    let factory_names: Vec<String> = factory["presets"]
        .as_array()
        .expect("factory.json needs a presets list")
        .iter()
        .map(|p| text(p, "name").to_string())
        .collect();
    let modules = doc["modules"].as_array().expect("params.json needs modules");
    let params = doc["params"].as_array().expect("params.json needs params");
    let module_index = |key: &str| {
        modules.iter().position(|m| text(m, "key") == key).unwrap_or_else(|| panic!("unknown module '{key}'"))
    };

    let mut variants = Vec::new();
    let mut names = Vec::new();
    let mut specs = String::new();
    for (i, p) in params.iter().enumerate() {
        let (id, key, label) = (num(p, "id") as u16, text(p, "key"), text(p, "label"));
        assert_eq!(usize::from(id), i + 1, "params.json ids must run 1, 2, 3... in order (found {id} at row {})", i + 1);
        assert!(label.len() <= MAX_LABEL_BYTES, "label '{label}' is longer than Premiere's {MAX_LABEL_BYTES} bytes");
        let variant = camel(key);
        writeln!(
            specs,
            "    Spec {{ p: Control::{variant}, id: {id}, key: {key:?}, label: {label:?}, module: {}, kind: {}, keyframe: {}, master: {} }},",
            module_index(text(p, "module")),
            kind_literal(p, &factory_names),
            p["keyframe"].as_bool().unwrap_or(true),
            p["master"].as_bool().unwrap_or(false),
        )
        .unwrap();
        names.push((variant.clone(), format!("rc{id}")));
        variants.push(variant);
    }

    let mut module_rows = String::new();
    for (i, m) in modules.iter().enumerate() {
        let (key, label) = (text(m, "key"), text(m, "label"));
        let (start, end) = (format!("Group{}", camel(key)), format!("Group{}End", camel(key)));
        writeln!(module_rows, "    Module {{ label: {label:?}, start: Control::{start}, end: Control::{end}, collapsed: {} }},", i != 0).unwrap();
        names.push((start.clone(), format!("rcg_{key}")));
        names.push((end.clone(), format!("rcg_{key}_end")));
        variants.push(start);
        variants.push(end);
    }

    let mut out = String::from("// @generated by build.rs from panel/params.json and panel/presets/factory.json. Do not edit.\n\n");
    out += "/// Every Effect Controls entry: the controls (in params.json order), then each group's start and end.\n";
    out += "#[derive(Eq, PartialEq, Hash, Clone, Copy)]\npub enum Control {\n";
    variants.iter().for_each(|v| writeln!(out, "    {v},").unwrap());
    out += "}\n\nimpl Control {\n";
    out += "    /// Premiere stores clip values under a hash of this name, so it is permanent: never change one.\n";
    out += "    pub const fn stable_name(self) -> &'static str {\n        match self {\n";
    names.iter().for_each(|(v, n)| writeln!(out, "            Control::{v} => {n:?},").unwrap());
    out += "        }\n    }\n}\n\n";
    writeln!(out, "pub const PARAM_COUNT: usize = {};", params.len()).unwrap();
    writeln!(out, "pub const MODULE_COUNT: usize = {};\n", modules.len()).unwrap();
    out += "pub static SPECS: [Spec; PARAM_COUNT] = [\n";
    out += &specs;
    out += "];\n\npub static MODULES: [Module; MODULE_COUNT] = [\n";
    out += &module_rows;
    out += "];\n";

    out += &factory_table(&factory, params);
    std::fs::write(out_dir.join("params_gen.rs"), out).expect("can't write params_gen.rs");
}

/// The factory looks as (key, value) lists, in Preset popup order (Custom is 0, so look n is
/// FACTORY[n - 1]). Popups are 0-based, checkboxes 0 or 1. Unknown keys fail the build.
fn factory_table(factory: &Value, params: &[Value]) -> String {
    let mut out = String::from("\n/// Factory looks in Preset popup order (popup index n is FACTORY[n - 1]).\npub static FACTORY: &[&[(&str, f64)]] = &[\n");
    for preset in factory["presets"].as_array().expect("factory.json needs a presets list") {
        let name = text(preset, "name");
        let values = preset["values"].as_object().unwrap_or_else(|| panic!("preset {name} needs a values object"));
        let entries: Vec<String> = values
            .iter()
            .map(|(key, v)| {
                assert!(params.iter().any(|p| text(p, "key") == key), "preset {name} sets unknown control '{key}'");
                let n = v.as_f64().or_else(|| v.as_bool().map(|b| f64::from(u8::from(b)))).unwrap_or_else(|| panic!("preset {name}: '{key}' isn't a number"));
                format!("({key:?}, {n:?})")
            })
            .collect();
        writeln!(out, "    &[{}],", entries.join(", ")).unwrap();
    }
    out += "];\n";
    out
}
