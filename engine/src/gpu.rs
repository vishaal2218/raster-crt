//! The wgpu side: one device, the six compute passes from shaders/crt.wgsl, and buffers
//! that are reused while the frame size stays the same.

use std::fmt;
use std::sync::{mpsc, Arc};

use crate::image::Image;
use crate::settings::{glow_size, Settings, Uniforms, UNIFORMS_WGSL};

const COMMON_WGSL: &str = include_str!("../shaders/common.wgsl");
const SIGNAL_WGSL: &str = include_str!("../shaders/signal.wgsl");
const SCREEN_WGSL: &str = include_str!("../shaders/screen.wgsl");
const WORKGROUP: u32 = 8;
/// bend_line runs one thread per raster line in groups of this many.
const LINE_WORKGROUP: u32 = 64;
const PIXEL_BYTES: u64 = 16;

/// The full shader: the generated uniform struct, then the hand-written passes.
fn shader_source() -> String {
    [UNIFORMS_WGSL, COMMON_WGSL, SIGNAL_WGSL, SCREEN_WGSL].concat()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    NoAdapter(String),
    NoDevice(String),
    Shader(String),
    TooLarge { width: u32, height: u32 },
    Gpu(String),
    Readback(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::NoAdapter(e) => write!(f, "no usable GPU found: {e}"),
            EngineError::NoDevice(e) => write!(f, "the GPU refused a device: {e}"),
            EngineError::Shader(e) => write!(f, "the CRT shader didn't compile: {e}"),
            EngineError::TooLarge { width, height } => write!(f, "{width}x{height} is larger than this GPU can process"),
            EngineError::Gpu(e) => write!(f, "the GPU rejected a frame: {e}"),
            EngineError::Readback(e) => write!(f, "couldn't read the frame back from the GPU: {e}"),
        }
    }
}

impl std::error::Error for EngineError {}

fn frame_bytes(width: u32, height: u32) -> u64 {
    u64::from(width) * u64::from(height) * PIXEL_BYTES
}

