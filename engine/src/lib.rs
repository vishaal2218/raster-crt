//! Raster CRT's GPU pipeline: wgpu running the WGSL passes in shaders/ (common, signal,
//! screen), plus the phosphor trail across frames (persistence.rs, on the CPU).
//!
//! Used by the Premiere/After Effects effect and by raster-cli, so the golden-frame tests
//! exercise exactly the shaders Premiere runs.

mod gpu;
mod image;
pub mod persistence;
mod settings;

pub use gpu::{Engine, EngineError};
pub use image::{Image, ImageError};
pub use settings::{Quality, Settings, ENGINE_KEYS};
