use crate::{
    adapters::ImageExt,
    context::EventListenerExt,
    globals::ComponentRegistryExt,
    navigation::{NavigatorExt, SettingsPage},
};
use domain::component::Status;
use event::Topic;
use gpui::{
    Context, FontWeight, Image as GpuiImage, ImageSource, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Task, Transformation,
    Window, div, img, percentage, prelude::FluentBuilder, relative, rems,
};
use std::sync::Arc;
use theme::{Theme, ThemeExt};
use ui::{Alert, Icon, Label, WithSize, WithVariant};

#[derive(Clone)]
struct Storefront {
    id: SharedString,
    name: SharedString,
    status: Status,
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

        let task = cx.listen(Topic::Component, |page, cx| {
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
                Storefront {
                    id: component.id().into(),
                    name: component.metadata().name.into(),
                    status: component.status(),
                    icon: component.metadata().icon.map(|i| i.to_gpui()),
                    plugin: component
                        .metadata()
                        .origin
                        .is_plugin()
                        .then(|| component.id().into()),
                }
            })
            .collect();

        storefronts.sort_by(|a, b| a.name.cmp(&b.name));
        self.has_errors = storefronts.iter().any(|s| s.status.is_any_error());
        self.storefronts = storefronts;
        cx.notify();
    }
}

impl Render for Storefronts {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let count = self.storefronts.len();
        let rows = self
            .storefronts
            .iter()
            .enumerate()
            .map(|(index, storefront)| render_row(index, count, storefront, &theme));

        div()
            .flex_grow_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(rems(1.25))
            .when(self.has_errors, |div| {
                div.child(Alert::new(SharedString::new_static(
                    "There are storefronts with errors. Open them to see more details.",
                )))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(theme.colors.border)
                    .rounded(theme.radius.lg)
                    .overflow_hidden()
                    .children(rows),
            )
    }
}

fn render_row(
    index: usize,
    count: usize,
    storefront: &Storefront,
    theme: &Theme,
) -> impl IntoElement {
    let storefront_id = storefront.id.clone();
    let is_plugin = storefront.plugin.is_some();

    div()
        .id(storefront.id.clone())
        .focusable()
        .tab_stop(true)
        .w_full()
        .h(rems(3.5))
        .px(rems(1.25))
        .flex()
        .justify_between()
        .items_center()
        .cursor_pointer()
        .when(index == 0, |this| this.rounded_t(theme.radius.lg))
        .hover(|this| this.bg(theme.colors.hover))
        .focus_visible(|this| this.bg(theme.colors.hover))
        .when_else(
            index + 1 < count,
            |this| this.border_b_1().border_color(theme.colors.border),
            |this| this.rounded_b(theme.radius.lg),
        )
        .on_click(move |_, _, cx| {
            let route = if is_plugin {
                SettingsPage::Plugins(Some(storefront_id.clone()))
            } else {
                SettingsPage::Storefronts(Some(storefront_id.clone()))
            };

            cx.navigate(route);
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(rems(0.875))
                .when_some(storefront.icon.clone(), |this, icon| {
                    this.child(
                        img(ImageSource::Image(icon))
                            .size(rems(1.5))
                            .rounded(theme.radius.md),
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rems(0.375))
                        .when(is_plugin, |this| {
                            this.child(Icon::plugin().size_sm().color(theme.colors.secondary))
                        })
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(rems(0.875))
                        .line_height(relative(1.))
                        .text_color(theme.colors.primary)
                        .child(storefront.name.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(rems(0.875))
                .child(status_label(&storefront.status))
                .child(
                    Icon::arrow()
                        .size_sm()
                        .color(theme.colors.tertiary)
                        .transform(Transformation::rotate(percentage(0.5))),
                ),
        )
}

fn status_label(status: &Status) -> Label {
    match status {
        Status::Initializing => Label::transparent("INITIALIZING").variant_secondary(),
        Status::Unauthenticated => Label::transparent("UNAUTHENTICATED").variant_warning(),
        Status::Active => Label::transparent("ACTIVE").variant_accent(),
        Status::Inactive => Label::transparent("INACTIVE"),
        Status::Error(_) | Status::InitError(_) => Label::transparent("ERROR").variant_danger(),
    }
    .status_dot(matches!(status, Status::Initializing))
}
