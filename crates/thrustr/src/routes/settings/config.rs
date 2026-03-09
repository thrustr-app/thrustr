use crate::{
    conversions::image::image_to_gpui,
    globals::{ComponentManagerExt, EventListenerExt},
    navigation::NavigationExt,
    webview::{WebviewError, open_auth_webview},
};
use component_manager::ComponentHandle;
use gpui::{
    AppContext, ClickEvent, Context, FontWeight, Image, ImageSource, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Task,
    Window, div, img, prelude::FluentBuilder, rems, svg,
};
use gpui_tokio::Tokio;
use ports::component::{AuthFlow, Field as ConfigField, Status};
use smol::unblock;
use std::{collections::HashMap, sync::Arc};
use theme_manager::ThemeExt;
use ui::{Alert, Button, Card, InputEvent, Label, WithSize, WithVariant, input};

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
    component: ComponentHandle,
    sections: Vec<Section>,
    values: HashMap<SharedString, SharedString>,
    error: Option<SharedString>,
    login_flow: Option<AuthFlow>,
    authenticating: bool,
    status: Status,
    _tasks: Vec<Task<()>>,
}

impl Config {
    pub fn new(cx: &mut Context<Self>, component_id: &str) -> Self {
        let component = cx.component(component_id).unwrap();
        let metadata = component.metadata();
        let icon = metadata.icon.clone().map(image_to_gpui);

        let values: HashMap<SharedString, SharedString> = component
            .get_config_values()
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();

        let sections = component
            .config()
            .map(|config| {
                config
                    .sections
                    .iter()
                    .map(|section| {
                        let fields = section
                            .fields
                            .iter()
                            .map(|field| match field {
                                ConfigField::Text {
                                    id,
                                    label,
                                    placeholder,
                                } => Field {
                                    id: id.clone().into(),
                                    label: label.clone().into(),
                                    placeholder: placeholder.clone().map(Into::into),
                                },
                            })
                            .collect();

                        Section {
                            name: section.name.clone().into(),
                            fields,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let error = match component.status() {
            Status::InitError(e) | Status::Error(e) => Some(e.to_string().into()),
            _ => None,
        };

        let _tasks = vec![cx.listen("component", Self::refresh_status)];

        let page = Self {
            name: component.metadata().name.clone().into(),
            icon,
            status: component.status(),
            component,
            sections,
            values,
            error,
            login_flow: None,
            authenticating: false,
            _tasks,
        };

        page.get_login_flow(cx);
        page
    }

    fn refresh_status(&mut self, cx: &mut Context<Self>) {
        self.status = self.component.status();
        cx.notify();
    }

    fn get_login_flow(&self, cx: &mut Context<Self>) {
        let component = self.component.clone();
        let login_flow_task = Tokio::spawn(cx, async move { component.get_login_flow().await });
        cx.spawn(async move |config, cx| {
            let result = login_flow_task.await.unwrap().unwrap();
            let _ = config.update(cx, |config, cx| {
                config.login_flow = result;
                cx.notify();
            });
        })
        .detach();
    }

    fn on_input(&mut self, id: SharedString, value: SharedString) {
        self.values.insert(id.clone(), value.clone());
    }

    fn on_save(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let config_fields = Arc::new(
            self.values
                .iter()
                .map(|(id, value)| (id.to_string(), value.to_string()))
                .collect::<Vec<_>>(),
        );

        let component = self.component.clone();
        let validate_task = Tokio::spawn(
            cx,
            async move { component.save_config(&config_fields).await },
        );

        cx.spawn(async move |config, cx| {
            let result = validate_task.await.unwrap();
            let _ = config.update(cx, |config, cx| {
                config.error = result.err().map(|e| e.to_string().into());
                cx.notify();
            });
        })
        .detach();
    }

    fn on_login(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.authenticating {
            return;
        }
        self.authenticating = true;
        if let Some(login_flow) = self.login_flow.clone() {
            let component = self.component.clone();
            let task = Tokio::spawn(cx, async move {
                let result =
                    unblock(move || open_auth_webview(&login_flow.url, &login_flow.target)).await;
                match result {
                    Ok((url, body)) => component.login(url, body).await,
                    Err(WebviewError::UserCancelled) => {
                        Err("Authentication cancelled by user".into())
                    }
                    Err(WebviewError::Internal(e)) => Err(e.into()),
                }
            });

            cx.spawn(async move |config, cx| {
                let result = task.await.unwrap();
                let _ = config.update(cx, |config, cx| {
                    config.authenticating = false;
                    config.error = result.err().map(|e| e.to_string().into());
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn on_logout(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.authenticating {
            return;
        }
        self.authenticating = true;
        let component = self.component.clone();
        let task = cx.background_spawn(async move {
            let logout_flow = component.get_logout_flow().await?;
            let result = if let Some(logout_flow) = logout_flow {
                unblock(move || open_auth_webview(&logout_flow.url, &logout_flow.target)).await
            } else {
                Ok(("".to_string(), "".to_string()))
            };

            match result {
                Ok((url, body)) => component.logout(url, body).await,
                Err(WebviewError::UserCancelled) => Err("Logout cancelled by user".into()),
                Err(WebviewError::Internal(e)) => Err(e.into()),
            }
        });

        cx.spawn(async move |config, cx| {
            let result = task.await;
            let _ = config.update(cx, |config, cx| {
                config.authenticating = false;
                config.error = result.err().map(|e| e.to_string().into());
                cx.notify();
            });
        })
        .detach();

        cx.notify();
    }
}

impl Render for Config {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let sections = self.sections.iter().map(|s| {
            let fields = s.fields.iter().map(|f| {
                let field_id = f.id.clone();
                input(f.id.clone())
                    .when(!self.status.can_configure(), |btn| btn.disabled())
                    .label(f.label.clone())
                    .max_w(rems(20.))
                    .when_some(f.placeholder.clone(), |input, placeholder| {
                        input.placeholder(placeholder)
                    })
                    .value(self.values.get(f.id.as_str()).cloned().unwrap_or_default())
                    .on_input(cx.listener(move |config, event: &InputEvent, _, _| {
                        config.on_input(field_id.clone(), event.value.clone());
                    }))
            });

            Card::new(s.name.clone())
                .flex_shrink_0()
                .title(s.name.clone())
                .child(div().flex().flex_col().gap(rems(1.5)).children(fields))
        });

        let status_label = match self.status {
            Status::Initializing => Label::new("INITIALIZING").variant_warning(),
            Status::Unauthenticated => Label::new("UNAUTHENTICATED").variant_warning(),
            Status::Active => Label::new("ACTIVE").variant_accent(),
            Status::Inactive => Label::new("INACTIVE"),
            Status::Error(_) | Status::InitError(_) => Label::new("ERROR").variant_destructive(),
        };

        let login_flow_exists = self.login_flow.is_some();

        div()
            .flex_grow()
            .pl(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(2.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .gap(rems(1.5))
                            .items_center()
                            .text_color(theme.colors.primary)
                            .child(
                                Button::new("back-button")
                                    .variant_ghost()
                                    .child(
                                        svg()
                                            .path("icons/arrow-left.svg")
                                            .size_full()
                                            .text_color(theme.colors.primary),
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
                                    .child(self.name.clone())
                                    .child(status_label),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(rems(1.))
                            .when(sections.len() > 0, |div| {
                                div.child(
                                    Button::new("save")
                                        .when(!self.status.can_configure(), |btn| btn.disabled())
                                        .size_lg()
                                        .child("Save")
                                        .w(rems(10.))
                                        .on_click(cx.listener(Self::on_save)),
                                )
                            })
                            .when(login_flow_exists && self.status.can_login(), |div| {
                                div.child(
                                    Button::new("login")
                                        .when(self.authenticating, |btn| btn.loading())
                                        .variant_accent()
                                        .size_lg()
                                        .child("Log In")
                                        .w(rems(10.))
                                        .on_click(cx.listener(Self::on_login)),
                                )
                            })
                            // There must be a login flow for a logout flow to exists, but a logout flow might not be required.
                            .when(login_flow_exists && self.status.can_logout(), |div| {
                                div.child(
                                    Button::new("logout")
                                        .when(self.authenticating, |btn| btn.loading())
                                        .variant_ghost()
                                        .size_lg()
                                        .child("Log Out")
                                        .w(rems(10.))
                                        .on_click(cx.listener(Self::on_logout)),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_grow()
                    .h_0()
                    .gap(rems(1.5))
                    .id("config-form")
                    .overflow_y_scroll()
                    .when_some(self.error.clone(), |div, error| {
                        div.child(Alert::new().title("Error").description(error))
                    })
                    .children(sections),
            )
    }
}
