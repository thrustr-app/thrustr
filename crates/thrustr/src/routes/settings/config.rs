use crate::{
    conversions::image::image_to_gpui, globals::ComponentManagerExt, navigation::NavigationExt,
};
use gpui::{
    Context, FontWeight, Image, ImageSource, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, div, img, prelude::FluentBuilder, rems, svg,
};
use ports::component::Component;
use std::sync::Arc;
use theme_manager::ThemeExt;
use ui::Button;

pub struct Config {
    name: SharedString,
    icon: Option<Arc<Image>>,
    component: Arc<dyn Component>,
}

impl Config {
    pub fn new(cx: &mut Context<Self>, component_id: &str) -> Self {
        let component = cx.component_manager().component(component_id).unwrap();
        let metadata = component.metadata();
        let icon = metadata.icon.clone().map(image_to_gpui);

        let page = Self {
            name: component.metadata().name.clone().into(),
            icon,
            component,
        };
        page
    }
}

impl Render for Config {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div().flex_grow().px(rems(1.5)).flex().flex_col().child(
            div()
                .flex()
                .gap(rems(1.))
                .items_center()
                .text_color(theme.colors.foreground_primary)
                .child(
                    Button::new("back-button")
                        .circular()
                        .child(
                            svg()
                                .path("icons/arrow-left.svg")
                                .size_full()
                                .text_color(theme.colors.foreground_primary),
                        )
                        .on_click(|_, _, cx| {
                            cx.navigate_back();
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rems(0.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(rems(1.5))
                        .when_some(self.icon.clone(), |div, icon| {
                            div.child(img(ImageSource::Image(icon)).size(rems(2.)))
                        })
                        .child(self.name.clone()),
                ),
        )
    }
}
