//! This module handles accent color extraction from game artwork.
//!
//! Most values here have been obtained by trial and error with random
//! Steam game covers and comparing results. It is not by any means
//! perfect, but should be good enough for most cases.

use domain::artwork::Color;
use image::{DynamicImage, imageops::FilterType};
use std::f32::consts::TAU;

const SAMPLE_SIZE: u32 = 128;

// Ignore pixels that are mostly transparent.
const MIN_ALPHA: u8 = 128;

// Count neighbor hue bins too for when a color sits right between two bins.
const HUE_BINS: usize = 36;
const NEIGHBOR_WEIGHT: f32 = 0.6;

// Ignore colors that are almost gray and prioritize stronger colors.
const CHROMA_FLOOR: f32 = 0.025;
const CHROMA_EXPONENT: f32 = 1.5;

// Colors near pure black or white get less weight but are not completely
// ignored.
const CLEAREST_LIGHTNESS: f32 = 0.60;
const LIGHTNESS_FALLOFF: f32 = 0.55;
const MIN_LIGHTNESS_WEIGHT: f32 = 0.05;

// Faces and hands can take up a lot of the picture and don't
// look very good as an accent color, so their weight is reduced.
const SKIN_HUE: std::ops::Range<f32> = 25.0..75.0;
const SKIN_MAX_CHROMA: f32 = 0.105;
const SKIN_WEIGHT: f32 = 0.25;

// If the image has almost no real color, don't make up a random hue and
// just use gray instead.
const COLORFUL_CHROMA: f32 = 0.045;
const MONOCHROME_RATIO: f32 = 0.005;

// Keep the accent color in this range so it looks good in the UI.
const MIN_ACCENT_LIGHTNESS: f32 = 0.55;
const MAX_ACCENT_LIGHTNESS: f32 = 0.70;
const MIN_ACCENT_CHROMA: f32 = 0.10;
const MAX_ACCENT_CHROMA: f32 = 0.19;

/// Stores the collected information for one hue.
///
/// The values are added together while looking at the image.
/// Colors with more votes have a bigger effect on the final result.
#[derive(Default, Clone, Copy)]
struct Hue {
    weight: f32,
    lightness: f32,
    chroma: f32,
    sin: f32,
    cos: f32,
}

impl Hue {
    /// Adds another hue's information to this one.
    ///
    /// `scale` is how much the other hue should count.
    fn add(self, other: &Self, scale: f32) -> Self {
        Self {
            weight: self.weight + other.weight * scale,
            lightness: self.lightness + other.lightness * scale,
            chroma: self.chroma + other.chroma * scale,
            sin: self.sin + other.sin * scale,
            cos: self.cos + other.cos * scale,
        }
    }
}

/// Extracts a strong, vibrant accent color out of an image.
///
/// Every pixel votes for its own hue, and a vivid pixel has more weight
/// than a dark or washed-out one. The hue with the most votes wins,
/// and the average color of its voters is then adjusted for the UI.
pub fn extract_accent(img: &DynamicImage) -> Option<Color> {
    accent(img).map(|(color, _)| color)
}

/// Where an accent color came from.
/// This exists pretty much for tests since the caller doesn't need to
/// know the source of an accent.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Source {
    Hue,
    Monochrome,
}

fn accent(img: &DynamicImage) -> Option<(Color, Source)> {
    let sample = img
        .resize(SAMPLE_SIZE, SAMPLE_SIZE, FilterType::Triangle)
        .to_rgba8();

    let mut hues = [Hue::default(); HUE_BINS];
    let mut opaque = 0.0_f32;
    let mut colorful = 0.0_f32;
    let mut total_lightness = 0.0_f32;

    for pixel in sample.pixels() {
        let [r, g, b, alpha] = pixel.0;
        if alpha < MIN_ALPHA {
            continue;
        }

        // Oklab gives us lightness and two color values (which direction the color points in).
        // The distance from the center tells how colorful the pixel is, while the angle tells
        // which hue it has.
        let (lightness, green_red, blue_yellow) = srgb_to_oklab(r, g, b);
        let chroma = (green_red * green_red + blue_yellow * blue_yellow).sqrt();
        let hue = blue_yellow.atan2(green_red);
        let weight = chroma_weight(chroma) * lightness_weight(lightness) * skin_weight(hue, chroma);

        opaque += 1.0;
        total_lightness += lightness;

        let bin = &mut hues[hue_bin(hue)];
        bin.weight += weight;
        bin.lightness += weight * lightness;
        bin.chroma += weight * chroma;
        // Hue is a circle, so we can't just average the angles. For example, red and violet
        // are close, but averaging their numbers would give us some kind of green.
        bin.sin += weight * hue.sin();
        bin.cos += weight * hue.cos();

        if chroma >= COLORFUL_CHROMA {
            colorful += 1.0;
        }
    }

    // Nothing solid enough to take a color from.
    if opaque == 0.0 {
        return None;
    }

    if colorful / opaque < MONOCHROME_RATIO {
        return Some((gray(total_lightness / opaque), Source::Monochrome));
    }

    // Any pixel that's colorful enough to get past the ckeck above
    // has positive weight.
    let winner = neighborhood(&hues, strongest(&hues));
    assert!(
        winner.weight > 0.0,
        "a colorful image should have a dominant hue"
    );

    let color = oklch(
        (winner.lightness / winner.weight).clamp(MIN_ACCENT_LIGHTNESS, MAX_ACCENT_LIGHTNESS),
        (winner.chroma / winner.weight).clamp(MIN_ACCENT_CHROMA, MAX_ACCENT_CHROMA),
        winner.sin.atan2(winner.cos),
    );

    Some((color, Source::Hue))
}

