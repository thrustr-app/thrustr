use crate::{ArtworkTask, color::extract_accent};
use anyhow::{Context, Result};
use bytes::Bytes;
use domain::artwork::Color;
use image::{DynamicImage, imageops::FilterType};
use reqwest::Client;
use std::time::Duration;
use tokio::task::spawn_blocking;
use webp::Encoder;

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);

const MAX_HEIGHT: u32 = 600;
const COVER_ASPECT: (u32, u32) = (2, 3);

pub struct ProcessedArtwork {
    pub bytes: Vec<u8>,
    pub hash: String,
    pub color: Option<Color>,
}

pub async fn process_task(task: &ArtworkTask, client: Client) -> Result<ProcessedArtwork> {
    let bytes = download_image(&task.url, client).await?;
    encode(bytes, task.quality).await
}

async fn encode(bytes: Bytes, quality: f32) -> Result<ProcessedArtwork> {
    spawn_blocking(move || {
        let img = decode_and_process(&bytes)?;
        let color = extract_accent(&img);
        let webp = encode_webp(&img, quality)?;
        let hash = blake3::hash(&webp).to_hex().to_string();
        Ok(ProcessedArtwork {
            bytes: webp,
            hash,
            color,
        })
    })
    .await?
}

async fn download_image(url: &str, client: Client) -> Result<Bytes> {
    let response = client
        .get(url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await?
        .error_for_status()
        .with_context(|| format!("Failed to download image from {url}"))?;

    Ok(response.bytes().await?)
}

fn decode_and_process(bytes: &[u8]) -> Result<DynamicImage> {
    let img = image::load_from_memory(bytes).context("Failed to decode image")?;
    let img = crop_to_aspect_ratio(img, COVER_ASPECT);
    let img = resize_to_max_height(img, MAX_HEIGHT);
    Ok(img)
}

fn encode_webp(img: &DynamicImage, quality: f32) -> Result<Vec<u8>> {
    let img = match img {
        DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_) => img,
        other if other.color().has_alpha() => &DynamicImage::ImageRgba8(other.to_rgba8()),
        other => &DynamicImage::ImageRgb8(other.to_rgb8()),
    };

    Ok(Encoder::from_image(img)
        .map_err(|e| anyhow::anyhow!("Failed to create WebP encoder: {e}"))?
        .encode(quality)
        .to_vec())
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
    use image::{GrayImage, Luma, Rgb, RgbImage, Rgba, RgbaImage};

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

        let webp = encode_webp(&DynamicImage::ImageLuma8(img), 75.).expect("grayscale encodes");
        assert!(!webp.is_empty());
    }

    #[test]
    fn transparency_survives_encoding() {
        let img = RgbaImage::from_pixel(60, 90, Rgba([200, 30, 30, 40]));
        let webp = encode_webp(&DynamicImage::ImageRgba8(img), 75.).expect("rgba encodes");
        assert!(!webp.is_empty());
    }
}
