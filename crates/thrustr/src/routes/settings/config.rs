use crate::{
    conversions::image::image_to_gpui,
    extensions::{EventListenerExt, SpawnTaskExt},
    navigation::NavigatorExt,
    webview::{WebviewError, open_auth_webview},
};
use component::{ComponentHandle, Operation};
use domain::component::{
    AuthFlow, Field as ConfigField, LoginForm, LoginMethod, LoginRequest, Section as ConfigSection,
    Status,
};
use gpui::{
    AppContext, ClickEvent, Context, Entity, FontWeight, Image, ImageSource, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString, Styled, Task, Window, div, img,
    prelude::FluentBuilder, rems, svg,
};
use smol::unblock;
use std::{collections::HashMap, sync::Arc};
use theme::ThemeExt;
use ui::{
    Alert, Button, Card, InputEvent, Label, PortalContext, WithFocus, WithScrollbar, WithSize,
    WithVariant, input,
};

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
    login_form_view: Option<Entity<LoginFormState>>,
    scroll_handle: ScrollHandle,
    _tasks: Vec<Task<()>>,
}

impl Config {
    pub fn new(cx: &mut Context<Self>, component: ComponentHandle) -> Self {
        let metadata = component.metadata();
        let icon = metadata.icon.to_owned().map(image_to_gpui);

        let mut local_error = None;
        let values: HashMap<SharedString, SharedString> = match component.config_values() {
            Ok(values) => values
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            Err(err) => {
                local_error = Some(err.to_string().into());
                HashMap::new()
            }
        };

        let sections = component
            .config()
            .map(|c| c.sections.into_iter().map(Into::into).collect())
            .unwrap_or_default();

        let _tasks = vec![cx.listen("component", Self::refresh_status)];

        let status = component.status();
        let mut page = Self {
            name: component.metadata().name.to_owned().into(),
            icon,
            status_error: status.error_message().map(Into::into),
            status,
            component,
            sections,
            values,
            local_error,
            login_method: None,
            login_form_view: None,
            scroll_handle: ScrollHandle::new(),
            _tasks,
        };

        page.load_login_method(cx);
        page
    }

    fn refresh_status(&mut self, cx: &mut Context<Self>) {
        let status = self.component.status();
        self.status_error = status.error_message().map(Into::into);
        self.status = status;
        cx.notify();
    }

    fn load_login_method(&mut self, cx: &mut Context<Self>) {
        let component = self.component.clone();
        cx.spawn_and_update(
            async move { component.login_method().await },
            |config, result, _| {
                config.login_method = match result {
                    Ok(method) => method,
                    Err(err) => {
                        config.local_error = Some(err.to_string().into());
                        None
                    }
                };
            },
        );
    }

    fn on_save(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(mut claim) = self.component.begin(Operation::Configure) else {
            return;
        };

        let fields = self
            .values
            .iter()
            .map(|(id, value)| (id.to_string(), value.to_string()))
            .collect();

        let component = self.component.clone();
        cx.spawn_and_update(
            async move { component.save_config(&mut claim, fields).await },
            |config, result, _| {
                config.local_error = result.err().map(|e| e.to_string().into());
            },
        );
        self.refresh_status(cx);
    }

    fn on_login(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        match self.login_method.clone() {
            Some(LoginMethod::Flow(login_flow)) => self.handle_login_flow(login_flow, cx),
            Some(LoginMethod::Form(login_form)) => self.handle_login_form(login_form, window, cx),
            None => {}
        }
    }

    fn handle_login_flow(&mut self, login_flow: AuthFlow, cx: &mut Context<Self>) {
        let Some(mut claim) = self.component.begin(Operation::Login) else {
            return;
        };

        let component = self.component.clone();
        cx.spawn_and_update(
            async move {
                let result =
                    unblock(move || open_auth_webview(&login_flow.url, &login_flow.target)).await;
                match result {
                    Ok((url, body)) => component
                        .login(&mut claim, LoginRequest::Flow { url, body })
                        .await
                        .map_err(|e| e.to_string()),
                    Err(WebviewError::UserCancelled) => Ok(()),
                    Err(WebviewError::Internal(e)) => Err(e),
                }
            },
            |config, result, _| {
                config.local_error = result.err().map(Into::into);
            },
        );
        self.refresh_status(cx);
    }

    fn handle_login_form(
        &mut self,
        login_form: LoginForm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let component = self.component.clone();
        let config_entity = cx.entity().downgrade();

        let form_entity = cx.new(|_| LoginFormState::new(login_form));
        self.login_form_view = Some(form_entity.clone());

        window.open_dialog(cx, move |dialog, _, cx| {
            let form_entity = form_entity.clone();
            let form_entity_child = form_entity.clone();
            let config_entity_for_ok = config_entity.clone();
            let config_entity_for_cancel = config_entity.clone();
            let component = component.clone();

            let is_valid = form_entity.read(cx).is_valid();
            dialog
                .title("Log In")
                .ok_text("Log In")
                .when(!is_valid, |dialog| dialog.disabled())
                .on_ok(move |_, _, cx| {
                    let Some(mut claim) = component.begin(Operation::Login) else {
                        return;
                    };

                    let fields = form_entity.read(cx).login_fields();
                    let config_entity = config_entity_for_ok.clone();
                    let component = component.clone();

                    let task = cx.background_spawn(async move {
                        component
                            .login(&mut claim, LoginRequest::Form { fields })
                            .await
                    });

                    cx.spawn(async move |cx| {
                        let result = task.await;
                        if let Some(entity) = config_entity.upgrade() {
                            entity.update(cx, |config, cx| {
                                config.login_form_view = None;
                                config.local_error = match result {
                                    Ok(()) => None,
                                    Err(e) => Some(e.to_string().into()),
                                };
                                cx.notify();
                            });
                        }
                    })
                    .detach();
                })
                .on_cancel(move |_, _, cx| {
                    if let Some(entity) = config_entity_for_cancel.upgrade() {
                        entity.update(cx, |config, cx| {
                            config.login_form_view = None;
                            cx.notify();
                        });
                    }
                })
                .child(form_entity_child)
        });
    }

