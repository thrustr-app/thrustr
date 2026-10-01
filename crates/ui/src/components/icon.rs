use crate::{Size, WithSize};
use gpui::{
    Animation, AnimationExt, Hsla, IntoElement, Rems, RenderOnce, SharedString, Styled,
    Transformation, Window, percentage, prelude::FluentBuilder, rems, svg,
};
use std::time::Duration;

macro_rules! icon_constructors {
    ($($fn_name:ident => $path:literal),+ $(,)?) => {
        impl Icon {
            $(
                pub fn $fn_name() -> Self {
                    Self::new($path)
                }
            )+
        }
    };
}

icon_constructors! {
    appearance => "icons/appearance.svg",
    apple => "icons/apple.svg",
    arrow => "icons/arrow.svg",
    check => "icons/check.svg",
    chevrons => "icons/chevrons.svg",
    collections => "icons/collections.svg",
    danger => "icons/danger.svg",
    download => "icons/download.svg",
    home => "icons/home.svg",
    library => "icons/library.svg",
    linux => "icons/linux.svg",
    loader => "icons/loader.svg",
    logo => "icons/logo.svg",
    maximize => "icons/maximize.svg",
    menu => "icons/menu.svg",
    minimize => "icons/minimize.svg",
    plugin => "icons/plugin.svg",
    restore => "icons/restore.svg",
    search => "icons/search.svg",
    settings => "icons/settings.svg",
    storefront => "icons/storefront.svg",
    windows => "icons/windows.svg",
    x => "icons/x.svg",
}

#[derive(IntoElement)]
pub struct Icon {
    path: SharedString,
    size: Size,
    color: Option<Hsla>,
    transformation: Option<Transformation>,
    spin: bool,
}

impl Icon {
    pub fn new(path: impl Into<SharedString>) -> Self {
        Self {
            path: path.into(),
            size: Size::default(),
            color: None,
            transformation: None,
            spin: false,
        }
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn transform(mut self, transformation: Transformation) -> Self {
        self.transformation = Some(transformation);
        self
    }

    pub fn spin(mut self) -> Self {
        self.spin = true;
        self
    }

    pub(crate) fn length(size: Size) -> Rems {
        match size {
            Size::Small => rems(1.),
            Size::Medium => rems(1.125),
            Size::Large | Size::ExtraLarge => rems(1.25),
        }
    }
}

impl WithSize for Icon {
    fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, _window: &mut Window, _cx: &mut gpui::App) -> impl IntoElement {
        let svg = svg()
            .flex_shrink_0()
            .path(self.path)
            .size(Self::length(self.size))
            .when_some(self.color, |svg, color| svg.text_color(color));

        if self.spin {
            svg.with_animation(
                "spin",
                Animation::new(Duration::from_millis(850))
                    .repeat()
                    .with_max_fps(30.),
                |svg, delta| svg.with_transformation(Transformation::rotate(percentage(delta))),
            )
            .into_any_element()
        } else {
            svg.when_some(self.transformation, |svg, transformation| {
                svg.with_transformation(transformation)
            })
            .into_any_element()
        }
    }
}
