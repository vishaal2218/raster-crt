// Raster CRT output passes, at the frame's own size:
//   screen (P8 beam + mask, P9 capture, P10 convergence, P11 glass warp) -> glow_down,
//   blur_h, blur_v (glow) -> finish (glow, halation, spectral bloom, P12 grade + Dry/Wet).

const SQRT_TAU = 2.5066283;
const MASK_DIM = 0.22;         // unlit phosphor in a mask cell
const MASK_GAIN = 1.45;        // gives back most of the light the mask takes away
const SLOT_GAP_DIM = 0.4;      // the dark bar between slots
const MAX_BLUR_TAPS = 32;

// ---------- glass ----------

// Output position (centred, x in -aspect..aspect, y in -1..1) to the screen position it shows.
fn warp(c0: vec2<f32>, aspect: f32) -> vec2<f32> {
    var c = c0 * (1.0 - 0.12 * u.overscan);
    let n = c / vec2<f32>(aspect, 1.0);
    let r2 = dot(n, n);
    c *= 1.0 + u.curve * 0.18 * r2 + u.fisheye * 0.12 * r2 * r2;
    c.x *= 1.0 + u.line_stretch * 0.15 * (1.0 - n.y * n.y);
    // supply ripple: mains hum wobbling the deflection
    c.x += u.ripple * 0.012 * sin(TAU * (n.y * 2.0 + u.time * 0.7));
    return c;
}

// Soft tube edges and corner fall-off, growing with the glass curve.
fn edge_shade(uv: vec2<f32>) -> f32 {
    if (u.curve <= 0.0) {
        return 1.0;
    }
    let e = min(uv, 1.0 - uv);
    let soft = 0.004 + 0.02 * u.curve;
    let edge = smoothstep(0.0, soft, e.x) * smoothstep(0.0, soft, e.y);
    let vignette = pow(clamp(16.0 * uv.x * uv.y * (1.0 - uv.x) * (1.0 - uv.y), 0.0, 1.0), 0.15 * u.curve);
    return edge * vignette;
}

// ---------- beam ----------

// Scanlines: each raster line is a beam with a gaussian profile, wider when it is bright.
fn beam(x: f32, y: f32) -> vec3<f32> {
    let above = floor(y - 0.5);
    var sum = vec3<f32>(0.0);
    for (var k = -1; k <= 2; k++) {
        let line = above + f32(k);
        let colour = line_aux_ras(x, i32(line)).rgb;
        let sigma = u.beam_sigma * (1.0 + u.bloom * dot(colour, LUMA));
        let d = y - (line + 0.5);
        // dividing by the profile's area keeps each line's total light the same
        sum += colour * exp(-0.5 * d * d / (sigma * sigma)) / (sigma * SQRT_TAU);
    }
    return sum;
}

// The picture at a screen position: the clean frame, or the processed raster drawn as
// scanlines, faded by how far the raster modules are turned up.
fn picture(uv: vec2<f32>) -> vec4<f32> {
    let clean = bilinear_src(uv * vec2<f32>(f32(u.src_w), f32(u.src_h)));
    if (u.signal_amount <= 0.0) {
        return clean;
    }
    let p = uv * vec2<f32>(f32(u.ras_w), f32(u.ras_h));
    // Line phase: the first output row lands on a line centre, so whole-number ratios give
    // crisp lines; two vertical taps per pixel soften the beating at in-between ratios.
    let s = f32(u.ras_h) / f32(u.out_h);
    let py = p.y + 0.5 * (1.0 - s);
    let beams = 0.5 * (beam(p.x, py - 0.25 * s) + beam(p.x, py + 0.25 * s));
    let crt = mix(bilinear_aux_ras(p).rgb, beams, u.scan_depth);
    return vec4<f32>(mix(clean.rgb, crt, u.signal_amount), clean.a);
}

// ---------- mask ----------

// How lit channel k is at a phase (0..1) across one triad.
fn stripe(phase: f32, k: f32) -> f32 {
    let d = abs(fract(phase - (k + 0.5) / 3.0 + 0.5) - 0.5);
    return mix(MASK_DIM, 1.0, 1.0 - smoothstep(0.08, 0.3, d));
}

fn stripes(x: f32, pitch: f32) -> vec3<f32> {
    let phase = x / pitch;
    return vec3<f32>(stripe(phase, 0.0), stripe(phase, 1.0), stripe(phase, 2.0));
}

fn slot_mask(p: vec2<f32>, pitch: f32) -> vec3<f32> {
    let column = floor(p.x / pitch);
    let height = pitch * 1.5;
    let row = fract((p.y + (column % 2.0) * height * 0.5) / height);
    let open = smoothstep(0.0, 0.12, row) * (1.0 - smoothstep(0.82, 1.0, row));
    return stripes(p.x, pitch) * mix(SLOT_GAP_DIM, 1.0, open);
}

