use gpui::{Hsla, Rems, rems};

mod focus;

pub use focus::*;

pub(crate) fn composite(background: Hsla, layer: Hsla) -> Hsla {
    if background.is_transparent() {
        layer
    } else {
        background.blend(layer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    Secondary,
    Accent,
    Warning,
    Danger,
    #[default]
    Outline,
    Ghost,
    Field,
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

    fn variant_field(self) -> Self {
        self.variant(Variant::Field)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
    ExtraLarge,
}

impl Size {
    pub fn control_height(self) -> Rems {
        match self {
            Self::Small => rems(2.),
            Self::Medium => rems(2.375),
            Self::Large => rems(2.625),
            Self::ExtraLarge => rems(2.875),
        }
    }

    pub(crate) fn field_px(self) -> Rems {
        match self {
            Self::Small => rems(0.625),
            Self::Medium => rems(0.75),
            Self::Large => rems(1.),
            Self::ExtraLarge => rems(1.25),
        }
    }
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

    fn size_xl(self) -> Self {
        self.size(Size::ExtraLarge)
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
