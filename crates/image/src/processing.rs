use crate::ImageTask;
use anyhow::{Context, Result};
use image::DynamicImage;
use reqwest::Client;
use std::{path::Path, time::Duration};
use tokio::{fs, task::spawn_blocking};
use webp::Encoder;

pub async fn process_task(task: ImageTask, client: Client) -> Result<()> {
    let bytes = download_image(&task.url, client).await?;
    let webp = to_webp(bytes, task.quality).await?;
    write_file(&task.path, &webp).await?;
    Ok(())
}

async fn download_image(url: &str, client: Client) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Failed to download image from {}: HTTP {}",
            url,
            response.status()
        );
    }

    let bytes = response.bytes().await?;
    Ok(bytes.to_vec())
}

async fn to_webp(bytes: Vec<u8>, quality: f32) -> Result<Vec<u8>> {
    spawn_blocking(move || {
        let img = image::load_from_memory(&bytes).context("Failed to decode image")?;
        let img = crop_to_aspect_ratio(img, 2, 3);
        let encoder = Encoder::from_image(&img)
            .map_err(|_| anyhow::anyhow!("Failed to create WebP encoder"))?;
        let webp = encoder.encode(quality);
        Ok(webp.to_vec())
    })
    .await?
}

fn crop_to_aspect_ratio(img: DynamicImage, target_w: u32, target_h: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());

    let (crop_w, crop_h) = if w * target_h > h * target_w {
        (h * target_w / target_h, h)
    } else {
        (w, w * target_h / target_w)
    };

    let x = (w - crop_w) / 2;
    let y = (h - crop_h) / 2;

    img.crop_imm(x, y, crop_w, crop_h)
}

async fn write_file(path: impl AsRef<Path>, data: &[u8]) -> Result<()> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    fs::write(path, data).await?;
    Ok(())
}
