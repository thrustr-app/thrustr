use domain::{
    artwork::Color,
    component::{Image, ImageFormat},
    platform::Platform,
};
use gpui::{Hsla, Image as GpuiImage, ImageFormat as GpuiImageFormat, Rgba, rgba};
use std::sync::Arc;
use ui::Icon;

pub trait ImageExt {
    fn to_gpui(&self) -> Arc<GpuiImage>;
}

impl ImageExt for Image {
    fn to_gpui(&self) -> Arc<GpuiImage> {
        Arc::new(GpuiImage::from_bytes(
            image_format_to_gpui(self.format),
            self.bytes.clone(),
        ))
    }
}

fn image_format_to_gpui(format: ImageFormat) -> GpuiImageFormat {
    match format {
        ImageFormat::Png => GpuiImageFormat::Png,
        ImageFormat::Jpeg => GpuiImageFormat::Jpeg,
        ImageFormat::Webp => GpuiImageFormat::Webp,
        ImageFormat::Gif => GpuiImageFormat::Gif,
        ImageFormat::Svg => GpuiImageFormat::Svg,
        ImageFormat::Bmp => GpuiImageFormat::Bmp,
        ImageFormat::Tiff => GpuiImageFormat::Tiff,
        ImageFormat::Ico => GpuiImageFormat::Ico,
        ImageFormat::Pnm => GpuiImageFormat::Pnm,
    }
}

pub trait ColorExt {
    fn to_gpui_rgba(&self) -> Rgba;
    fn to_gpui_hsla(&self) -> Hsla;
}

impl ColorExt for Color {
    fn to_gpui_rgba(&self) -> Rgba {
        rgba(self.to_rgba_hex())
    }

    fn to_gpui_hsla(&self) -> Hsla {
        self.to_gpui_rgba().into()
    }
}

pub trait PlatformExt {
    fn icon(self) -> Option<Icon>;
}

impl PlatformExt for Platform {
    fn icon(self) -> Option<Icon> {
        match self {
            Platform::Windows => Some(Icon::windows()),
            Platform::Linux => Some(Icon::linux()),
            Platform::Macos => Some(Icon::apple()),
            Platform::Android => Some(Icon::android()),
            Platform::Other => None,
        }
    }
}
