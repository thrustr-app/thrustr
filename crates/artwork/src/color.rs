use domain::artwork::Color;
use image::{DynamicImage, imageops::FilterType};

const SAMPLE_SIZE: u32 = 96;

const QUANT_BITS: u32 = 4;
const QUANT_SHIFT: u32 = 8 - QUANT_BITS;
const QUANT_LEVELS: usize = 1 << QUANT_BITS;

struct Bucket {
    count: u64,
    r: u64,
    g: u64,
    b: u64,
}

/// Extracts a vibrant background color from an image.
///
/// Downscales, quantizes pixels into a coarse color histogram, then scores each bucket
/// by population weighted towards saturated, mid-luminance colors so the result is a
/// lively accent rather than a dull average or a near-black/white extreme.
pub fn extract_vibrant(img: &DynamicImage) -> Option<Color> {
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

        let score = count * vibrancy_weight(r, g, b);
        if score > best_score {
            best_score = score;
            best = Some(Color { r, g, b });
        }
    }

    best
}

fn bucket_index(r: u8, g: u8, b: u8) -> usize {
    let r = (r >> QUANT_SHIFT) as usize;
    let g = (g >> QUANT_SHIFT) as usize;
    let b = (b >> QUANT_SHIFT) as usize;
    (r * QUANT_LEVELS + g) * QUANT_LEVELS + b
}

/// Weights a color by saturation and how far its luminance sits from pure black/white,
/// so flat dark/light backgrounds lose out to colorful regions.
fn vibrancy_weight(r: u8, g: u8, b: u8) -> f32 {
    let (rf, gf, bf) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);

    let saturation = if max <= 0.0 { 0.0 } else { (max - min) / max };
    let luminance = 0.299 * rf + 0.587 * gf + 0.114 * bf;
    // Tent function peaking at mid luminance, zero at the extremes.
    let luma_weight = 1.0 - (luminance - 0.5).abs() * 2.0;

    // Keep a small floor so a fully desaturated-but-mid image still yields a color.
    0.1 + saturation * luma_weight
}
