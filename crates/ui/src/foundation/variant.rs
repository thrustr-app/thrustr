#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Secondary,
    Accent,
    Warning,
    Destructive,
    Outline,
}

pub trait WithVariant: Sized {
    fn variant(self, variant: Variant) -> Self;

    fn variant_primary(self) -> Self {
        self.variant(Variant::Primary)
    }

    fn variant_secondary(self) -> Self {
        self.variant(Variant::Secondary)
    }

    fn variant_accent(self) -> Self {
        self.variant(Variant::Accent)
    }

    fn variant_warning(self) -> Self {
        self.variant(Variant::Warning)
    }

    fn variant_destructive(self) -> Self {
        self.variant(Variant::Destructive)
    }

    fn variant_outline(self) -> Self {
        self.variant(Variant::Outline)
    }
}
