use crate::{conversions::image::image_format_to_gpui, globals::StorefrontManagerExt};
use gpui::{
    Context, FontWeight, Image as GpuiImage, ImageSource, IntoElement, ParentElement, Render,
    SharedString, Styled, Window, div, img, prelude::FluentBuilder, rems,
};
use ports::{managers::StorefrontManager, providers::StorefrontProviderStatus};
use std::sync::Arc;
use theme_manager::ThemeExt;
use ui::Card;

struct StorefrontProvider {
    name: SharedString,
    status: StorefrontProviderStatus,
    icon: Option<Arc<GpuiImage>>,
}

pub struct Storefronts {
    providers: Vec<StorefrontProvider>,
}

impl Storefronts {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            providers: Vec::new(),
        };
        page.refresh_providers(cx);
        page
    }

    pub fn refresh_providers(&mut self, cx: &mut Context<Self>) {
        self.providers = cx
            .storefront_manager()
            .storefront_providers()
            .into_iter()
            .map(|provider| StorefrontProvider {
                name: provider.name().to_string().into(),
                status: provider.status(),
                icon: provider.icon().map(|icon| {
                    Arc::new(GpuiImage::from_bytes(
                        image_format_to_gpui(icon.format),
                        icon.bytes.clone(),
                    ))
                }),
            })
            .collect();
        cx.notify();
    }
}

impl Render for Storefronts {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let cards = self.providers.iter().map(|provider| {
            let mut status = div().font_weight(FontWeight::BOLD).text_size(rems(0.6));
            match provider.status {
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
                .when_some(provider.icon.clone(), |card, icon| {
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
                                .child(provider.name.clone()),
                        )
                        .child(status),
                )
        });

        div().flex_grow().px(rems(1.5)).children(cards)
    }
}
