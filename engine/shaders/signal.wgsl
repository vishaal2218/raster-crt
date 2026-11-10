// Raster CRT signal chain, all at the processing raster:
//   raster (P0) -> colour (P1-P2, RGB in, YIQ out) -> bend_line + bend_decode (P3, the
//   composite signal) -> damage (P4 + P5 noise) -> decode (P6 + P7 waves, RGB out).
// Our own maths. Every random choice comes from the frame seed, so renders repeat exactly.

const LINE_TAPS = 16;

// ---------- P0: the processing raster (box filter over each raster pixel's footprint) ----------

@compute @workgroup_size(8, 8)
fn raster(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.ras_w || id.y >= u.ras_h) {
        return;
    }
    let step = vec2<f32>(f32(u.src_w) / f32(u.ras_w), f32(u.src_h) / f32(u.ras_h));
    let start = vec2<f32>(id.xy) * step;
    let x0 = u32(floor(start.x));
    let y0 = u32(floor(start.y));
    let x1 = min(u32(ceil(start.x + step.x)), u.src_w);
    let y1 = min(u32(ceil(start.y + step.y)), u.src_h);
    var sum = vec4<f32>(0.0);
    var count = 0.0;
    for (var y = y0; y < y1; y++) {
        for (var x = x0; x < x1; x++) {
            sum += src[at(x, y, u.src_w)];
            count += 1.0;
        }
    }
    dst[at(id.x, id.y, u.ras_w)] = sum / max(count, 1.0);
}

// ---------- P1-P2: Colour Signal, along each line ----------

// Gaussian average of chroma along the line, centred `lag` pixels to the left (chroma
// trails luma on tape). Taps are spread out for wide blurs so the reach stays the same at
// every raster width.
fn chroma_smear(x: i32, line: i32, sigma: f32, lag: f32) -> vec2<f32> {
    let stride = max(1.0, sigma * 2.5 / f32(LINE_TAPS));
    var sum = vec2<f32>(0.0);
    var weight = 0.0;
    for (var k = -LINE_TAPS; k <= LINE_TAPS; k++) {
        let d = f32(k) * stride;
        let w = exp(-0.5 * d * d / (sigma * sigma));
        sum += rgb_to_yiq(line_src_ras(f32(x) + 0.5 - lag + d, line).rgb).yz * w;
        weight += w;
    }
    return sum / weight;
}

// Luma glow: a wide average of the highlights.
fn luma_glow(x: i32, line: i32, sigma: f32) -> f32 {
    let stride = max(1.0, sigma * 2.5 / f32(LINE_TAPS));
    var sum = 0.0;
    var weight = 0.0;
    for (var k = -LINE_TAPS; k <= LINE_TAPS; k++) {
        let d = f32(k) * stride;
        let w = exp(-0.5 * d * d / (sigma * sigma));
        sum += max(dot(line_src_ras(f32(x) + 0.5 + d, line).rgb, LUMA) - 0.55, 0.0) * w;
        weight += w;
    }
    return sum / weight;
}

@compute @workgroup_size(8, 8)
fn colour(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.ras_w || id.y >= u.ras_h) {
        return;
    }
    let x = i32(id.x);
    let line = i32(id.y);
    let width = f32(u.ras_w);
    let here = rgb_to_yiq(src_ras(x, line).rgb);

    var luma = here.x;
    if (u.sharpen > 0.0) {
        // edge sharpening rings a little, like an over-peaked video amplifier
        let near = (dot(src_ras(x - 1, line).rgb, LUMA) + dot(src_ras(x + 1, line).rgb, LUMA)) * 0.25 + luma * 0.5;
        luma += u.sharpen * 1.6 * (luma - near);
    }
    if (u.signal_glow > 0.0) {
        luma += u.signal_glow * 1.2 * luma_glow(x, line, width * 0.008);
    }
    luma *= u.gain;

    var chroma = here.yz;
    if (u.bleed > 0.0) {
        chroma = mix(chroma, chroma_smear(x, line, max(u.bleed * width * 0.006, 0.3), u.bleed * width * 0.003), smoothstep(0.0, 0.05, u.bleed));
    }
    chroma *= u.chroma;
    if (u.hue > 0.0) {
        let angle = u.hue * 0.9 + u.hue * 0.12 * sin(f32(line) * 0.031 + u.time * 1.7);
        let c = cos(angle);
        let s = sin(angle);
        chroma = vec2<f32>(chroma.x * c - chroma.y * s, chroma.x * s + chroma.y * c);
    }
    dst[at(id.x, id.y, u.ras_w)] = vec4<f32>(luma, chroma, 1.0);
}