fn delta_mask(p: vec2<f32>, pitch: f32) -> vec3<f32> {
    let cell = pitch / 1.7320508;
    let row = floor(p.y / cell);
    let q = vec2<f32>(p.x / cell + (row % 2.0) * 0.5, p.y / cell);
    let dot_light = 1.0 - smoothstep(0.28, 0.5, length(fract(q) - 0.5));
    let channel = u32(floor(q.x) + 2.0 * row) % 3u;
    var m = vec3<f32>(MASK_DIM);
    m[channel] = mix(MASK_DIM, 1.0, dot_light);
    return m;
}

fn grid_mask(p: vec2<f32>, pitch: f32) -> vec3<f32> {
    let f = fract(p / (pitch / 3.0));
    let open = smoothstep(0.0, 0.18, f.x) * (1.0 - smoothstep(0.82, 1.0, f.x))
             * smoothstep(0.0, 0.18, f.y) * (1.0 - smoothstep(0.82, 1.0, f.y));
    return stripes(p.x, pitch) * mix(SLOT_GAP_DIM, 1.0, open);
}

fn mask_at(p: vec2<f32>) -> vec3<f32> {
    switch u.mask_type {
        case 0u: { return slot_mask(p, u.mask_pitch); }
        case 1u: { return stripes(p.x, u.mask_pitch); }
        case 2u: { return delta_mask(p, u.mask_pitch); }
        case 3u: { return slot_mask(p, u.mask_pitch * 0.6); }
        default: { return grid_mask(p, u.mask_pitch); }
    }
}

// The mask over this output pixel. Phosphor misalignment shifts red and blue apart.
fn mask(p: vec2<f32>) -> vec3<f32> {
    // a triad under ~3 output pixels can't be drawn without moire, so it fades out
    let strength = u.mask_strength * smoothstep(2.0, 4.0, u.mask_pitch);
    if (strength <= 0.0) {
        return vec3<f32>(1.0);
    }
    var m = mask_at(p);
    if (u.conv_phosphor > 0.0) {
        let d = vec2<f32>(u.conv_dir_x, u.conv_dir_y) * u.conv_phosphor * u.mask_pitch * 0.5;
        m = vec3<f32>(mask_at(p + d).r, m.g, mask_at(p - d).b);
    }
    return mix(vec3<f32>(1.0), m * MASK_GAIN, strength);
}

// ---------- capture: a camera filming the tube ----------

fn capture(c: vec3<f32>, uv: vec2<f32>, px: vec2<f32>) -> vec3<f32> {
    var out = c;
    // the scan band: the tube lights each line briefly, the camera exposes for 1/shutter s
    let along = (uv.y - 0.5) * u.band_cos + (uv.x - 0.5) * u.band_sin;
    let bands = max(0.25, u.refresh / 60.0 * max(u.readout, 0.05));
    let wander = u.scan_sync * 0.35 * (vnoise(u.time * 2.3, 61u) - 0.5);
    let phase = fract(along * bands - u.time * (0.2 + abs(u.refresh - 60.0) * 0.02) + wander);
    let tau = 0.03 + u.persistence * 0.45;
    // phosphor decay after the beam passes, scaled to average 1 over a cycle
    let decay = exp(-phase / tau) / (tau * (1.0 - exp(-1.0 / tau)));
    let profile = mix(clamp(decay, 0.0, 3.0), 1.0, u.shutter_duty);
    out *= mix(1.0, profile, min(u.pulse, 2.0) * 0.5);
    if (u.moire > 0.0) {
        // the sensor's grid beating against the mask
        let k = 1.0 / max(u.mask_pitch, 2.0);
        let m = sin(TAU * (px.x * k * 1.04 + px.y * 0.013)) * sin(TAU * (px.y * k * 0.97 - px.x * 0.009));
        out *= 1.0 + u.moire * 0.35 * vec3<f32>(m, -0.6 * m, 0.4 * m);
    }
    if (u.rainbow > 0.0) {
        let hue = fract((uv.x * 1.3 + uv.y) * 5.0 + u.time * 0.15);
        let bow = clamp(abs(fract(hue + vec3<f32>(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0, vec3<f32>(0.0), vec3<f32>(1.0));
        out *= mix(vec3<f32>(1.0), 0.6 + 0.8 * bow, u.rainbow * 0.5);
    }
    return out;
}

// ---------- P8-P11: the screen ----------

// src: the source frame; aux: the processed raster (RGB).
@compute @workgroup_size(8, 8)
fn screen(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.out_w || id.y >= u.out_h) {
        return;
    }
    let res = vec2<f32>(f32(u.out_w), f32(u.out_h));
    let aspect = res.x / res.y;
    let px = vec2<f32>(id.xy) + 0.5;
    let c = warp((px / res * 2.0 - 1.0) * vec2<f32>(aspect, 1.0), aspect);
    var uv = (c / vec2<f32>(aspect, 1.0)) * 0.5 + 0.5;
    let inside = all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
    let out_index = at(id.x, id.y, u.out_w);
    if (!inside && u.extend_edges == 0u) {
        dst[out_index] = vec4<f32>(0.0, 0.0, 0.0, select(1.0, 0.0, u.outside_transparent == 1u));
        return;
    }
    uv = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));

    let mid = picture(uv);
    var colour = mid.rgb;
    if (u.conv_beam > 0.0) {
        // beam misalignment: red and blue land beside green, more toward the edges
        let r = length(uv - 0.5) * 2.0;
        let off = vec2<f32>(u.conv_dir_x / aspect, u.conv_dir_y) * u.conv_beam * (0.3 + 0.7 * r) * 0.006;
        colour = vec3<f32>(picture(uv + off).r, colour.g, picture(uv - off).b);
    }
    colour *= mask(px);
    colour *= edge_shade(uv) * u.flicker_gain;
    if (u.capture > 0.0) {
        colour = capture(colour, uv, px);
    }
    dst[out_index] = vec4<f32>(colour, mid.a);
}

