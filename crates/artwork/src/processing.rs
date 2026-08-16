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
    encode(bytes, task.quality).await
}

async fn encode(bytes: Bytes, quality: f32) -> Result<ProcessedArtwork, ProcessingError> {
    spawn_blocking(move || {
        let img = decode_and_process(&bytes)?;
        let color = extract_accent(&img);
        let webp = encode_webp(&img, quality);
        let hash = blake3::hash(&webp).to_hex().to_string();
        Ok(ProcessedArtwork {
            bytes: webp,
            hash,
            color,
        })
    })
    .await?
}

async fn download_image(url: &str, client: Client) -> Result<Bytes, ProcessingError> {
    let response = client.get(url).timeout(DOWNLOAD_TIMEOUT).send().await?;

    let status = response.status();
    if !status.is_success() {
        return Err(ProcessingError::Status {
            status,
            retry_after: retry_after(response.headers()),
        });
    }

    Ok(response.bytes().await?)
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();

    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    let deadline = DateTime::parse_from_rfc2822(value).ok()?;
    (deadline.to_utc() - Utc::now()).to_std().ok()
}

fn decode_and_process(bytes: &[u8]) -> Result<DynamicImage, ImageError> {
    let img = image::load_from_memory(bytes)?;
    let img = crop_to_aspect_ratio(img, COVER_ASPECT);
    let img = resize_to_max_height(img, MAX_HEIGHT);
    Ok(img)
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
    use image::{GrayImage, Luma, Rgb, Rgba};
    use reqwest::header::HeaderValue;

    fn headers(retry_after: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static(retry_after));
        headers
    }

    #[test]
    fn retry_after_reads_a_delay_in_seconds() {
        assert_eq!(retry_after(&headers("120")), Some(Duration::from_secs(120)));
    }

    #[test]
    fn retry_after_reads_a_date() {
        let delay = retry_after(&headers("Wed, 21 Oct 2099 03:48:00 GMT"))
            .expect("an http date should parse");

        // The exact delay depends on when the test runs, so just check that it is reasonable.
        assert!(delay > Duration::from_secs(60 * 60 * 24));
    }

    #[test]
    fn retry_after_ignores_a_date_that_has_passed() {
        assert_eq!(retry_after(&headers("Wed, 30 Jun 2015 16:13:00 GMT")), None);
    }

    #[test]
    fn retry_after_ignores_what_it_cannot_read() {
        assert_eq!(retry_after(&HeaderMap::new()), None);
        assert_eq!(retry_after(&headers("soon")), None);
        assert_eq!(retry_after(&headers("-5")), None);
    }

    fn image(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(w, h, Rgb([200, 30, 30])))
    }

    #[test]
    fn wide_images_lose_their_sides() {
        let cropped = crop_to_aspect_ratio(image(900, 600), COVER_ASPECT);
        assert_eq!((cropped.width(), cropped.height()), (400, 600));
    }

    #[test]
    fn tall_images_lose_their_top_and_bottom() {
        let cropped = crop_to_aspect_ratio(image(400, 900), COVER_ASPECT);
        assert_eq!((cropped.width(), cropped.height()), (400, 600));
    }

    #[test]
    fn images_already_in_shape_are_left_alone() {
        let cropped = crop_to_aspect_ratio(image(400, 600), COVER_ASPECT);
        assert_eq!((cropped.width(), cropped.height()), (400, 600));
    }

    #[test]
    fn crops_are_centered() {
        let mut img = RgbImage::from_pixel(3, 1, Rgb([0, 0, 0]));
        img.put_pixel(1, 0, Rgb([200, 30, 30]));

        let cropped = crop_to_aspect_ratio(DynamicImage::ImageRgb8(img), (1, 1));
        assert_eq!(cropped.to_rgb8().get_pixel(0, 0), &Rgb([200, 30, 30]));
    }

    #[test]
    fn oversized_images_shrink_and_keep_their_shape() {
        let resized = resize_to_max_height(image(400, 1200), MAX_HEIGHT);
        assert_eq!((resized.width(), resized.height()), (200, MAX_HEIGHT));
    }

    #[test]
    fn small_images_are_not_upscaled() {
        let resized = resize_to_max_height(image(200, 300), MAX_HEIGHT);
        assert_eq!((resized.width(), resized.height()), (200, 300));
    }

    #[test]
    fn grayscale_images_survive_encoding() {
        let mut img = GrayImage::new(60, 90);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Luma([((x + y) % 256) as u8]);
        }

        let webp = encode_webp(&DynamicImage::ImageLuma8(img), 75.);
        assert!(!webp.is_empty());
    }

    #[test]
    fn transparency_survives_encoding() {
        let img = RgbaImage::from_pixel(60, 90, Rgba([200, 30, 30, 40]));
        let webp = encode_webp(&DynamicImage::ImageRgba8(img), 75.);
        assert!(!webp.is_empty());
    }
}
