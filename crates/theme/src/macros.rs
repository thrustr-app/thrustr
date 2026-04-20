macro_rules! define_theme_colors {
    ($($field:ident),* $(,)?) => {
        #[derive(Debug, Clone, Deserialize)]
        pub struct ThemeColors {
            $(pub $field: Hsla),*
        }

        #[derive(Debug, Deserialize)]
        pub struct PartialThemeColors {
            $(pub $field: Option<Hsla>),*
        }

        impl PartialThemeColors {
            pub fn merge(self, other: &ThemeColors) -> ThemeColors {
                ThemeColors {
                    $($field: self.$field.unwrap_or(other.$field)),*
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
