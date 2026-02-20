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
