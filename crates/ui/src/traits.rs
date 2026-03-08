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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Accent,
    Ghost,
}

pub trait WithVariant: Sized {
    fn variant(self, variant: Variant) -> Self;

    fn variant_primary(self) -> Self {
        self.variant(Variant::Primary)
    }

    fn variant_accent(self) -> Self {
        self.variant(Variant::Accent)
    }

    fn variant_ghost(self) -> Self {
        self.variant(Variant::Ghost)
    }
}