// ---------- P3: Circuit Bend, on the composite signal ----------

// Composite video: luma plus chroma riding a subcarrier whose phase flips every line (NTSC).
fn subcarrier_phase(x: u32, line: u32) -> f32 {
    return TAU * (f32(x) * u.subcarrier + select(0.0, 0.5, (line & 1u) == 1u));
}

// One thread per line: the resonant filter has memory, so a line runs left to right.
// Output .x is the bent composite.
@compute @workgroup_size(64)
fn bend_line(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.ras_h) {
        return;
    }
    let line = id.x;
    let w = u.ras_w;
    // Bend Motion makes the resonance wander up and down the picture and over time
    let drift = 1.0 + 0.25 * u.bend_motion * sin(f32(line) * 0.07 + u.time * 4.0 * sign(u.bend_motion));
    let w0 = TAU * clamp(u.bend_freq * drift, 0.002, 0.45);
    // peaking filter (boost in dB at the bend frequency, sharpness Q)
    let gain = pow(10.0, u.bend_boost / 40.0);
    let alpha = sin(w0) / (2.0 * u.bend_q);
    let a0 = 1.0 + alpha / gain;
    let b0 = (1.0 + alpha * gain) / a0;
    let b1 = -2.0 * cos(w0) / a0;
    let b2 = (1.0 - alpha * gain) / a0;
    let a2 = (1.0 - alpha / gain) / a0;
    let delay = u32(round(u.feedback_delay));
    var x1 = 0.0;
    var x2 = 0.0;
    var y1 = 0.0;
    var y2 = 0.0;
    for (var x = 0u; x < w; x++) {
        let s = src[at(x, line, w)].xyz;
        let phase = subcarrier_phase(x, line);
        let composite = s.x + s.y * cos(phase) + s.z * sin(phase);
        // a1 equals b1 for a peaking filter
        let y = b0 * composite + b1 * x1 + b2 * x2 - b1 * y1 - a2 * y2;
        x2 = x1;
        x1 = composite;
        y2 = y1;
        y1 = y;
        var bent = clamp(y, u.clip_low, u.clip_high);
        if (x >= delay) {
            bent += u.feedback * dst[at(x - delay, line, w)].x;
        }
        if (u.bend_ring > 0.0) {
            // the resonance rings and locks onto the subcarrier near 3.58 MHz; how far it is
            // detuned picks the hue it injects, and out of lock range it drifts into bands
            let unlock = smoothstep(0.35, 0.6, abs(u.bend_detune));
            let carrier = phase + TAU * u.bend_detune * 1.6 + 1.5707963 + unlock * TAU * f32(x) * (u.bend_freq * drift - u.subcarrier);
            bent += u.bend_ring * (0.25 + 0.6 * s.x) * 0.5 * cos(carrier);
        }
        dst[at(x, line, w)] = vec4<f32>(mix(composite, bent, u.bend), 0.0, 0.0, 0.0);
    }
}

// Back to luma and chroma: average over about two subcarrier cycles to split them.
// aux holds the YIQ from before the bend, which shows through as the bend fades out.
@compute @workgroup_size(8, 8)
fn bend_decode(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.ras_w || id.y >= u.ras_h) {
        return;
    }
    let i = at(id.x, id.y, u.ras_w);
    let before = aux[i].xyz;
    if (u.bend <= 0.0) {
        dst[i] = vec4<f32>(before, 1.0);
        return;
    }
    let reach = max(1, i32(round(1.0 / u.subcarrier)));
    var sum = vec3<f32>(0.0);
    var count = 0.0;
    for (var k = -reach; k <= reach; k++) {
        let x = clamp_to(i32(id.x) + k, u.ras_w);
        let c = src[at(x, id.y, u.ras_w)].x;
        let phase = subcarrier_phase(x, id.y);
        sum += vec3<f32>(c, 2.0 * c * cos(phase), 2.0 * c * sin(phase));
        count += 1.0;
    }
    dst[i] = vec4<f32>(mix(before, sum / count, smoothstep(0.0, 0.1, u.bend)), 1.0);
}

