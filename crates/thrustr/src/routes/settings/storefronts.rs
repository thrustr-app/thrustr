use crate::{conversions::image::image_to_gpui, globals::StorefrontManagerExt};
use gpui::{
    Context, FontWeight, Image as GpuiImage, ImageSource, IntoElement, ParentElement, Render,
    SharedString, Styled, Window, div, img, prelude::FluentBuilder, rems,
};
use ports::{managers::StorefrontManager, providers::StorefrontProviderStatus};
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
        let theme = cx.theme();

        let cards = cx
            .storefront_manager()
            .storefront_providers()
            .into_iter()
            .map(|provider| {
                let name: SharedString = provider.name().to_string().into();
                let icon = self.icons.get(&name).cloned();

                let mut status = div().font_weight(FontWeight::BOLD).text_size(rems(0.6));

                match provider.status() {
                    StorefrontProviderStatus::Active => {
                        status = status.text_color(theme.colors.accent).child("ACTIVE");
                    }
                    StorefrontProviderStatus::Inactive => {
                        status = status
                            .text_color(theme.colors.card_foreground_secondary)
                            .child("INACTIVE");
                    }
                    StorefrontProviderStatus::Error(_) => {
                        status = status.text_color(theme.colors.error).child("ERROR");
                    }
                }
                Card::new()
                    .gap(rems(1.))
                    .items_center()
                    .size(rems(10.))
                    .when_some(icon, |card, icon| {
                        card.child(img(ImageSource::Image(icon)).size_full())
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.colors.card_foreground_primary)
                                    .child(name),
                            )
                            .child(status),
                    )
            });

        div().flex_grow().px(rems(1.5)).children(cards)
    }
}
