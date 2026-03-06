use crate::{
    conversions::image::image_to_gpui, globals::ComponentManagerExt, navigation::NavigationExt,
};
use gpui::{
    Context, FontWeight, Image, ImageSource, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, div, img, prelude::FluentBuilder, rems, svg,
};
use ports::component::{Component, Field as ConfigField};
use std::sync::Arc;
use theme_manager::ThemeExt;
use ui::{Button, Card, input};

struct Field {
    id: SharedString,
    label: SharedString,
    placeholder: Option<SharedString>,
}

struct Section {
    name: SharedString,
    fields: Vec<Field>,
}

pub struct Config {
    name: SharedString,
    icon: Option<Arc<Image>>,
    component: Arc<dyn Component>,
    sections: Vec<Section>,
}

impl Config {
    pub fn new(cx: &mut Context<Self>, component_id: &str) -> Self {
        let component = cx.component_manager().component(component_id).unwrap();
        let metadata = component.metadata();
        let icon = metadata.icon.clone().map(image_to_gpui);

        let mut sections = Vec::new();

        if let Some(config) = component.config() {
            for section in config.sections.iter() {
                let mut fields = Vec::new();
                for field in section.fields.iter() {
                    let field = match field {
                        ConfigField::Text { id, label } => Field {
                            id: id.clone().into(),
                            label: label.clone().into(),
                            placeholder: None,
                        },
                    };
                    fields.push(field);
                }

                sections.push(Section {
                    name: section.name.clone().into(),
                    fields,
                });
            }
        }

        let page = Self {
            name: component.metadata().name.clone().into(),
            icon,
            component,
            sections,
        };
        page
    }
}

impl Render for Config {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let sections = self.sections.iter().map(|s| {
            let fields = s.fields.iter().map(|f| {
                div()
                    .child(
                        div()
                            .child(f.label.clone())
                            .mb(rems(0.5))
                            .text_color(theme.colors.card_primary),
                    )
                    .child(
                        input(f.id.clone())
                            .max_w(rems(20.))
                            .when_some(f.placeholder.clone(), |input, placeholder| {
                                input.placeholder(placeholder)
                            }),
                    )
            });

            Card::new("section")
                .title(s.name.clone())
                .child(div().flex().flex_col().gap(rems(1.5)).children(fields))
        });

        div()
            .flex_grow()
            .pl(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(2.))
            .child(
                div()
                    .flex()
                    .gap(rems(1.5))
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
            .children(sections)
    }
}
