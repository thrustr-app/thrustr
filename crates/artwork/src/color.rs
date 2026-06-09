use domain::artwork::Color;
use image::{DynamicImage, imageops::FilterType};

const SAMPLE_SIZE: u32 = 96;

const QUANT_BITS: u32 = 4;
const QUANT_SHIFT: u32 = 8 - QUANT_BITS;
const QUANT_LEVELS: usize = 1 << QUANT_BITS;

// This avoids super dark or washed-out colors.
const MIN_LIGHTNESS: f32 = 0.38;
const MAX_LIGHTNESS: f32 = 0.62;

// If a color is too desaturated, bump it up to at least this level
// so it looks good as an accent color.
const MIN_SATURATION: f32 = 0.55;

struct Bucket {
    count: u64,
    r: u64,
    g: u64,
    b: u64,
}

/// Extracts a strong, vibrant accent color out of an image.
///
/// The process is:
/// 1. Shrink the image to reduce processing time
/// 2. Group similar colors together into buckets
/// 3. Score each bucket based on how common it is and how visually "interesting" it is
/// 4. Pick the best one and adjust it so it works well in the UI
pub fn extract_accent(img: &DynamicImage) -> Option<Color> {
    let sample = img
        .resize(SAMPLE_SIZE, SAMPLE_SIZE, FilterType::Triangle)
        .to_rgb8();

    let mut buckets: Vec<Bucket> = (0..QUANT_LEVELS.pow(3))
        .map(|_| Bucket {
            count: 0,
            r: 0,
            g: 0,
            b: 0,
        })
        .collect();

    for pixel in sample.pixels() {
        let [r, g, b] = pixel.0;
        let idx = bucket_index(r, g, b);
        let bucket = &mut buckets[idx];
        bucket.count += 1;
        bucket.r += r as u64;
        bucket.g += g as u64;
        bucket.b += b as u64;
    }

    let mut best_score = 0.0_f32;
    let mut best: Option<Color> = None;

    for bucket in buckets.iter().filter(|b| b.count > 0) {
        let count = bucket.count as f32;

        let r = (bucket.r as f32 / count) as u8;
        let g = (bucket.g as f32 / count) as u8;
        let b = (bucket.b as f32 / count) as u8;

        let score = count * (1.0 + vibrancy_weight(r, g, b));
        if score > best_score {
            best_score = score;
            best = Some(Color { r, g, b });
        }
    }

    best.map(|c| normalize(c.r, c.g, c.b))
}

fn bucket_index(r: u8, g: u8, b: u8) -> usize {
    let r = (r >> QUANT_SHIFT) as usize;
    let g = (g >> QUANT_SHIFT) as usize;
    let b = (b >> QUANT_SHIFT) as usize;
    (r * QUANT_LEVELS + g) * QUANT_LEVELS + b
}

fn vibrancy_weight(r: u8, g: u8, b: u8) -> f32 {
    let (rf, gf, bf) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);

    let saturation = if max <= 0.0 { 0.0 } else { (max - min) / max };
    let luminance = 0.299 * rf + 0.587 * gf + 0.114 * bf;

    let luma_weight = (1.0 - (luminance - 0.45).abs() / 0.45).max(0.1);

    saturation * luma_weight
}

fn normalize(r: u8, g: u8, b: u8) -> Color {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let (h, s, l) = rgb_to_hsl(rf, gf, bf);
    let l_norm = l.clamp(MIN_LIGHTNESS, MAX_LIGHTNESS);
    let s_norm = s.max(MIN_SATURATION);

    let (nr, ng, nb) = hsl_to_rgb(h, s_norm, l_norm);
    Color {
        r: (nr * 255.0).round() as u8,
        g: (ng * 255.0).round() as u8,
        b: (nb * 255.0).round() as u8,
    }
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }

    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if r >= g && r >= b {
        let mut h = (g - b) / d;
        if h < 0.0 {
            h += 6.0;
        }
        h / 6.0
    } else if g >= b {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };

    (h, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-6 {
        return (l, l, l);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    (
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    )
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}
