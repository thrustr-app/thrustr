macro_rules! define_theme_color_group {
    ($name:ident, $partial_name:ident { $($field:ident),* $(,)? }) => {
        #[derive(Debug, Clone, Deserialize)]
        pub struct $name {
            $(pub $field: Hsla),*
        }

        #[derive(Debug, Deserialize)]
        pub struct $partial_name {
            $(pub $field: Option<Hsla>),*
        }

        impl $partial_name {
            pub fn merge(self, other: &$name) -> $name {
                $name {
                    $($field: self.$field.unwrap_or(other.$field)),*
                }
            }
        }
    };
}

macro_rules! define_theme_colors {
    (
        colors: [$($field:ident),* $(,)?],
        groups: [$($group_field:ident: $group_ty:ident / $partial_group_ty:ident),* $(,)?]
    ) => {
        #[derive(Debug, Clone, Deserialize)]
        pub struct ThemeColors {
            $(pub $field: Hsla,)*
            $(pub $group_field: $group_ty,)*
        }

        #[derive(Debug, Deserialize)]
        pub struct PartialThemeColors {
            $(pub $field: Option<Hsla>,)*
            $(pub $group_field: Option<$partial_group_ty>,)*
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
    };
}

macro_rules! define_theme_radius{
    ($($field:ident),* $(,)?) => {
        #[derive(Debug, Clone, Deserialize)]
        pub struct ThemeRadius {
            $(pub $field: AbsoluteLength),*
        }

        #[derive(Debug, Deserialize)]
        pub struct PartialThemeRadius {
            $(pub $field: Option<AbsoluteLength>),*
        }

        impl PartialThemeRadius{
            pub fn merge(self, other: &ThemeRadius) -> ThemeRadius {
                ThemeRadius {
                    $($field: self.$field.unwrap_or(other.$field)),*
                }
            }
        }
    };
}

macro_rules! define_theme_text{
    ($($field:ident),* $(,)?) => {
        #[derive(Debug, Clone, Deserialize)]
        pub struct ThemeText {
            $(pub $field: AbsoluteLength),*
        }

        #[derive(Debug, Deserialize)]
        pub struct PartialThemeText {
            $(pub $field: Option<AbsoluteLength>),*
        }

        impl PartialThemeText{
            pub fn merge(self, other: &ThemeText) -> ThemeText {
                ThemeText {
                    $($field: self.$field.unwrap_or(other.$field)),*
                }
            }
        }
    };
}