    fn on_logout(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(mut claim) = self.component.begin(Operation::Logout) else {
            return;
        };

        let component = self.component.clone();
        cx.spawn_and_update(
            async move {
                if let Some(flow) = component.logout_flow().await.map_err(|e| e.to_string())? {
                    match unblock(move || open_auth_webview(&flow.url, &flow.target)).await {
                        Ok(_) => {}
                        Err(WebviewError::UserCancelled) => return Ok(()),
                        Err(WebviewError::Internal(e)) => return Err(e),
                    }
                }
                component
                    .logout(&mut claim)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, _| {
                this.local_error = result.err().map(Into::into);
            },
        );
        self.refresh_status(cx);
    }

    fn render_header(&mut self, autofocus_back: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let has_login = self.login_method.is_some();
        // Read live rather than cached: a missed refresh event then costs a
        // frame, not a button stuck in its loading state.
        let running = self.component.running();

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
                            .variant_outline()
                            .auto_focus(autofocus_back)
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
                                .when(!self.component.can(Operation::Configure), |btn| {
                                    btn.disabled()
                                })
                                .when(running == Some(Operation::Configure), Button::loading)
                                .size_lg()
                                .child("Save")
                                .w(rems(10.))
                                .on_click(cx.listener(Self::on_save)),
                        )
                    })
                    .when(has_login && self.status.can_login(), |div| {
                        div.child(
                            Button::new("login")
                                .when(!self.component.can(Operation::Login), |btn| btn.disabled())
                                .when(running == Some(Operation::Login), Button::loading)
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
                                .when(!self.component.can(Operation::Logout), |btn| btn.disabled())
                                .when(running == Some(Operation::Logout), Button::loading)
                                .variant_outline()
                                .size_lg()
                                .child("Log Out")
                                .w(rems(10.))
                                .on_click(cx.listener(Self::on_logout)),
                        )
                    }),
            )
    }

    fn render_body(
        &mut self,
        autofocus_field: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let sections = self.sections.iter().map(|s| {
            let fields = s.fields.iter().map(|f| {
                let field_id = f.id.clone();
                input(f.id.clone())
                    .when(!self.component.can(Operation::Configure), |btn| {
                        btn.disabled()
                    })
                    .auto_focus(autofocus_field.as_ref() == Some(&f.id))
                    .reveal_on_focus(&self.scroll_handle)
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
            .flex_grow_1()
            .h_0()
            .gap(rems(1.5))
            .id("config-form")
            .mr(rems(-1.5))
            .pr(rems(1.5))
            .overflow_y_scrollbar()
            .handle(&self.scroll_handle)
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
        let autofocus_field = self
            .component
            .can(Operation::Configure)
            .then(|| self.sections.iter().find_map(|s| s.fields.first()))
            .flatten()
            .map(|f| f.id.clone());

        div()
            .flex_grow_1()
            .pl(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(2.))
            .child(self.render_header(autofocus_field.is_none(), cx))
            .child(self.render_body(autofocus_field, cx))
    }
}

struct LoginFormState {
    fields: Vec<Field>,
    required_ids: Vec<SharedString>,
    values: HashMap<SharedString, SharedString>,
}

impl LoginFormState {
    pub fn new(login_form: LoginForm) -> Self {
        let required_ids = login_form
            .fields
            .iter()
            .filter_map(|f| match f {
                ConfigField::Text {
                    id, required: true, ..
                } => Some(SharedString::from(id.to_string())),
                _ => None,
            })
            .collect();

        let fields = login_form.fields.into_iter().map(Into::into).collect();

        Self {
            fields,
            required_ids,
            values: HashMap::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.required_ids
            .iter()
            .all(|id| self.values.get(id).is_some_and(|v| !v.is_empty()))
    }

    pub fn login_fields(&self) -> HashMap<String, String> {
        self.values
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}

impl Render for LoginFormState {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fields = self.fields.iter().map(|f| {
            let field_id = f.id.clone();
            input(f.id.clone())
                .label(f.label.clone())
                .w(rems(20.))
                .when_some(f.placeholder.clone(), |input, placeholder| {
                    input.placeholder(placeholder)
                })
                .value(self.values.get(f.id.as_str()).cloned().unwrap_or_default())
                .on_input(cx.listener(move |this, event: &InputEvent, _, _| {
                    this.values.insert(field_id.clone(), event.value.clone());
                }))
        });

        div().flex().flex_col().gap(rems(1.5)).children(fields)
    }
}

impl From<ConfigSection> for Section {
    fn from(section: ConfigSection) -> Self {
        Section {
            name: section.name.into(),
            fields: section.fields.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ConfigField> for Field {
    fn from(field: ConfigField) -> Self {
        match field {
            ConfigField::Text {
                id,
                label,
                placeholder,
                required,
            } => Field {
                id: id.into(),
                label: if required {
                    format!("{label} *").into()
                } else {
                    label.into()
                },
                placeholder: placeholder.map(Into::into),
            },
        }
    }
}
