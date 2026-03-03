#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Svg,
    Bmp,
    Tiff,
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
            _ => None,
        }
    }
}

pub struct Image {
    pub bytes: Vec<u8>,
    pub format: ImageFormat,
}

#[derive(Debug)]
pub enum Origin {
    Core,
    Plugin(String),
}

impl Origin {
    pub fn is_core(&self) -> bool {
        matches!(self, Self::Core)
    }

    pub fn is_plugin(&self) -> bool {
        matches!(self, Self::Plugin(_))
    }

    pub fn plugin_id(&self) -> Option<&str> {
        match self {
            Self::Plugin(id) => Some(id),
            _ => None,
        }
    }
}

pub trait Manifest {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn origin(&self) -> &Origin;
    fn description(&self) -> Option<&str>;
    fn icon(&self) -> Option<&Image>;
}