// ---------- P4 + P5: Damage and Noise ----------

// How far this line is pushed sideways (raster pixels).
fn line_offset(ys: f32, line: u32, h: f32, w: f32) -> f32 {
    var dx = u.shake * (vnoise(f32(line) * 0.15, u.seed) - 0.5) * w * 0.012;
    // sync tear: the top of the picture flags sideways, and a tear travels slowly down
    let flag = exp(-ys / (h * 0.06)) * (0.6 + 0.4 * sin(u.time * 7.0));
    let tear_y = fract(u.time * 0.07 + rnd(u.seed / 90u, 7u, 3u)) * h;
    let below = ys - tear_y;
    let tear = smoothstep(0.0, h * 0.02, below) * exp(-max(below, 0.0) / (h * 0.08));
    dx += u.tear * w * (0.05 * flag + 0.06 * tear);
    if (u.head_switch > 0.0 && ys >= h * 0.975) {
        dx += u.head_switch * w * (0.03 + 0.03 * rnd(line, u.seed, 11u));
    }
    return dx;
}

@compute @workgroup_size(8, 8)
fn damage(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.ras_w || id.y >= u.ras_h) {
        return;
    }
    let i = at(id.x, id.y, u.ras_w);
    let w = f32(u.ras_w);
    let h = f32(u.ras_h);
    let x0 = f32(id.x);
    let seed = u.seed;

    // vertical roll, with the blanking bar between frames showing as it passes
    var ys = f32(id.y);
    if (u.roll > 0.0) {
        let span = h * 1.06;
        ys = (ys + fract(u.time * 0.35 * u.roll) * span) % span;
        if (ys >= h) {
            dst[i] = vec4<f32>(0.0, 0.0, 0.0, 1.0);
            return;
        }
    }
    let line = u32(ys);

    // dropouts: a random stretch of a random line, in the chosen style
    let d_start = rnd(line, seed, 29u) * w;
    let d_len = (0.05 + 0.3 * rnd(line, seed, 31u)) * w;
    let dropped = rnd(line, seed, 23u) < u.dropout * 0.03 && x0 >= d_start && x0 < d_start + d_len;

    var dx = line_offset(ys, line, h, w);
    if (dropped && u.dropout_style == 4u) {
        dx += w * 0.05 * (rnd(line, seed, 37u) - 0.3);
    }
    var source_line = i32(line);
    if (dropped && u.dropout_style == 2u) {
        source_line = max(source_line - 1 - i32(rnd(line, seed, 41u) * 3.0), 0);
    }
    let sx = x0 + 0.5 - dx;
    var s = line_src_ras(sx, source_line).xyz;
    if (u.echo != 0.0) {
        // ghost: a delayed copy of the line; negative echo arrives inverted
        s += u.echo * 0.4 * line_src_ras(sx - w * 0.035, source_line).xyz;
    }
    s.x *= 1.0 - u.hum * 0.3 * (0.5 + 0.5 * sin(TAU * (ys / h + u.time * 0.15)));
    if (rnd(seed, 3u, 5u) < u.flashes * 0.12) {
        s.x = s.x * 1.5 + 0.12;
    }
    if (dropped) {
        switch u.dropout_style {
            case 0u: { s = vec3<f32>(0.85 + 0.15 * rnd(id.x / 3u, line, seed), 0.0, 0.0); }
            case 1u: { s = vec3<f32>(0.02, 0.0, 0.0); }
            case 3u: { s = vec3<f32>(s.x, 0.0, 0.0); }
            default: {}
        }
    }
    if (u.head_switch > 0.0 && ys >= h * 0.975) {
        s = vec3<f32>(s.x + u.head_switch * 0.15 * (rnd(id.x / 2u, line, seed) - 0.5), s.yz * mix(1.0, 0.3, u.head_switch));
    }
    if (u.top_line > 0.0 && ys < h * 0.015) {
        let n = vec3<f32>(rnd(id.x / 2u, line, seed ^ 0x55u), rnd(id.x / 2u, line, seed ^ 0x77u), rnd(id.x / 2u, line, seed ^ 0x99u)) - 0.5;
        s += u.top_line * n * vec3<f32>(0.5, 0.6, 0.6);
    }

    // tape grain, stretched along the line
    if (u.grain > 0.0) {
        let gx = id.x / 2u;
        s += u.grain * vec3<f32>(0.12 * (rnd(gx, line, seed) - 0.5), 0.06 * (rnd(gx, line, seed ^ 0x1234u) - 0.5), 0.06 * (rnd(gx, line, seed ^ 0x4321u) - 0.5));
    }
    // RF specks: rare sparks, longer as Speck Size grows
    if (u.specks > 0.0) {
        let cell = u32(x0 / (1.0 + u.speck_size * 8.0));
        if (rnd(cell, line, seed ^ 0xabcdu) > 1.0 - u.specks * 0.006) {
            let level = select(0.05, 1.0, rnd(cell, line, seed ^ 0x5555u) > 0.3);
            s = vec3<f32>(mix(s.x, level, 0.85), s.yz * 0.2);
        }
    }
    dst[i] = vec4<f32>(s, 1.0);
}

