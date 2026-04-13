use crate::{
    conversions::image::image_to_gpui,
    extensions::EventListenerExt,
    globals::ComponentRegistryExt,
    navigation::{NavigationExt, SettingsPage},
};
use domain::component::ComponentStatus;
use gpui::{
    Context, FontWeight, Image as GpuiImage, ImageSource, IntoElement, ParentElement, Render,
    SharedString, Styled, Task, Window, div, img, prelude::FluentBuilder, rems, svg,
};
use std::sync::Arc;
use theme_manager::ThemeExt;
use ui::{Alert, Card};

#[derive(Clone)]
struct Storefront {
    id: SharedString,
    name: SharedString,
    status: ComponentStatus,
    icon: Option<Arc<GpuiImage>>,
    plugin: Option<SharedString>,
}

pub struct Storefronts {
    storefronts: Vec<Storefront>,
    has_errors: bool,
    _tasks: Vec<Task<()>>,
}

impl Storefronts {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            storefronts: Vec::new(),
            has_errors: false,
            _tasks: Vec::new(),
        };

        let task = cx.listen("component", |page, cx| {
            page.refresh_storefronts(cx);
        });
        page._tasks.push(task);

        page.refresh_storefronts(cx);
        page
    }

    pub fn refresh_storefronts(&mut self, cx: &mut Context<Self>) {
        let mut storefronts: Vec<Storefront> = cx
            .storefronts()
            .into_iter()
            .map(|storefront| {
                let component = storefront.component();
                if component.status().is_error() {
                    self.has_errors = true;
                }
                Storefront {
                    id: component.id().to_owned().into(),
                    name: component.metadata().name.to_owned().into(),
                    status: component.status(),
                    icon: component.metadata().icon.clone().map(image_to_gpui),
                    plugin: component
                        .metadata()
                        .origin
                        .is_plugin()
                        .then(|| component.id().to_string().into()),
                }
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
                ComponentStatus::Initializing => {
                    status = status
                        .text_color(theme.colors.warning)
                        .child("INITIALIZING");
                }
                ComponentStatus::Unauthenticated => {
                    status = status
                        .text_color(theme.colors.warning)
                        .child("UNAUTHENTICATED");
                }
                ComponentStatus::Active => {
                    status = status.text_color(theme.colors.accent).child("ACTIVE");
                }
                ComponentStatus::Inactive => {
                    status = status
                        .text_color(theme.colors.card_secondary)
                        .child("INACTIVE");
                }
                ComponentStatus::Error(_) | ComponentStatus::InitError(_) => {
                    status = status.text_color(theme.colors.error).child("ERROR");
                }
            }

            let sorefront_id = storefront.id.clone();
            let is_plugin = storefront.plugin.is_some();

            Card::new(storefront.id)
                .relative()
                .gap(rems(1.))
                .size(rems(11.))
                .when_some(storefront.icon, |card, icon| {
                    card.child(img(ImageSource::Image(icon)).size_full())
                })
                .on_click(move |_, _, cx| {
                    let route = if is_plugin {
                        SettingsPage::Plugins(Some(sorefront_id.clone()))
                    } else {
                        SettingsPage::Storefronts(Some(sorefront_id.clone()))
                    };

                    cx.navigate(route);
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
                                            .text_color(theme.colors.card_primary),
                                    )
                                })
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.colors.card_primary)
                                .child(storefront.name),
                        )
                        .child(status),
                )
        });

        div()
            .flex_grow()
            .flex()
            .flex_col()
            .px(rems(1.5))
            .gap(rems(1.5))
            .when(self.has_errors, |div| {
                div.child(Alert::new().title("Storefront errors").description(
                    "One or more storefronts have encountered errors, open them to view details.",
                ))
            })
            .child(div().flex().gap(rems(1.5)).children(cards))
    }
}