/// Finds the hue with the most votes, neighbors are counted too.
fn strongest(hues: &[Hue; HUE_BINS]) -> usize {
    let weight = |bin: usize| neighborhood(hues, bin).weight;

    (0..HUE_BINS)
        .max_by(|&a, &b| weight(a).total_cmp(&weight(b)))
        .unwrap_or(0)
}

/// Merges a bin with the one on either side, at a reduced weight. The wheel
/// wraps around, so the first and last bins are neighbors.
fn neighborhood(hues: &[Hue; HUE_BINS], bin: usize) -> Hue {
    let previous = hues[(bin + HUE_BINS - 1) % HUE_BINS];
    let next = hues[(bin + 1) % HUE_BINS];

    hues[bin]
        .add(&previous, NEIGHBOR_WEIGHT)
        .add(&next, NEIGHBOR_WEIGHT)
}

/// Turns a hue angle into the index of the bin that contains it.
fn hue_bin(hue: f32) -> usize {
    let turns = (hue / TAU).rem_euclid(1.0);
    ((turns * HUE_BINS as f32) as usize).min(HUE_BINS - 1)
}

fn chroma_weight(chroma: f32) -> f32 {
    (chroma - CHROMA_FLOOR).max(0.0).powf(CHROMA_EXPONENT)
}

fn lightness_weight(lightness: f32) -> f32 {
    let distance = ((lightness - CLEAREST_LIGHTNESS) / LIGHTNESS_FALLOFF).abs();

    (1.0 - distance).clamp(MIN_LIGHTNESS_WEIGHT, 1.0)
}

/// Gives less weight to skin (dull orange) colors so that face
/// and hands don't become the accent easily.
fn skin_weight(hue: f32, chroma: f32) -> f32 {
    let degrees = hue.to_degrees().rem_euclid(360.0);
    if SKIN_HUE.contains(&degrees) && chroma < SKIN_MAX_CHROMA {
        SKIN_WEIGHT
    } else {
        1.0
    }
}

/// Makes a gray accent when the image does not have a useful hue.
fn gray(lightness: f32) -> Color {
    oklch(
        lightness.clamp(MIN_ACCENT_LIGHTNESS, MAX_ACCENT_LIGHTNESS),
        0.0,
        0.0,
    )
}

/// Converts to sRGB, lowering the chroma untile the color fits, so the hue
/// survives instead of being clipped.
fn oklch(lightness: f32, chroma: f32, hue: f32) -> Color {
    let (mut low, mut high) = (0.0, chroma);
    let mut fitted = oklab_to_srgb(lightness, 0.0, 0.0);

    for _ in 0..16 {
        let mid = (low + high) / 2.0;
        let rgb = oklab_to_srgb(lightness, mid * hue.cos(), mid * hue.sin());
        if in_gamut(rgb) {
            fitted = rgb;
            low = mid;
        } else {
            high = mid;
        }
    }

    let (r, g, b) = fitted;
    Color::rgb(encode_srgb(r), encode_srgb(g), encode_srgb(b))
}

/// Whether a color can actually be shown on an sRGB screen.
fn in_gamut((r, g, b): (f32, f32, f32)) -> bool {
    const EPSILON: f32 = 1e-4;
    let range = -EPSILON..=1.0 + EPSILON;
    range.contains(&r) && range.contains(&g) && range.contains(&b)
}

// Matrices from Björn Ottosson's Oklab reference implementation.
// https://bottosson.github.io/posts/oklab
#[allow(clippy::excessive_precision)]
fn srgb_to_oklab(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = decode_srgb(r);
    let g = decode_srgb(g);
    let b = decode_srgb(b);

    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();

    (
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    )
}

