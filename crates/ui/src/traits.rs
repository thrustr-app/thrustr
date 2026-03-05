#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Small,
    Medium,
    Large,
}

pub trait WithSize: Sized {
    fn size(self, size: Size) -> Self;

    fn size_small(self) -> Self {
        self.size(Size::Small)
    }

    fn size_medium(self) -> Self {
        self.size(Size::Medium)
    }

    fn size_large(self) -> Self {
        self.size(Size::Large)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Ghost,
}

pub trait WithVariant: Sized {
    fn variant(self, variant: Variant) -> Self;

    fn ghost(self) -> Self {
        self.variant(Variant::Ghost)
    }
}
