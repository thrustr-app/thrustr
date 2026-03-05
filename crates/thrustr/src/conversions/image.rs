use gpui::{Image as GpuiImage, ImageFormat as GpuiImageFormat};
use ports::component::{Image, ImageFormat};
use std::sync::Arc;

pub fn image_format_to_gpui(format: ImageFormat) -> GpuiImageFormat {
    match format {
        ImageFormat::Png => GpuiImageFormat::Png,
        ImageFormat::Jpeg => GpuiImageFormat::Jpeg,
        ImageFormat::Webp => GpuiImageFormat::Webp,
        ImageFormat::Gif => GpuiImageFormat::Gif,
        ImageFormat::Svg => GpuiImageFormat::Svg,
        ImageFormat::Bmp => GpuiImageFormat::Bmp,
        ImageFormat::Tiff => GpuiImageFormat::Tiff,
    }
}

pub fn image_to_gpui(image: &Image) -> Arc<GpuiImage> {
    Arc::new(GpuiImage::from_bytes(
        image_format_to_gpui(image.format),
        image.bytes.clone(),
    ))
}
