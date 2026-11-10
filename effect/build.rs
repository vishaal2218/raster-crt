// Generates the effect's registration resource (PiPL) for Premiere and After Effects, and the
// control tables (src/params.rs includes them) from panel/params.json.
// The match name is permanent: projects find the effect by it, so never change it.
use pipl::*;
use std::path::Path;

#[path = "build/params_codegen.rs"]
mod params_codegen;

const PF_PLUG_IN_VERSION: u16 = 13;
const PF_PLUG_IN_SUBVERS: u16 = 28;
const PARAMS_JSON: &str = "../panel/params.json";
const FACTORY_JSON: &str = "../panel/presets/factory.json";

#[rustfmt::skip]
fn main() {
    for path in ["build.rs", "build/params_codegen.rs", PARAMS_JSON, FACTORY_JSON] {
        println!("cargo:rerun-if-changed={path}");
    }
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    params_codegen::generate(Path::new(PARAMS_JSON), Path::new(FACTORY_JSON), Path::new(&out_dir));

    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("Raster CRT"),
        Property::Category("Raster"),

        #[cfg(target_os = "windows")]
        Property::CodeWin64X86("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacIntel64("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacARM64("EffectMain"),

        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version { major: PF_PLUG_IN_VERSION, minor: PF_PLUG_IN_SUBVERS },
        Property::AE_Effect_Version {
            version: 0,
            subversion: 1,
            bugversion: 0,
            stage: Stage::Develop,
            build: 0,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(
            OutFlags::PixIndependent |
            OutFlags::UseOutputExtent |
            // noise, roll and flicker change every frame even on a still picture
            OutFlags::NonParamVary |
            // Persistence reads earlier frames of the input
            OutFlags::WideTimeInput |
            OutFlags::DeepColorAware
        ),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::SupportsThreadedRendering
        ),
        Property::AE_Effect_Match_Name("RASTER Raster CRT"),
        Property::AE_Reserved_Info(0),
        Property::AE_Effect_Support_URL("https://github.com/"),
    ]);

    // Switches read inside the toolkit's define_effect! macro (pipl sets some of them).
    for name in ["with_premiere", "threaded_rendering", "catch_panics", "smart_render",
                 "gpu_render", "does_dialog", "uses_audio", "sends_update_params_ui"] {
        println!("cargo::rustc-check-cfg=cfg({name})");
    }
    // A panic inside the effect must never take Premiere down, in release builds too.
    println!("cargo:rustc-cfg=catch_panics");
}