// ---------- glow: quarter-size copy, then a separable gaussian ----------

@compute @workgroup_size(8, 8)
fn glow_down(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.glow_w || id.y >= u.glow_h) {
        return;
    }
    let step = vec2<f32>(f32(u.out_w) / f32(u.glow_w), f32(u.out_h) / f32(u.glow_h));
    let x0 = u32(floor(f32(id.x) * step.x));
    let y0 = u32(floor(f32(id.y) * step.y));
    let x1 = min(u32(ceil(f32(id.x + 1u) * step.x)), u.out_w);
    let y1 = min(u32(ceil(f32(id.y + 1u) * step.y)), u.out_h);
    var sum = vec4<f32>(0.0);
    var count = 0.0;
    for (var y = y0; y < y1; y++) {
        for (var x = x0; x < x1; x++) {
            sum += src[at(x, y, u.out_w)];
            count += 1.0;
        }
    }
    dst[at(id.x, id.y, u.glow_w)] = sum / max(count, 1.0);
}

fn blur(id: vec2<u32>, dir: vec2<i32>) {
    if (id.x >= u.glow_w || id.y >= u.glow_h) {
        return;
    }
    let taps = min(i32(ceil(u.glow_radius)), MAX_BLUR_TAPS);
    let sigma = max(u.glow_radius / 2.5, 0.5);
    var sum = vec4<f32>(0.0);
    var weight = 0.0;
    for (var k = -taps; k <= taps; k++) {
        let w = exp(-0.5 * f32(k * k) / (sigma * sigma));
        let x = clamp(i32(id.x) + k * dir.x, 0, i32(u.glow_w) - 1);
        let y = clamp(i32(id.y) + k * dir.y, 0, i32(u.glow_h) - 1);
        sum += src[at(u32(x), u32(y), u.glow_w)] * w;
        weight += w;
    }
    dst[at(id.x, id.y, u.glow_w)] = sum / weight;
}

@compute @workgroup_size(8, 8)
fn blur_h(@builtin(global_invocation_id) id: vec3<u32>) {
    blur(id.xy, vec2<i32>(1, 0));
}

@compute @workgroup_size(8, 8)
fn blur_v(@builtin(global_invocation_id) id: vec3<u32>) {
    blur(id.xy, vec2<i32>(0, 1));
}

// ---------- P12: glow, halation, spectral bloom, grade, Dry/Wet ----------

// src: the source frame; aux: the screen pass; glowbuf: the blurred screen.
@compute @workgroup_size(8, 8)
fn finish(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.out_w || id.y >= u.out_h) {
        return;
    }
    let i = at(id.x, id.y, u.out_w);
    let screen_px = aux[i];
    var colour = screen_px.rgb;
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(f32(u.out_w), f32(u.out_h));
    let glow_res = vec2<f32>(f32(u.glow_w), f32(u.glow_h));
    let g = bilinear_glow(uv * glow_res);
    colour += g * u.glow * 0.8;
    // halation: highlights bleeding a warm red halo through the glass
    colour += g * u.halation * vec3<f32>(1.2, 0.48, 0.26);
    // spectral bloom: the glow's red spreads outward and its blue inward
    let c = uv - 0.5;
    let red = bilinear_glow((0.5 + c * (1.0 + 0.02 * u.spectral)) * glow_res).r;
    let blue = bilinear_glow((0.5 + c * (1.0 - 0.02 * u.spectral)) * glow_res).b;
    colour += vec3<f32>(red, g.g, blue) * u.spectral * 0.9;

    let graded = (colour - 0.5) * u.contrast + 0.5 + u.brightness;
    let source = src[i];
    dst[i] = vec4<f32>(source.rgb + (graded - source.rgb) * u.mix_amount,
                       source.a + (screen_px.a - source.a) * u.mix_amount);
}
