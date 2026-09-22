macro_rules! define_theme_group {
    ($name:ident : $ty:ty { $($field:ident),* $(,)? }) => {
        paste::paste! {
            #[derive(Debug, Clone, Deserialize)]
            pub struct $name {
                $(pub $field: $ty),*
            }

            #[derive(Debug, Deserialize)]
            pub struct [<Partial $name>] {
                $(pub $field: Option<$ty>),*
            }

            impl [<Partial $name>] {
                pub fn merge(self, other: &$name) -> $name {
                    $name {
                        $($field: self.$field.unwrap_or(other.$field)),*
                    }
                }
            }
        }
    };
}

macro_rules! define_theme_colors {
    (
        colors: [$($field:ident),* $(,)?],
        groups: [$($group_field:ident: $group_ty:ident),* $(,)?]
    ) => {
        paste::paste! {
            #[derive(Debug, Clone, Deserialize)]
            pub struct ThemeColors {
                $(pub $field: Hsla,)*
                $(pub $group_field: $group_ty,)*
            }

            #[derive(Debug, Deserialize)]
            pub struct PartialThemeColors {
                $(pub $field: Option<Hsla>,)*
                $(pub $group_field: Option<[<Partial $group_ty>]>,)*
            }

            impl PartialThemeColors {
                pub fn merge(self, other: &ThemeColors) -> ThemeColors {
                    ThemeColors {
                        $($field: self.$field.unwrap_or(other.$field),)*
                        $($group_field: self.$group_field
                            .map(|g| g.merge(&other.$group_field))
                            .unwrap_or_else(|| other.$group_field.clone()),)*
                    }
                }
            }
        }
    };
}