struct Passes {
    raster: wgpu::ComputePipeline,
    colour: wgpu::ComputePipeline,
    bend_line: wgpu::ComputePipeline,
    bend_decode: wgpu::ComputePipeline,
    damage: wgpu::ComputePipeline,
    decode: wgpu::ComputePipeline,
    screen: wgpu::ComputePipeline,
    glow_down: wgpu::ComputePipeline,
    blur_h: wgpu::ComputePipeline,
    blur_v: wgpu::ComputePipeline,
    finish: wgpu::ComputePipeline,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SizeKey {
    width: u32,
    height: u32,
    ras_w: u32,
    ras_h: u32,
}

struct Buffers {
    key: SizeKey,
    source: wgpu::Buffer,
    /// the clean picture at the processing raster (RGB)
    raster: wgpu::Buffer,
    /// signal chain work buffers at the raster (YIQ or composite)
    sig_a: wgpu::Buffer,
    sig_b: wgpu::Buffer,
    sig_c: wgpu::Buffer,
    /// the processed raster handed to the screen (RGB)
    processed: wgpu::Buffer,
    screen: wgpu::Buffer,
    glow_a: wgpu::Buffer,
    glow_b: wgpu::Buffer,
    output: wgpu::Buffer,
    staging: wgpu::Buffer,
}

pub struct Engine {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_name: String,
    uniforms: wgpu::Buffer,
    passes: Passes,
    buffers: Option<Buffers>,
}

impl Engine {
    /// Opens the GPU and compiles the shader. Slow (tens of milliseconds); do it once.
    pub fn new() -> Result<Self, EngineError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| EngineError::NoAdapter(e.to_string()))?;
        let adapter_name = adapter.get_info().name;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("raster-crt"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| EngineError::NoDevice(e.to_string()))?;
        // Anything not caught by an error scope is reported, never allowed to abort the host.
        device.on_uncaptured_error(Arc::new(|e| eprintln!("raster-crt: GPU error: {e}")));

        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crt.wgsl"),
            source: wgpu::ShaderSource::Wgsl(shader_source().into()),
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let passes = Passes {
            raster: pipeline("raster"),
            colour: pipeline("colour"),
            bend_line: pipeline("bend_line"),
            bend_decode: pipeline("bend_decode"),
            damage: pipeline("damage"),
            decode: pipeline("decode"),
            screen: pipeline("screen"),
            glow_down: pipeline("glow_down"),
            blur_h: pipeline("blur_h"),
            blur_v: pipeline("blur_v"),
            finish: pipeline("finish"),
        };
        if let Some(err) = pollster::block_on(scope.pop()) {
            return Err(EngineError::Shader(err.to_string()));
        }
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self { device, queue, adapter_name, uniforms, passes, buffers: None })
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Renders one frame. The output has the source's size.
    pub fn render(&mut self, source: &Image, settings: &Settings) -> Result<Image, EngineError> {
        let (width, height) = (source.width(), source.height());
        let u = settings.uniforms(width, height);
        let bytes = frame_bytes(width, height);
        let limits = self.device.limits();
        if bytes > u64::from(limits.max_storage_buffer_binding_size) || bytes > limits.max_buffer_size {
            return Err(EngineError::TooLarge { width, height });
        }

        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        self.ensure_buffers(SizeKey { width, height, ras_w: u.ras_w, ras_h: u.ras_h });
        let b = self.buffers.as_ref().expect("buffers exist after ensure_buffers");
        self.queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&u));
        self.queue.write_buffer(&b.source, 0, bytemuck::cast_slice(source.pixels()));

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("raster-crt frame") });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("crt"), timestamp_writes: None });
            let p = &self.passes;
            let (rw, rh) = (u.ras_w, u.ras_h);
            self.dispatch(&mut pass, &p.raster, &[(1, &b.source), (3, &b.raster)], rw, rh);
            if u.signal_on == 1 {
                self.dispatch(&mut pass, &p.colour, &[(1, &b.raster), (3, &b.sig_a)], rw, rh);
                self.dispatch_lines(&mut pass, &p.bend_line, &[(1, &b.sig_a), (3, &b.sig_b)], rh);
                self.dispatch(&mut pass, &p.bend_decode, &[(1, &b.sig_b), (2, &b.sig_a), (3, &b.sig_c)], rw, rh);
                self.dispatch(&mut pass, &p.damage, &[(1, &b.sig_c), (3, &b.sig_a)], rw, rh);
            }
            self.dispatch(&mut pass, &p.decode, &[(1, &b.sig_a), (2, &b.raster), (3, &b.processed)], rw, rh);
            self.dispatch(&mut pass, &p.screen, &[(1, &b.source), (2, &b.processed), (3, &b.screen)], width, height);
            self.dispatch(&mut pass, &p.glow_down, &[(1, &b.screen), (3, &b.glow_a)], u.glow_w, u.glow_h);
            self.dispatch(&mut pass, &p.blur_h, &[(1, &b.glow_a), (3, &b.glow_b)], u.glow_w, u.glow_h);
            self.dispatch(&mut pass, &p.blur_v, &[(1, &b.glow_b), (3, &b.glow_a)], u.glow_w, u.glow_h);
            self.dispatch(&mut pass, &p.finish, &[(1, &b.source), (2, &b.screen), (3, &b.output), (4, &b.glow_a)], width, height);
        }
        encoder.copy_buffer_to_buffer(&b.output, 0, &b.staging, 0, bytes);
        self.queue.submit([encoder.finish()]);
        if let Some(err) = pollster::block_on(scope.pop()) {
            return Err(EngineError::Gpu(err.to_string()));
        }
        let pixels = self.read_back(bytes)?;
        Image::new(width, height, pixels).map_err(|e| EngineError::Readback(e.to_string()))
    }

    /// One thread per raster line (the bend filter runs along each line in order).
    fn dispatch_lines(&self, pass: &mut wgpu::ComputePass<'_>, pipeline: &wgpu::ComputePipeline, buffers: &[(u32, &wgpu::Buffer)], lines: u32) {
        self.bind(pass, pipeline, buffers);
        pass.dispatch_workgroups(lines.div_ceil(LINE_WORKGROUP), 1, 1);
    }

    fn dispatch(&self, pass: &mut wgpu::ComputePass<'_>, pipeline: &wgpu::ComputePipeline, buffers: &[(u32, &wgpu::Buffer)], width: u32, height: u32) {
        self.bind(pass, pipeline, buffers);
        pass.dispatch_workgroups(width.div_ceil(WORKGROUP), height.div_ceil(WORKGROUP), 1);
    }

    fn bind(&self, pass: &mut wgpu::ComputePass<'_>, pipeline: &wgpu::ComputePipeline, buffers: &[(u32, &wgpu::Buffer)]) {
        let entries: Vec<wgpu::BindGroupEntry> = std::iter::once((0, &self.uniforms))
            .chain(buffers.iter().copied())
            .map(|(binding, buffer)| wgpu::BindGroupEntry { binding, resource: buffer.as_entire_binding() })
            .collect();
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
    }

    fn ensure_buffers(&mut self, key: SizeKey) {
        if self.buffers.as_ref().is_some_and(|b| b.key == key) {
            return;
        }
        use wgpu::BufferUsages as Use;
        let (glow_w, glow_h) = glow_size(key.width, key.height);
        let frame = frame_bytes(key.width, key.height);
        let raster = frame_bytes(key.ras_w, key.ras_h);
        let make = |label: &str, size: u64, usage: Use| {
            self.device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false })
        };
        let buffers = Buffers {
            key,
            source: make("source", frame, Use::STORAGE | Use::COPY_DST),
            raster: make("raster", raster, Use::STORAGE),
            sig_a: make("signal a", raster, Use::STORAGE),
            sig_b: make("signal b", raster, Use::STORAGE),
            sig_c: make("signal c", raster, Use::STORAGE),
            processed: make("processed", raster, Use::STORAGE),
            screen: make("screen", frame, Use::STORAGE),
            glow_a: make("glow a", frame_bytes(glow_w, glow_h), Use::STORAGE),
            glow_b: make("glow b", frame_bytes(glow_w, glow_h), Use::STORAGE),
            output: make("output", frame, Use::STORAGE | Use::COPY_SRC),
            staging: make("staging", frame, Use::MAP_READ | Use::COPY_DST),
        };
        self.buffers = Some(buffers);
    }

    fn read_back(&self, bytes: u64) -> Result<Vec<[f32; 4]>, EngineError> {
        let readback = |e: &dyn fmt::Display| EngineError::Readback(e.to_string());
        let staging = &self.buffers.as_ref().expect("buffers exist after ensure_buffers").staging;
        let (tx, rx) = mpsc::channel();
        staging.map_async(wgpu::MapMode::Read, ..bytes, move |result| {
            // the receiver only goes away if render already returned an error
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| readback(&e))?;
        rx.recv().map_err(|e| readback(&e))?.map_err(|e| readback(&e))?;
        let pixels = {
            let view = staging.get_mapped_range(..bytes).map_err(|e| readback(&e))?;
            match bytemuck::try_cast_slice::<u8, [f32; 4]>(&view) {
                Ok(aligned) => aligned.to_vec(),
                Err(_) => view.chunks_exact(PIXEL_BYTES as usize).map(pixel_from_bytes).collect(),
            }
        };
        staging.unmap();
        Ok(pixels)
    }
}

fn pixel_from_bytes(chunk: &[u8]) -> [f32; 4] {
    std::array::from_fn(|i| f32::from_le_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]))
}
