use crate::ImageTask;
use anyhow::{Context, Result};
use bytes::Bytes;
use image::{DynamicImage, imageops::FilterType};
use reqwest::Client;
use std::{path::Path, time::Duration};
use tokio::{fs, task::spawn_blocking};
use webp::Encoder;

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_HEIGHT: u32 = 600;

pub async fn process_task(task: ImageTask, client: Client) -> Result<()> {
    let bytes = download_image(&task.url, client).await?;
    let webp = to_webp(bytes, task.quality).await?;
    write_file(&task.path, &webp).await?;
    Ok(())
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

async fn to_webp(bytes: Bytes, quality: f32) -> Result<Vec<u8>> {
    spawn_blocking(move || {
        let img = decode_and_process(&bytes)?;
        encode_webp(&img, quality)
    })
    .await?
}

fn decode_and_process(bytes: &[u8]) -> Result<DynamicImage> {
    let img = image::load_from_memory(bytes).context("Failed to decode image")?;
    let img = crop_to_aspect_ratio(img, 2, 3);
    let img = resize_to_max_height(img, MAX_HEIGHT);
    Ok(img)
}

fn encode_webp(img: &DynamicImage, quality: f32) -> Result<Vec<u8>> {
    Ok(Encoder::from_image(img)
        .map_err(|e| anyhow::anyhow!("Failed to create WebP encoder: {e}"))?
        .encode(quality)
        .to_vec())
}

fn crop_to_aspect_ratio(img: DynamicImage, target_w: u32, target_h: u32) -> DynamicImage {
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

async fn write_file(path: impl AsRef<Path>, data: &[u8]) -> Result<()> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    fs::write(path, data).await?;

    Ok(())
}
