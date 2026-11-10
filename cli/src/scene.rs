//! A small synthetic landscape for preset thumbnails: sky, sun, hills, colour patches and
//! fine stripes. Drawn from maths only, so the thumbnails contain nothing but our own work.

use raster_engine::Image;

const HORIZON: f32 = 0.62;
const SKY_TOP: [f32; 3] = [0.10, 0.18, 0.45];
const SKY_LOW: [f32; 3] = [0.95, 0.55, 0.35];
const SUN: [f32; 3] = [1.0, 0.86, 0.48];
const FAR_HILLS: [f32; 3] = [0.36, 0.26, 0.46];
const NEAR_HILLS: [f32; 3] = [0.12, 0.26, 0.22];
const GROUND: [f32; 3] = [0.08, 0.12, 0.10];
const PATCHES: [[f32; 3]; 6] = [
    [0.85, 0.15, 0.15], [0.15, 0.75, 0.25], [0.15, 0.30, 0.85],
    [0.90, 0.85, 0.20], [0.20, 0.80, 0.85], [0.85, 0.25, 0.80],
];

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

fn pixel(x: f32, y: f32, aspect: f32) -> [f32; 3] {
    // ground: colour patches on the left, fine black/white stripes on the right
    if y > 0.8 {
        if (0.06..0.54).contains(&x) && (0.84..0.95).contains(&y) {
            let i = ((x - 0.06) / 0.08) as usize;
            let inside = ((x - 0.06) % 0.08) < 0.07;
            return if inside { PATCHES[i.min(5)] } else { GROUND };
        }
        if (0.62..0.94).contains(&x) && (0.84..0.95).contains(&y) {
            let stripe = ((x * 400.0) as i32) % 2 == 0;
            return if stripe { [0.95; 3] } else { [0.05; 3] };
        }
        return GROUND;
    }
    let near = 0.68 + 0.035 * (x * 5.0 + 2.0).sin() + 0.015 * (x * 17.0).sin();
    if y > near {
        return NEAR_HILLS;
    }
    let far = 0.56 + 0.05 * (x * 9.0).sin() + 0.025 * (x * 23.0 + 1.0).sin();
    if y > far {
        return FAR_HILLS;
    }
    let sky = mix3(SKY_TOP, SKY_LOW, (y / HORIZON).clamp(0.0, 1.0).powf(1.6));
    let d = (((x - 0.68) * aspect).powi(2) + (y - 0.46).powi(2)).sqrt();
    if d < 0.085 {
        return SUN;
    }
    mix3(sky, SUN, (1.0 - (d - 0.085) / 0.12).clamp(0.0, 1.0).powi(2) * 0.6)
}

pub fn scene(width: u32, height: u32) -> Image {
    let aspect = width as f32 / height as f32;
    let pixels = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let rgb = pixel((x as f32 + 0.5) / width as f32, (y as f32 + 0.5) / height as f32, aspect);
            [rgb[0], rgb[1], rgb[2], 1.0]
        })
        .collect();
    Image::new(width, height, pixels).expect("scene size is never zero")
}
