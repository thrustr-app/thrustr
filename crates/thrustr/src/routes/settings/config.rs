use crate::{
    conversions::image::image_to_gpui,
    extensions::{EventListenerExt, SpawnTaskExt},
    globals::ComponentManagerExt,
    navigation::NavigationExt,
    webview::{WebviewError, open_auth_webview},
};
use component_manager::ComponentHandle;
use domain::component::{Field as ConfigField, LoginMethod, Section as ConfigSection, Status};
use gpui::{
    ClickEvent, Context, FontWeight, Image, ImageSource, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Task, Window, div,
    img, prelude::FluentBuilder, rems, svg,
};
use gpui_tokio::Tokio;
use smol::unblock;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use theme_manager::ThemeExt;
use ui::{Alert, Button, Card, InputEvent, Label, PortalContext, WithSize, WithVariant, input};

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
    status: Status,
    local_error: Option<SharedString>,
    status_error: Option<SharedString>,
    login_method: Option<LoginMethod>,
    authenticating: bool,
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
            .map(|c| c.sections.iter().map(Into::into).collect())
            .unwrap_or_default();

        let _tasks = vec![cx.listen("component", Self::refresh_status)];

        let status = component.status();
        let mut page = Self {
            name: component.metadata().name.clone().into(),
            icon,
            status_error: status.error_message().map(Into::into),
            status: status,
            component,
            sections,
            values,
            local_error: None,
            login_method: None,
            authenticating: false,
            _tasks,
        };

        page.get_login_method(cx);
        page
    }

    fn refresh_status(&mut self, cx: &mut Context<Self>) {
        let status = self.component.status();
        self.status_error = status.error_message().map(Into::into);
        self.status = status;
        cx.notify();
    }

    fn get_login_method(&mut self, cx: &mut Context<Self>) {
        let component = self.component.clone();
        cx.spawn_and_update_tokio(
            async move { component.get_login_method().await },
            |config, result, cx| {
                config.login_method = match result {
                    Ok(method) => method,
                    Err(err) => {
                        config.local_error = Some(err.into());
                        None
                    }
                };
                cx.notify();
            },
        );
    }

    fn on_save(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let fields = self
            .values
            .iter()
            .map(|(id, value)| (id.to_string(), value.to_string()))
            .collect::<Vec<_>>();

        let component = self.component.clone();

        cx.spawn_and_update_tokio(
            async move { component.save_config(&fields).await },
            |config, result, cx| {
                config.local_error = result.err().map(|e| e.to_string().into());
                cx.notify();
            },
        );
    }

    fn on_login(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.authenticating {
            return;
        }
        let Some(login_method) = self.login_method.as_ref() else {
            return;
        };

        self.authenticating = true;
        match login_method {
            LoginMethod::Flow(_) => self.handle_login_flow(cx),
            LoginMethod::Form(_) => self.handle_login_form(window, cx),
        }
        cx.notify();
    }

    fn handle_login_flow(&mut self, cx: &mut Context<Self>) {
        let Some(LoginMethod::Flow(login_flow)) = self.login_method.clone() else {
            return;
        };
        let component = self.component.clone();

        cx.spawn_and_update_tokio(
            async move {
                let result =
                    unblock(move || open_auth_webview(&login_flow.url, &login_flow.target)).await;
                match result {
                    Ok((url, body)) => component.login(Some(url), Some(body), None).await,
                    Err(WebviewError::UserCancelled) => Ok(()),
                    Err(WebviewError::Internal(e)) => Err(e),
                }
            },
            |config, result, cx| {
                config.authenticating = false;
                config.local_error = result.err().map(|e| e.to_string().into());
                cx.notify();
            },
        );
    }

    fn handle_login_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(LoginMethod::Form(login_form)) = self.login_method.clone() else {
            return;
        };

        let component = self.component.clone();
        let entity = cx.entity().downgrade();
        let form_values = Rc::new(RefCell::new(HashMap::<SharedString, SharedString>::new()));

        window.open_dialog(cx, move |dialog, _, _| {
            let login_form = login_form.clone();
            let required_ids: Vec<SharedString> = login_form
                .fields
                .iter()
                .filter_map(|f| match f {
                    ConfigField::Text {
                        id, required: true, ..
                    } => Some(id.into()),
                    _ => None,
                })
                .collect();

            let form_values = form_values.clone();
            let component = component.clone();
            let entity = entity.clone();

            let fields = login_form.fields.into_iter().map(|f| match f {
                ConfigField::Text {
                    id,
                    label,
                    placeholder,
                    ..
                } => {
                    let id: SharedString = id.into();
                    let label: SharedString = label.into();
                    let placeholder: Option<SharedString> = placeholder.map(Into::into);
                    let form_values = form_values.clone();
                    let id_clone = id.clone();
                    let current_value = form_values.borrow().get(&id).cloned().unwrap_or_default();

                    input(id)
                        .label(label)
                        .w(rems(20.))
                        .when_some(placeholder, |input, placeholder| {
                            input.placeholder(placeholder)
                        })
                        .value(current_value)
                        .on_input(move |event: &InputEvent, _, _| {
                            form_values
                                .borrow_mut()
                                .insert(id_clone.clone(), event.value.clone());
                        })
                }
            });

            let form_values_for_ok = form_values.clone();
            let form_values_for_disabled = form_values.clone();
            let entity_for_cancel = entity.clone();
            dialog
                .title("Log In")
                .ok_text("Log In")
                .when(
                    {
                        let values = form_values_for_disabled.borrow();
                        required_ids
                            .iter()
                            .any(|id| values.get(id).map_or(true, |v| v.is_empty()))
                    },
                    |dialog| dialog.disabled(),
                )
                .on_ok(move |_, _, cx| {
                    let fields = form_values_for_ok
                        .borrow()
                        .iter()
                        .map(|(id, value)| (id.to_string(), value.to_string()))
                        .collect::<Vec<_>>();
                    let component = component.clone();

                    let task = Tokio::spawn(cx, async move {
                        component.login(None, None, Some(fields)).await
                    });

                    let entity = entity.clone();

                    cx.spawn(async move |cx| {
                        let result = task.await;
                        if let Some(entity) = entity.upgrade() {
                            let _ = entity.update(cx, |config, cx| {
                                config.authenticating = false;

                                let error = match result {
                                    Ok(Ok(())) => None,
                                    Ok(Err(e)) => Some(e.to_string()),
                                    Err(e) => Some(e.to_string()),
                                };
                                config.local_error = error.map(Into::into);

                                cx.notify();
                            });
                        }
                    })
                    .detach();
                })
                .on_cancel(move |_, _, cx| {
                    if let Some(entity) = entity_for_cancel.upgrade() {
                        let _ = entity.update(cx, |config, cx| {
                            config.authenticating = false;
                            cx.notify();
                        });
                    }
                })
                .child(div().flex().flex_col().gap(rems(1.5)).children(fields))
        });
    }

    fn on_logout(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.authenticating {
            return;
        }
        self.authenticating = true;

        let component = self.component.clone();
        cx.spawn_and_update_tokio(
            async move {
                if let Some(flow) = component.get_logout_flow().await? {
                    match unblock(move || open_auth_webview(&flow.url, &flow.target)).await {
                        Ok(_) => {}
                        Err(WebviewError::UserCancelled) => return Ok(()),
                        Err(WebviewError::Internal(e)) => return Err(e),
                    }
                }
                component.logout().await
            },
            |this, result, cx| {
                this.authenticating = false;
                this.local_error = result.err().map(|e| e.to_string().into());
                cx.notify();
            },
        );
        cx.notify();
    }

    fn render_header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let has_login = self.login_method.is_some();

        let status_label = match self.status {
            Status::Initializing => Label::new("INITIALIZING").variant_warning(),
            Status::Unauthenticated => Label::new("UNAUTHENTICATED").variant_warning(),
            Status::Active => Label::new("ACTIVE").variant_accent(),
            Status::Inactive => Label::new("INACTIVE"),
            Status::Error(_) | Status::InitError(_) => Label::new("ERROR").variant_destructive(),
        };

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
                            .on_click(|_, _, cx| cx.navigate_back()),
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
                    .when(!self.sections.is_empty(), |div| {
                        div.child(
                            Button::new("save")
                                .when(!self.status.can_configure(), |btn| btn.disabled())
                                .size_lg()
                                .child("Save")
                                .w(rems(10.))
                                .on_click(cx.listener(Self::on_save)),
                        )
                    })
                    .when(has_login && self.status.can_login(), |div| {
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
                    // There must be a login method for a logout flow to exist, but a logout flow might not be required.
                    .when(has_login && self.status.can_logout(), |div| {
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
            )
    }

    fn render_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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
                        config.values.insert(field_id.clone(), event.value.clone());
                    }))
            });

            Card::new(s.name.clone())
                .flex_shrink_0()
                .title(s.name.clone())
                .child(div().flex().flex_col().gap(rems(1.5)).children(fields))
        });

        div()
            .flex()
            .flex_col()
            .flex_grow()
            .h_0()
            .gap(rems(1.5))
            .id("config-form")
            .overflow_y_scroll()
            .when_some(self.local_error.clone(), |div, error| {
                div.child(Alert::new().title("Error").description(error))
            })
            .when_some(self.status_error.clone(), |div, error| {
                div.child(Alert::new().title("Error").description(error))
            })
            .children(sections)
    }
}

impl Render for Config {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_grow()
            .pl(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(2.))
            .child(self.render_header(cx))
            .child(self.render_body(cx))
    }
}

impl From<&ConfigSection> for Section {
    fn from(section: &ConfigSection) -> Self {
        Section {
            name: section.name.to_string().into(),
            fields: section.fields.iter().map(Into::into).collect(),
        }
    }
}

impl From<&ConfigField> for Field {
    fn from(field: &ConfigField) -> Self {
        match field {
            ConfigField::Text {
                id,
                label,
                placeholder,
                ..
            } => Field {
                id: id.into(),
                label: label.into(),
                placeholder: placeholder.clone().map(Into::into),
            },
        }
    }
}