#[allow(clippy::excessive_precision)]
fn oklab_to_srgb(lightness: f32, green_red: f32, blue_yellow: f32) -> (f32, f32, f32) {
    let l = lightness + 0.3963377774 * green_red + 0.2158037573 * blue_yellow;
    let m = lightness - 0.1055613458 * green_red - 0.0638541728 * blue_yellow;
    let s = lightness - 0.0894841775 * green_red - 1.2914855480 * blue_yellow;

    let (l, m, s) = (l * l * l, m * m * m, s * s * s);

    (
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    )
}

fn decode_srgb(value: u8) -> f32 {
    let value = value as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn encode_srgb(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let value = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };

    (value * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};
    use std::ops::RangeInclusive;

    // Hues only need to be close enough, tests do not need the exact hue.
    const HUE_SLACK: f32 = 4.0;

    const LIGHTNESS_SLACK: f32 = 0.01;
    const EXPECTED_LIGHTNESS_RANGE: RangeInclusive<f32> =
        (MIN_ACCENT_LIGHTNESS - LIGHTNESS_SLACK)..=(MAX_ACCENT_LIGHTNESS + LIGHTNESS_SLACK);

    const MIN_ACCENT_SATURATION: f32 = 0.2;

    const RED: Color = Color::rgb(200, 30, 30);
    const GREEN: Color = Color::rgb(40, 160, 60);
    const BLUE: Color = Color::rgb(30, 120, 200);
    const SKIN: Color = Color::rgb(222, 176, 148);

    const SMALL: (u32, u32) = (64, 96);
    const COVER: (u32, u32) = (600, 900);

    #[derive(Debug)]
    enum Expected {
        None,
        Gray(Source),
        Hue(Color),
    }

    #[track_caller]
    fn check(img: DynamicImage, expected: Expected) {
        let Some((color, source)) = accent(&img) else {
            assert!(
                matches!(expected, Expected::None),
                "expected {expected:?}, got no accent"
            );
            return;
        };

        let lightness = lightness(color);
        assert!(
            EXPECTED_LIGHTNESS_RANGE.contains(&lightness),
            "{color:?} has lightness {lightness}, outside the expected range {EXPECTED_LIGHTNESS_RANGE:?}"
        );

        match expected {
            Expected::None => panic!("expected no accent, got {color:?}"),
            Expected::Gray(path) => {
                assert!(saturation(color) < 0.02, "expected a gray, got {color:?}");
                assert_eq!(source, path, "gray came from the wrong path");
            }
            Expected::Hue(wanted) => {
                assert_eq!(source, Source::Hue, "expected a hue, got {source:?}");

                let saturation = saturation(color);
                assert!(
                    saturation > MIN_ACCENT_SATURATION,
                    "{color:?} has saturation {saturation}, too washed out for an accent"
                );

                let (wanted, got) = (hue(wanted), hue(color));
                assert!(
                    hue_distance(got, wanted) < HUE_SLACK,
                    "expected the hue of {wanted} degrees, got {got} ({color:?})"
                );
            }
        }
    }

    fn solid(color: Color) -> DynamicImage {
        filled(SMALL, color)
    }

    fn filled((w, h): (u32, u32), color: Color) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(w, h, Rgb([color.r, color.g, color.b])))
    }

    /// Paints the top `share` of the image with the first color and the rest
    /// with the second one.
    fn split((w, h): (u32, u32), share: f32, top: Color, bottom: Color) -> DynamicImage {
        let mut img = RgbImage::new(w, h);
        let edge = (img.height() as f32 * share) as u32;
        for (_, y, pixel) in img.enumerate_pixels_mut() {
            let color = if y < edge { top } else { bottom };
            *pixel = Rgb([color.r, color.g, color.b]);
        }

        DynamicImage::ImageRgb8(img)
    }

    /// Paints a square of `color` in the corner of a gray image.
    fn patch(side: u32, color: Color) -> DynamicImage {
        let mut img = RgbImage::from_pixel(SMALL.0, SMALL.1, Rgb([128, 128, 128]));
        for y in 0..side {
            for x in 0..side {
                img.put_pixel(x, y, Rgb([color.r, color.g, color.b]));
            }
        }

        DynamicImage::ImageRgb8(img)
    }

    fn hue_color(degrees: f32) -> Color {
        let sector = degrees.rem_euclid(360.0) / 60.0;
        let byte = |value: f32| (value * 255.0).round() as u8;
        let (rise, fall) = (byte(sector.fract()), byte(1.0 - sector.fract()));

        match sector as u32 {
            0 => Color::rgb(255, rise, 0),
            1 => Color::rgb(fall, 255, 0),
            2 => Color::rgb(0, 255, rise),
            3 => Color::rgb(0, fall, 255),
            4 => Color::rgb(rise, 0, 255),
            _ => Color::rgb(255, 0, fall),
        }
    }

    /// Where the color sits on the wheel, in degrees.
    fn hue(color: Color) -> f32 {
        let (_, green_red, blue_yellow) = srgb_to_oklab(color.r, color.g, color.b);

        blue_yellow.atan2(green_red).to_degrees().rem_euclid(360.0)
    }

    fn hue_distance(a: f32, b: f32) -> f32 {
        let distance = (a - b).rem_euclid(360.0);
        distance.min(360.0 - distance)
    }

    fn saturation(color: Color) -> f32 {
        let (r, g, b, _) = color.normalized();
        r.max(g).max(b) - r.min(g).min(b)
    }

    fn lightness(color: Color) -> f32 {
        srgb_to_oklab(color.r, color.g, color.b).0
    }

    #[test]
    fn accents_keep_the_hue_of_the_source() {
        for color in [RED, BLUE, GREEN] {
            check(solid(color), Expected::Hue(color));
        }
    }

    #[test]
    fn dark_and_light_sources_still_land_in_the_ui_band() {
        for color in [Color::rgb(12, 4, 40), Color::rgb(250, 246, 200)] {
            check(solid(color), Expected::Hue(color));
        }
    }

    #[test]
    fn every_hue_on_the_wheel_comes_back_as_itself() {
        for degrees in (0..360).step_by(5) {
            let color = hue_color(degrees as f32);
            check(solid(color), Expected::Hue(color));
        }
    }

    #[test]
    fn covers_larger_than_the_sample_keep_their_hue() {
        check(filled(COVER, BLUE), Expected::Hue(BLUE));
        check(split(COVER, 0.5, SKIN, BLUE), Expected::Hue(BLUE));
    }

    #[test]
    fn grayscale_images_do_not_invent_a_hue() {
        let mut img = RgbImage::new(SMALL.0, SMALL.1);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let value = ((x + y) * 2) as u8;
            *pixel = Rgb([value, value, value]);
        }

        check(
            DynamicImage::ImageRgb8(img),
            Expected::Gray(Source::Monochrome),
        );
    }

    #[test]
    fn black_and_white_images_get_a_gray_the_ui_can_use() {
        for color in [Color::BLACK, Color::WHITE] {
            check(solid(color), Expected::Gray(Source::Monochrome));
        }
    }

    #[test]
    fn a_speck_of_color_does_not_carry_a_gray_image() {
        check(patch(4, BLUE), Expected::Gray(Source::Monochrome));
        check(patch(16, BLUE), Expected::Hue(BLUE));
    }

    #[test]
    fn skin_tones_lose_to_real_colors() {
        check(split(SMALL, 0.9, SKIN, BLUE), Expected::Hue(BLUE));
    }

    #[test]
    fn an_image_of_nothing_but_skin_still_gets_an_accent() {
        check(solid(SKIN), Expected::Hue(SKIN));
    }

    #[test]
    fn hues_average_around_the_wheel() {
        let (left, right) = (Color::rgb(214, 33, 118), Color::rgb(222, 41, 76));
        check(
            split(SMALL, 0.5, left, right),
            Expected::Hue(Color::rgb(218, 37, 97)),
        );
    }

    #[test]
    fn transparent_pixels_are_ignored() {
        let mut img = RgbaImage::from_pixel(SMALL.0, SMALL.1, Rgba([0, 0, 0, 0]));
        for (_, y, pixel) in img.enumerate_pixels_mut() {
            if y >= SMALL.1 - 16 {
                *pixel = Rgba(BLUE.to_rgba());
            }
        }

        check(DynamicImage::ImageRgba8(img), Expected::Hue(BLUE));
    }

    #[test]
    fn the_alpha_cutoff_decides_whether_a_pixel_counts() {
        for (alpha, expected) in [
            (MIN_ALPHA - 1, Expected::None),
            (MIN_ALPHA, Expected::Hue(RED)),
        ] {
            let img = RgbaImage::from_pixel(SMALL.0, SMALL.1, Rgba([RED.r, RED.g, RED.b, alpha]));

            check(DynamicImage::ImageRgba8(img), expected);
        }
    }

    #[test]
    fn fully_transparent_images_have_no_accent() {
        let img = RgbaImage::from_pixel(SMALL.0, SMALL.1, Rgba([RED.r, RED.g, RED.b, 0]));
        check(DynamicImage::ImageRgba8(img), Expected::None);
    }

    #[test]
    fn every_byte_survives_an_srgb_round_trip() {
        for value in 0..=u8::MAX {
            assert_eq!(encode_srgb(decode_srgb(value)), value);
        }
    }
}
