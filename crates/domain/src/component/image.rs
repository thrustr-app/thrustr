#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Svg,
    Bmp,
    Tiff,
    Ico,
    Pnm,
}

impl ImageFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            "gif" => Some(Self::Gif),
            "svg" => Some(Self::Svg),
            "bmp" => Some(Self::Bmp),
            "tiff" => Some(Self::Tiff),
            "ico" => Some(Self::Ico),
            "pbm" | "ppm" | "pgm" => Some(Self::Pnm),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Image {
    pub bytes: Vec<u8>,
    pub format: ImageFormat,
}
