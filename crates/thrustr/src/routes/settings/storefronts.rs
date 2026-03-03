use crate::{
    conversions::image::image_to_gpui,
    globals::{EventListenerExt, StorefrontManagerExt},
};
use gpui::{
    Context, FontWeight, Image as GpuiImage, ImageSource, IntoElement, ParentElement, Render,
    SharedString, Styled, Task, Window, div, img, prelude::FluentBuilder, rems, svg,
};
use ports::{capabilities::StorefrontStatus, managers::StorefrontManager};
use std::sync::Arc;
use theme_manager::ThemeExt;
use ui::Card;

#[derive(Clone)]
struct Storefront {
    name: SharedString,
    status: StorefrontStatus,
    icon: Option<Arc<GpuiImage>>,
    plugin: Option<SharedString>,
}

pub struct Storefronts {
    storefronts: Vec<Storefront>,
    _tasks: Vec<Task<()>>,
}

impl Storefronts {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            storefronts: Vec::new(),
            _tasks: Vec::new(),
        };

        let task = cx.listen("storefront", |page, cx| {
            page.refresh_storefronts(cx);
        });
        page._tasks.push(task);

        page.refresh_storefronts(cx);
        page
    }

    pub fn refresh_storefronts(&mut self, cx: &mut Context<Self>) {
        let mut storefronts: Vec<Storefront> = cx
            .storefront_manager()
            .storefronts()
            .into_iter()
            .map(|storefront| Storefront {
                name: storefront.name().to_string().into(),
                status: storefront.status(),
                icon: storefront.icon().map(image_to_gpui),
                plugin: storefront
                    .origin()
                    .plugin_id()
                    .map(|id| id.to_string().into()),
            })
            .collect();

        storefronts.sort_by(|a, b| a.name.cmp(&b.name));
        self.storefronts = storefronts;
        cx.notify();
    }
}

impl Render for Storefronts {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let cards = self.storefronts.clone().into_iter().map(|storefront| {
            let mut status = div().font_weight(FontWeight::BOLD).text_size(rems(0.6));
            match storefront.status {
                StorefrontStatus::Initializing => {
                    status = status
                        .text_color(theme.colors.warning)
                        .child("INITIALIZING");
                }
                StorefrontStatus::Active => {
                    status = status.text_color(theme.colors.accent).child("ACTIVE");
                }
                StorefrontStatus::Inactive => {
                    status = status
                        .text_color(theme.colors.card_foreground_secondary)
                        .child("INACTIVE");
                }
                StorefrontStatus::Error(_) => {
                    status = status.text_color(theme.colors.error).child("ERROR");
                }
            }

            Card::new()
                .relative()
                .gap(rems(1.))
                .size(rems(11.))
                .when_some(storefront.icon, |card, icon| {
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
                                .when_some(storefront.plugin, |this, _| {
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
                                .child(storefront.name),
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
