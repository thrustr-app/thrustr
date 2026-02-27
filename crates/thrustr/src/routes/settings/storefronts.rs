use crate::{conversions::image::image_to_gpui, globals::StorefrontManagerExt};
use gpui::{
    Context, Image as GpuiImage, ImageSource, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, div, img, rems,
};
use ports::managers::StorefrontManager;
use std::{collections::HashMap, sync::Arc};
use theme_manager::ThemeExt;
use ui::Card;

pub struct Storefronts {
    icons: HashMap<SharedString, Arc<GpuiImage>>,
}

impl Storefronts {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let icons = cx
            .storefront_manager()
            .storefront_providers()
            .into_iter()
            .filter_map(|provider| {
                let name: SharedString = provider.name().to_string().into();
                let icon = provider.icon()?;
                Some((name, image_to_gpui(icon)))
            })
            .collect();

        Self { icons }
    }
}

impl Render for Storefronts {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme();

        let cards = self.icons.iter().map(|(name, icon)| {
            Card::new()
                .gap(rems(1.))
                .items_center()
                .size(rems(10.))
                .child(img(ImageSource::Image(icon.clone())).size_full())
                .child(name.clone())
        });

        div().flex_grow().px(rems(1.5)).children(cards)
    }
}
