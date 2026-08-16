use crate::{ArtworkTask, color::extract_accent};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use domain::artwork::Color;
use image::{DynamicImage, ImageError, RgbImage, RgbaImage, imageops::FilterType};
use reqwest::{
    Client, StatusCode,
    header::{HeaderMap, RETRY_AFTER},
};
use std::time::Duration;
use thiserror::Error;
use tokio::task::{JoinError, spawn_blocking};
use webp::Encoder;

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);

const MAX_HEIGHT: u32 = 600;
const COVER_ASPECT: (u32, u32) = (2, 3);
const MIN_SIZE: (u32, u32) = COVER_ASPECT;

#[derive(Debug, Error)]
pub enum ProcessingError {
    /// No response arrived at all.
    #[error(transparent)]
    Request(#[from] reqwest::Error),

    /// A response arrived, but it was not an image.
    #[error("server responded with {status}")]
    Status {
        status: StatusCode,
        retry_after: Option<Duration>,
    },

    #[error(transparent)]
    Decode(#[from] ImageError),

    #[error("image is {width}x{height}, too small for a cover")]
    TooSmall { width: u32, height: u32 },

    #[error(transparent)]
    Task(#[from] JoinError),
}

pub struct ProcessedArtwork {
    pub bytes: Vec<u8>,
    pub hash: String,
    pub color: Option<Color>,
}

pub async fn process_task(
    task: &ArtworkTask,
    client: Client,
) -> Result<ProcessedArtwork, ProcessingError> {
    let bytes = download_image(&task.url, client).await?;
    let quality = task.quality;

    spawn_blocking(move || build_cover(&bytes, quality)).await?
}

fn build_cover(bytes: &[u8], quality: f32) -> Result<ProcessedArtwork, ProcessingError> {
    let img = decode_and_fit(bytes)?;
    let color = extract_accent(&img);
    let bytes = encode_webp(&img, quality);
    let hash = blake3::hash(&bytes).to_hex().to_string();

    Ok(ProcessedArtwork { bytes, hash, color })
}

async fn download_image(url: &str, client: Client) -> Result<Bytes, ProcessingError> {
    let response = client.get(url).timeout(DOWNLOAD_TIMEOUT).send().await?;

    let status = response.status();
    if !status.is_success() {
        return Err(ProcessingError::Status {
            status,
            retry_after: retry_after(response.headers(), Utc::now()),
        });
    }

    Ok(response.bytes().await?)
}

/// `now` is passed as a parameter so tests are deterministic.
fn retry_after(headers: &HeaderMap, now: DateTime<Utc>) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();

    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    let deadline = DateTime::parse_from_rfc2822(value).ok()?;
    (deadline.to_utc() - now).to_std().ok()
}

fn decode_and_fit(bytes: &[u8]) -> Result<DynamicImage, ProcessingError> {
    let img = image::load_from_memory(bytes)?;

    let (width, height) = (img.width(), img.height());
    if width < MIN_SIZE.0 || height < MIN_SIZE.1 {
        return Err(ProcessingError::TooSmall { width, height });
    }

    let img = crop_to_aspect_ratio(img, COVER_ASPECT);
    Ok(resize_to_max_height(img, MAX_HEIGHT))
}

fn encode_webp(img: &DynamicImage, quality: f32) -> Vec<u8> {
    match img {
        DynamicImage::ImageRgb8(rgb) => encode_rgb(rgb, quality),
        DynamicImage::ImageRgba8(rgba) => encode_rgba(rgba, quality),
        other if other.color().has_alpha() => encode_rgba(&other.to_rgba8(), quality),
        other => encode_rgb(&other.to_rgb8(), quality),
    }
}

fn encode_rgb(img: &RgbImage, quality: f32) -> Vec<u8> {
    Encoder::from_rgb(img.as_raw(), img.width(), img.height())
        .encode(quality)
        .to_vec()
}

fn encode_rgba(img: &RgbaImage, quality: f32) -> Vec<u8> {
    Encoder::from_rgba(img.as_raw(), img.width(), img.height())
        .encode(quality)
        .to_vec()
}

fn crop_to_aspect_ratio(img: DynamicImage, (target_w, target_h): (u32, u32)) -> DynamicImage {
    let (w, h) = (img.width(), img.height());

    let (crop_w, crop_h) = if w * target_h > h * target_w {
        (h * target_w / target_h, h)
    } else {
        (w, w * target_h / target_w)
    };

    img.crop_imm((w - crop_w) / 2, (h - crop_h) / 2, crop_w, crop_h)
}

fn resize_to_max_height(img: DynamicImage, max_h: u32) -> DynamicImage {
    if img.height() <= max_h {
        return img;
    }

    img.resize(u32::MAX, max_h, FilterType::Lanczos3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, ImageFormat, Luma, Rgb, Rgba};
    use reqwest::header::HeaderValue;
    use std::io::Cursor;

    const RED: Rgb<u8> = Rgb([200, 30, 30]);
    const BLUE: Rgb<u8> = Rgb([30, 120, 200]);

    /// Tests are measured agains this date.
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc2822("Wed, 21 Oct 2099 03:46:00 GMT")
            .expect("the test date should parse")
            .to_utc()
    }

    #[track_caller]
    fn check_retry_after(header: Option<&'static str>, expected: Option<Duration>) {
        let mut headers = HeaderMap::new();
        if let Some(value) = header {
            headers.insert(RETRY_AFTER, HeaderValue::from_static(value));
        }

        assert_eq!(retry_after(&headers, now()), expected, "header {header:?}");
    }

    #[test]
    fn retry_after_reads_a_delay() {
        for (header, expected) in [
            ("120", Duration::from_secs(120)),
            ("0", Duration::ZERO),
            ("  120  ", Duration::from_secs(120)),
            ("Wed, 21 Oct 2099 03:48:00 GMT", Duration::from_secs(120)),
        ] {
            check_retry_after(Some(header), Some(expected));
        }
    }

    #[test]
    fn retry_after_ignores_what_it_cannot_use() {
        for header in [
            None,
            Some("soon"),
            Some("-5"),
            // A date that has already passed.
            Some("Wed, 30 Jun 2015 16:13:00 GMT"),
        ] {
            check_retry_after(header, None);
        }
    }

    fn image(w: u32, h: u32, color: Rgb<u8>) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(w, h, color))
    }

    fn png(w: u32, h: u32, color: Rgb<u8>) -> Vec<u8> {
        encode_png(image(w, h, color))
    }

    fn gradient_png(w: u32, h: u32) -> Vec<u8> {
        let mut img = RgbImage::new(w, h);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgb([(x % 256) as u8, (y % 256) as u8, ((x * y) % 256) as u8]);
        }

        encode_png(DynamicImage::ImageRgb8(img))
    }

    fn encode_png(img: DynamicImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("the png should encode");

        bytes
    }

    #[track_caller]
    fn check_cover(bytes: &[u8], quality: f32) -> ProcessedArtwork {
        build_cover(bytes, quality).expect("the image should become a cover")
    }

    #[track_caller]
    fn check_crop(size: (u32, u32), aspect: (u32, u32), expected: (u32, u32)) {
        let cropped = crop_to_aspect_ratio(image(size.0, size.1, RED), aspect);

        assert_eq!(
            (cropped.width(), cropped.height()),
            expected,
            "cropping {size:?} to {aspect:?}"
        );
    }

    /// Marks one pixel and checks that cropping the image to a square keeps it.
    #[track_caller]
    fn check_centered_crop(size: (u32, u32), keeps: (u32, u32)) {
        let mut img = RgbImage::from_pixel(size.0, size.1, Rgb([0, 0, 0]));
        img.put_pixel(keeps.0, keeps.1, RED);

        let cropped = crop_to_aspect_ratio(DynamicImage::ImageRgb8(img), (1, 1));

        assert_eq!(
            cropped.to_rgb8().get_pixel(0, 0),
            &RED,
            "cropping {size:?} should have kept the pixel at {keeps:?}"
        );
    }

    #[track_caller]
    fn check_resize(size: (u32, u32), max_h: u32, expected: (u32, u32)) {
        let resized = resize_to_max_height(image(size.0, size.1, RED), max_h);

        assert_eq!(
            (resized.width(), resized.height()),
            expected,
            "resizing {size:?} to fit {max_h}"
        );
    }

    #[test]
    fn image_bytes_become_covers() {
        let cover = check_cover(&png(1800, 1200, RED), 75.);

        let decoded = image::load_from_memory(&cover.bytes).expect("the cover should be a webp");
        assert_eq!((decoded.width(), decoded.height()), (400, MAX_HEIGHT));

        let color = cover.color.expect("a red cover should have an accent");
        assert!(
            color.r > color.g && color.r > color.b,
            "expected a red accent, got {color:?}"
        );
    }

    #[test]
    fn the_same_image_always_hashes_the_same() {
        let red = check_cover(&png(400, 600, RED), 75.);

        assert_eq!(red.hash, check_cover(&png(400, 600, RED), 75.).hash);
        assert_ne!(red.hash, check_cover(&png(400, 600, BLUE), 75.).hash);
    }

    #[test]
    fn covers_are_hashed_by_the_bytes_they_carry() {
        let sharp = check_cover(&gradient_png(400, 600), 90.);
        let rough = check_cover(&gradient_png(400, 600), 10.);
        assert_ne!(sharp.bytes, rough.bytes, "quality should change the bytes");
        assert_ne!(sharp.hash, rough.hash);
    }

    #[test]
    fn non_images_are_turned_away() {
        let Err(error) = build_cover(b"<html>404</html>", 75.) else {
            panic!("a page of html should not become a cover");
        };

        assert!(matches!(error, ProcessingError::Decode(_)), "{error:?}");
    }

    #[test]
    fn images_too_small_for_a_cover_are_turned_away() {
        for (w, h) in [(400, 1), (1, 1), (1, 600)] {
            let Err(error) = build_cover(&png(w, h, RED), 75.) else {
                panic!("a {w}x{h} image should not become a cover");
            };

            assert!(
                matches!(error, ProcessingError::TooSmall { width, height } if (width, height) == (w, h)),
                "{error:?}"
            );
        }
    }

    #[test]
    fn the_smallest_allowed_image_still_becomes_a_cover() {
        let cover = check_cover(&png(MIN_SIZE.0, MIN_SIZE.1, RED), 75.);
        let decoded = image::load_from_memory(&cover.bytes).expect("the cover should be a webp");

        assert_eq!((decoded.width(), decoded.height()), MIN_SIZE);
    }

    #[test]
    fn crops_reach_the_target_shape() {
        check_crop((900, 600), COVER_ASPECT, (400, 600));
        check_crop((400, 900), COVER_ASPECT, (400, 600));
        check_crop((400, 600), COVER_ASPECT, (400, 600));
    }

    #[test]
    fn crops_are_centered() {
        check_centered_crop((3, 1), (1, 0));
        check_centered_crop((1, 3), (0, 1));
        // Odd margin so there is no middle pixel.
        check_centered_crop((4, 1), (1, 0));
    }

    #[test]
    fn resizing_only_shrinks() {
        check_resize((400, 1200), MAX_HEIGHT, (200, MAX_HEIGHT));
        check_resize((200, 300), MAX_HEIGHT, (200, 300));
        check_resize((400, MAX_HEIGHT), MAX_HEIGHT, (400, MAX_HEIGHT));
    }

    #[test]
    fn grayscale_images_survive_encoding() {
        let mut img = GrayImage::new(60, 90);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Luma([((x + y) % 256) as u8]);
        }

        let webp = encode_webp(&DynamicImage::ImageLuma8(img), 75.);
        let decoded = image::load_from_memory(&webp).expect("the result should be a webp");

        assert_eq!((decoded.width(), decoded.height()), (60, 90));
    }

    #[test]
    fn transparency_survives_encoding() {
        let img = RgbaImage::from_pixel(60, 90, Rgba([200, 30, 30, 40]));

        let webp = encode_webp(&DynamicImage::ImageRgba8(img), 75.);
        let decoded = image::load_from_memory(&webp).expect("the result should be a webp");

        assert_eq!((decoded.width(), decoded.height()), (60, 90));
        // The encoder is lossy, so the alpha value may change a little.
        let alpha = decoded.to_rgba8().get_pixel(30, 45).0[3];
        assert!(
            alpha.abs_diff(40) < 16,
            "expected a translucent cover, got alpha {alpha}"
        );
    }
}