// ---------- P6 + P7: decode, RGB split, Clean Signal, Waves ----------

fn wave_band(phase: f32) -> f32 {
    let s = 0.5 + 0.5 * sin(TAU * phase);
    let edge = 0.02 + 0.06 * u.wave_smooth;
    return smoothstep(0.5 - edge, 0.5 + edge, s);
}

fn waves(rgb: vec3<f32>, p: vec2<f32>) -> vec3<f32> {
    let along = select(p.y / f32(u.ras_h), p.x / f32(u.ras_w), u.wave_vertical == 1u);
    let direction = select(1.0, -1.0, u.wave_backward == 1u);
    let phase = along * u.wave_count + direction * u.time * 0.6 + u.wave_react * 8.0 * dot(rgb, LUMA);
    var w = vec3<f32>(wave_band(phase));
    if (u.wave_rgb == 1u) {
        w = vec3<f32>(wave_band(phase), wave_band(phase + 1.0 / 3.0), wave_band(phase + 2.0 / 3.0));
    }
    var layer: vec3<f32>;
    switch u.wave_blend {
        case 1u: { layer = rgb * w; }
        case 2u: { layer = 1.0 - (1.0 - rgb) * (1.0 - w); }
        case 3u: { layer = rgb + w * 0.5; }
        // normal: the bands light and darken the picture itself, so it stays visible
        default: { layer = rgb * (0.35 + 1.3 * w); }
    }
    return mix(rgb, layer, u.wave_mix);
}

// src: YIQ from the damage pass; aux: the clean RGB raster.
@compute @workgroup_size(8, 8)
fn decode(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.ras_w || id.y >= u.ras_h) {
        return;
    }
    let i = at(id.x, id.y, u.ras_w);
    let clean = aux[i].rgb;
    var rgb = clean;
    if (u.signal_on == 1u) {
        let x = f32(id.x) + 0.5;
        let line = i32(id.y);
        rgb = yiq_to_rgb(line_src_ras(x, line).xyz);
        if (u.split > 0.0) {
            let s = u.split * f32(u.ras_w) * 0.004;
            rgb = vec3<f32>(yiq_to_rgb(line_src_ras(x - s, line).xyz).r, rgb.g, yiq_to_rgb(line_src_ras(x + s, line).xyz).b);
        }
        rgb = mix(rgb, clean, u.clean_signal);
        if (u.wave_mix > 0.0) {
            rgb = waves(rgb, vec2<f32>(x, f32(id.y) + 0.5));
        }
    }
    dst[i] = vec4<f32>(rgb, 1.0);
}
