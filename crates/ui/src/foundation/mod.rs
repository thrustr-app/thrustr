mod focus;

pub use focus::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    Secondary,
    Accent,
    Warning,
    Danger,
    #[default]
    Outline,
    Ghost,
}

pub trait WithVariant: Sized {
    fn variant(self, variant: Variant) -> Self;

    fn variant_secondary(self) -> Self {
        self.variant(Variant::Secondary)
    }

    fn variant_accent(self) -> Self {
        self.variant(Variant::Accent)
    }

    fn variant_warning(self) -> Self {
        self.variant(Variant::Warning)
    }

    fn variant_danger(self) -> Self {
        self.variant(Variant::Danger)
    }

    fn variant_outline(self) -> Self {
        self.variant(Variant::Outline)
    }

    fn variant_ghost(self) -> Self {
        self.variant(Variant::Ghost)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

pub trait WithSize: Sized {
    fn size(self, size: Size) -> Self;

    fn size_sm(self) -> Self {
        self.size(Size::Small)
    }

    fn size_md(self) -> Self {
        self.size(Size::Medium)
    }

    fn size_lg(self) -> Self {
        self.size(Size::Large)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Radius {
    #[default]
    Medium,
    Pill,
}

pub trait WithRadius: Sized {
    fn radius(self, radius: Radius) -> Self;

    fn radius_md(self) -> Self {
        self.radius(Radius::Medium)
    }

    fn radius_pill(self) -> Self {
        self.radius(Radius::Pill)
    }
}
