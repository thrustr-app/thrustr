use crate::{ArtworkTask, color::extract_accent};
use anyhow::{Context, Result};
use bytes::Bytes;
use domain::artwork::Color;
use image::{DynamicImage, imageops::FilterType};
use reqwest::Client;
use std::{path::Path, time::Duration};
use tokio::{fs, task::spawn_blocking};
use webp::Encoder;

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_HEIGHT: u32 = 600;

pub struct ProcessedArtwork {
    pub bytes: Vec<u8>,
    pub hash: String,
    pub color: Option<Color>,
}

pub async fn process_task(task: ArtworkTask, client: Client) -> Result<ProcessedArtwork> {
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

pub(crate) async fn write_file(path: impl AsRef<Path>, data: &[u8]) -> Result<()> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    fs::write(path, data).await?;

    Ok(())
}
