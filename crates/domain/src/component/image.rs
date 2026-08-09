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
            "tiff" | "tif" => Some(Self::Tiff),
            "ico" => Some(Self::Ico),
            "pnm" | "pbm" | "ppm" | "pgm" => Some(Self::Pnm),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Image {
    pub bytes: Vec<u8>,
    pub format: ImageFormat,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_match_regardless_of_case() {
        assert_eq!(ImageFormat::from_extension("PNG"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_extension("Jpeg"), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn aliased_extensions_share_a_format() {
        assert_eq!(ImageFormat::from_extension("jpg"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("jpeg"), Some(ImageFormat::Jpeg));

        for ext in ["pbm", "ppm", "pgm"] {
            assert_eq!(ImageFormat::from_extension(ext), Some(ImageFormat::Pnm));
        }
    }

    #[test]
    fn unknown_extensions_have_no_format() {
        assert_eq!(ImageFormat::from_extension("txt"), None);
        assert_eq!(ImageFormat::from_extension(""), None);
    }

    #[test]
    fn extensions_are_matched_verbatim_apart_from_case() {
        assert_eq!(ImageFormat::from_extension("png "), None);
        assert_eq!(ImageFormat::from_extension(" png"), None);
        assert_eq!(ImageFormat::from_extension(".png"), None);
    }
}
