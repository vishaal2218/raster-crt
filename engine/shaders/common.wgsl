// Raster CRT shared shader code: bindings, sampling, colour space and repeatable noise.
// `struct Uniforms` is generated from settings.rs and placed above this file.
//
// Buffers hold vec4<f32> pixels, rows top to bottom. Each entry point binds only what it
// uses (pipelines take their layouts from the shader), so a buffer's meaning depends on the
// pass: `src` is the pass input, `aux` a second input, `dst` the output.

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> aux: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> glowbuf: array<vec4<f32>>;

const LUMA = vec3<f32>(0.299, 0.587, 0.114);
const TAU = 6.2831853;

fn at(x: u32, y: u32, w: u32) -> u32 {
    return y * w + x;
}

fn clamp_to(v: i32, size: u32) -> u32 {
    return u32(clamp(v, 0, i32(size) - 1));
}

// ---------- reads ----------

fn src_px(x: i32, y: i32) -> vec4<f32> {
    return src[at(clamp_to(x, u.src_w), clamp_to(y, u.src_h), u.src_w)];
}

fn src_ras(x: i32, y: i32) -> vec4<f32> {
    return src[at(clamp_to(x, u.ras_w), clamp_to(y, u.ras_h), u.ras_w)];
}

fn aux_ras(x: i32, y: i32) -> vec4<f32> {
    return aux[at(clamp_to(x, u.ras_w), clamp_to(y, u.ras_h), u.ras_w)];
}

fn glow_px(x: i32, y: i32) -> vec4<f32> {
    return glowbuf[at(clamp_to(x, u.glow_w), clamp_to(y, u.glow_h), u.glow_w)];
}

// p is in pixel units with pixel centres at +0.5.
fn bilinear_src(p: vec2<f32>) -> vec4<f32> {
    let q = p - 0.5;
    let i = vec2<i32>(floor(q));
    let f = fract(q);
    return mix(mix(src_px(i.x, i.y), src_px(i.x + 1, i.y), f.x), mix(src_px(i.x, i.y + 1), src_px(i.x + 1, i.y + 1), f.x), f.y);
}

fn bilinear_aux_ras(p: vec2<f32>) -> vec4<f32> {
    let q = p - 0.5;
    let i = vec2<i32>(floor(q));
    let f = fract(q);
    return mix(mix(aux_ras(i.x, i.y), aux_ras(i.x + 1, i.y), f.x), mix(aux_ras(i.x, i.y + 1), aux_ras(i.x + 1, i.y + 1), f.x), f.y);
}

fn bilinear_glow(p: vec2<f32>) -> vec3<f32> {
    let q = p - 0.5;
    let i = vec2<i32>(floor(q));
    let f = fract(q);
    return mix(mix(glow_px(i.x, i.y), glow_px(i.x + 1, i.y), f.x), mix(glow_px(i.x, i.y + 1), glow_px(i.x + 1, i.y + 1), f.x), f.y).rgb;
}

// One raster line, sampled between its pixels (x in pixel units, centres at +0.5).
fn line_src_ras(x: f32, line: i32) -> vec4<f32> {
    let q = x - 0.5;
    let i = i32(floor(q));
    return mix(src_ras(i, line), src_ras(i + 1, line), fract(q));
}

fn line_aux_ras(x: f32, line: i32) -> vec4<f32> {
    let q = x - 0.5;
    let i = i32(floor(q));
    return mix(aux_ras(i, line), aux_ras(i + 1, line), fract(q));
}

// ---------- colour: FCC YIQ and its exact inverse ----------

fn rgb_to_yiq(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        dot(c, vec3<f32>(0.299, 0.587, 0.114)),
        dot(c, vec3<f32>(0.595716, -0.274453, -0.321263)),
        dot(c, vec3<f32>(0.211456, -0.522591, 0.311135)));
}

fn yiq_to_rgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        dot(c, vec3<f32>(1.0, 0.9562948, 0.6210251)),
        dot(c, vec3<f32>(1.0, -0.2721214, -0.6473809)),
        dot(c, vec3<f32>(1.0, -1.1069890, 1.7046150)));
}

// ---------- repeatable noise (same inputs, same numbers, on every render) ----------

fn hash_u(x: u32) -> u32 {
    let v = x * 747796405u + 2891336453u;
    let w = ((v >> ((v >> 28u) + 4u)) ^ v) * 277803737u;
    return (w >> 22u) ^ w;
}

fn rnd(a: u32, b: u32, c: u32) -> f32 {
    return f32(hash_u(a ^ hash_u(b ^ hash_u(c))) >> 8u) / 16777216.0;
}

// Smooth value noise along t (t >= 0).
fn vnoise(t: f32, salt: u32) -> f32 {
    let i = u32(floor(max(t, 0.0)));
    let f = fract(t);
    return mix(rnd(i, salt, 17u), rnd(i + 1u, salt, 17u), f * f * (3.0 - 2.0 * f));
}
