#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
    Macos,
    Android,
    Other,
}

impl Platform {
    #[cfg(target_os = "windows")]
    pub const CURRENT: Self = Self::Windows;

    #[cfg(target_os = "linux")]
    pub const CURRENT: Self = Self::Linux;

    #[cfg(target_os = "macos")]
    pub const CURRENT: Self = Self::Macos;

    pub const fn is_supported(self) -> bool {
        matches!(self, Self::CURRENT | Self::Other)
    }
}
