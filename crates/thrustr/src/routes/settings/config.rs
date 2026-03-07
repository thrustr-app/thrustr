use crate::{
    conversions::image::image_to_gpui, globals::ComponentManagerExt, navigation::NavigationExt,
};
use gpui::{
    AppContext, ClickEvent, Context, FontWeight, Image, ImageSource, IntoElement, ParentElement,
    Render, SharedString, Styled, Window, div, img, prelude::FluentBuilder, rems, svg,
};
use ports::component::{Component, Field as ConfigField};
use std::{collections::HashMap, sync::Arc};
use theme_manager::ThemeExt;
use ui::{Button, Card, InputEvent, WithSize, WithVariant, input};

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
    values: HashMap<SharedString, SharedString>,
}

impl Config {
    pub fn new(cx: &mut Context<Self>, component_id: &str) -> Self {
        let component = cx.component_manager().component(component_id).unwrap();
        let metadata = component.metadata();
        let icon = metadata.icon.clone().map(image_to_gpui);

        let mut sections = Vec::new();
        let mut values = HashMap::new();

        if let Some(config) = component.config() {
            for section in config.sections.iter() {
                let mut fields = Vec::new();
                for field in section.fields.iter() {
                    values.insert(field.id().to_string().into(), SharedString::new(""));
                    let field = match field {
                        ConfigField::Text {
                            id,
                            label,
                            placeholder,
                        } => Field {
                            id: id.clone().into(),
                            label: label.clone().into(),
                            placeholder: placeholder.clone().map(Into::into),
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
            values,
        };
        page
    }

    fn on_input(&mut self, id: SharedString, value: SharedString) {
        self.values.insert(id.clone(), value.clone());
    }

    fn on_submit(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let config_fields = Arc::new(
            self.values
                .iter()
                .map(|(id, value)| (id.to_string(), value.to_string()))
                .collect::<Vec<_>>(),
        );

        let component = self.component.clone();
        let component_manager = cx.component_manager();

        let validate_task = cx.background_spawn(async move {
            let result = component.validate_config(&config_fields).await;
            if result.is_ok() {
                component_manager.save_config(&component.metadata().id, &config_fields);
            }
            result
        });

        cx.spawn(async move |_, _| {
            let _ = validate_task.await;
        })
        .detach();
    }
}

impl Render for Config {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let sections = self.sections.iter().map(|s| {
            let fields = s.fields.iter().map(|f| {
                let field_id = f.id.clone();
                input(f.id.clone())
                    .label(f.label.clone())
                    .max_w(rems(20.))
                    .when_some(f.placeholder.clone(), |input, placeholder| {
                        input.placeholder(placeholder)
                    })
                    .value(self.values.get(f.id.as_str()).unwrap())
                    .on_input(cx.listener(move |config, event: &InputEvent, _, _| {
                        config.on_input(field_id.clone(), event.value.clone());
                    }))
            });

            Card::new(s.name.clone())
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
                            .variant_ghost()
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
            .child(
                Button::new("submit")
                    .size_lg()
                    .child("Save")
                    .circular()
                    .max_w(rems(10.))
                    .on_click(cx.listener(Self::on_submit)),
            )
    }
}
