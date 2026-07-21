#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Medium,
    Large,
}

pub trait WithSize: Sized {
    fn size(self, size: Size) -> Self;

    fn size_md(self) -> Self {
        self.size(Size::Medium)
    }

    fn size_lg(self) -> Self {
        self.size(Size::Large)
    }
}
