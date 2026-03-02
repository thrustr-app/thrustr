use crate::{
    conversions::image::image_to_gpui,
    globals::{EventListenerExt, StorefrontManagerExt},
};
use gpui::{
    Context, FontWeight, Image as GpuiImage, ImageSource, IntoElement, ParentElement, Render,
    SharedString, Styled, Task, Window, div, img, prelude::FluentBuilder, rems, svg,
};
use ports::{managers::StorefrontManager, providers::StorefrontProviderStatus};
use std::sync::Arc;
use theme_manager::ThemeExt;
use ui::Card;

#[derive(Clone)]
struct StorefrontProvider {
    name: SharedString,
    status: StorefrontProviderStatus,
    icon: Option<Arc<GpuiImage>>,
    plugin: Option<SharedString>,
}

pub struct Storefronts {
    providers: Vec<StorefrontProvider>,
    _tasks: Vec<Task<()>>,
}

impl Storefronts {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            providers: Vec::new(),
            _tasks: Vec::new(),
        };

        let task = cx.listen("storefront", |page, cx| {
            page.refresh_providers(cx);
        });
        page._tasks.push(task);

        page.refresh_providers(cx);
        page
    }

    pub fn refresh_providers(&mut self, cx: &mut Context<Self>) {
        let mut providers: Vec<StorefrontProvider> = cx
            .storefront_manager()
            .storefront_providers()
            .into_iter()
            .map(|provider| StorefrontProvider {
                name: provider.name().to_string().into(),
                status: provider.status(),
                icon: provider.icon().map(image_to_gpui),
                plugin: provider
                    .origin()
                    .plugin_id()
                    .map(|id| id.to_string().into()),
            })
            .collect();

        providers.sort_by(|a, b| a.name.cmp(&b.name));
        self.providers = providers;
        cx.notify();
    }
}

impl Render for Storefronts {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let cards = self.providers.clone().into_iter().map(|provider| {
            let mut status = div().font_weight(FontWeight::BOLD).text_size(rems(0.6));
            match provider.status {
                StorefrontProviderStatus::Initializing => {
                    status = status
                        .text_color(theme.colors.warning)
                        .child("INITIALIZING");
                }
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
                .relative()
                .gap(rems(1.))
                .size(rems(11.))
                .when_some(provider.icon, |card, icon| {
                    card.child(img(ImageSource::Image(icon)).size_full())
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap(rems(0.5))
                                .when_some(provider.plugin, |this, _| {
                                    this.child(
                                        svg()
                                            .path("icons/plugins.svg")
                                            .size(rems(1.))
                                            .flex_shrink_0()
                                            .text_color(theme.colors.card_foreground_primary),
                                    )
                                })
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.colors.card_foreground_primary)
                                .child(provider.name),
                        )
                        .child(status),
                )
        });

        div()
            .flex_grow()
            .flex()
            .gap(rems(1.5))
            .px(rems(1.5))
            .children(cards)
    }
}
