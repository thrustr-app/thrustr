use crate::{
    adapters::ImageExt,
    auth_webview::{WebviewError, open_auth_webview},
    context::{EventListenerExt, SpawnTaskExt},
    navigation::NavigatorExt,
};
use component::{ComponentHandle, Operation, Permit};
use domain::component::{
    AuthFlow, ConfigSection, Element as ConfigElement, LoginForm, LoginMethod, LoginRequest, Status,
};
use event::Topic;
use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, Entity, FontWeight, Image, ImageSource,
    InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle, SharedString, Styled,
    Task, WeakEntity, Window, div, img, prelude::FluentBuilder, relative, rems,
};
use smol::unblock;
use std::{collections::HashMap, sync::Arc};
use theme::ThemeExt;
use ui::{
    Alert, Button, Dialog, Empty, Icon, InputEvent, Label, PortalContext, WithFocus, WithScrollbar,
    WithSize, WithVariant, input,
};

struct Field {
    id: SharedString,
    label: SharedString,
    placeholder: Option<SharedString>,
    required: bool,
}

enum Element {
    Field(Field),
    Hbox(Vec<Field>),
}

impl Element {
    fn fields(&self) -> &[Field] {
        match self {
            Element::Field(field) => std::slice::from_ref(field),
            Element::Hbox(fields) => fields,
        }
    }
}

struct Section {
    name: SharedString,
    elements: Vec<Element>,
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
        let icon = metadata.icon.map(|i| i.to_gpui());

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

        let _tasks = vec![cx.listen(Topic::Component, Self::refresh_status)];

        let status = component.status();
        let mut page = Self {
            name: metadata.name.into(),
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
        let fields = self
            .values
            .iter()
            .map(|(id, value)| (id.to_string(), value.to_string()))
            .collect();

        let component = self.component.clone();
        cx.spawn_and_update(
            async move { component.save_config(fields).await },
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
        let permit = match self.component.begin_login() {
            Ok(permit) => permit,
            Err(err) => {
                self.local_error = Some(err.to_string().into());
                return;
            }
        };

        let component = self.component.clone();
        cx.spawn_and_update(
            async move {
                let result =
                    unblock(move || open_auth_webview(&login_flow.url, &login_flow.target)).await;
                match result {
                    Ok((url, body)) => component
                        .login(permit, LoginRequest::Flow { url, body })
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
        let icon = self.icon.clone();
        let title = SharedString::new(format!("Log in to {}", self.name));

        let form_entity = cx.new(|_| LoginFormState::new(login_form));
        self.login_form_view = Some(form_entity.clone());

        window.open_dialog(cx, move |dialog, _, cx| {
            let form_entity = form_entity.clone();
            let form_entity_child = form_entity.clone();
            let config_entity_for_ok = config_entity.clone();
            let config_entity_for_cancel = config_entity.clone();
            let component = component.clone();
            let title = title.clone();

            let form = form_entity.read(cx);
            let is_valid = form.is_valid();
            let submitting = form.submitting;
            let submit_error = form.submit_error.clone();
            dialog
                .w(rems(24.))
                .header(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(rems(1.))
                        .when_some(icon.clone(), |header, icon| {
                            header.child(img(ImageSource::Image(icon)).size(rems(2.75)))
                        })
                        .child(
                            div()
                                .w_full()
                                .text_center()
                                .text_size(cx.theme().text.lg)
                                .line_height(relative(1.))
                                .font_weight(FontWeight::BOLD)
                                .child(title),
                        ),
                )
                .ok_text("Log In")
                .when(!is_valid, Dialog::disabled)
                .when(submitting, Dialog::loading)
                .when_some(submit_error, Dialog::error)
                .on_ok(move |_, window, cx| {
                    let permit = match component.begin_login() {
                        Ok(permit) => permit,
                        Err(err) => {
                            form_entity.update(cx, |form, cx| {
                                form.submit_error = Some(err.to_string().into());
                                cx.notify();
                            });
                            return;
                        }
                    };

                    let fields = form_entity.read(cx).login_fields();
                    form_entity.update(cx, |form, cx| {
                        form.submitting = true;
                        form.submit_error = None;
                        cx.notify();
                    });

                    submit_login_form(
                        component.clone(),
                        permit,
                        fields,
                        form_entity.clone(),
                        config_entity_for_ok.clone(),
                        window,
                        cx,
                    );
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
        let permit = match self.component.begin_logout() {
            Ok(permit) => permit,
            Err(err) => {
                self.local_error = Some(err.to_string().into());
                return;
            }
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
                component.logout(permit).await.map_err(|e| e.to_string())
            },
            |this, result, _| {
                this.local_error = result.err().map(Into::into);
            },
        );
        self.refresh_status(cx);
    }

    fn is_valid(&self) -> bool {
        let fields = self
            .sections
            .iter()
            .flat_map(|s| &s.elements)
            .flat_map(Element::fields);
        fields_valid(fields, &self.values)
    }

    fn render_header(&mut self, autofocus_back: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let has_login = self.login_method.is_some();

        let status_label = match self.status {
            Status::Initializing => Label::new("INITIALIZING").variant_secondary(),
            Status::Unauthenticated => Label::new("UNAUTHENTICATED").variant_warning(),
            Status::Active => Label::new("ACTIVE").variant_accent(),
            Status::Inactive => Label::new("INACTIVE"),
            Status::Error(_) | Status::InitError(_) => Label::new("ERROR").variant_danger(),
        };

        div()
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .flex()
                    .gap(rems(0.875))
                    .items_center()
                    .text_color(theme.colors.primary)
                    .child(
                        Button::icon("back-button", Icon::arrow())
                            .variant_outline()
                            .size_sm()
                            .auto_focus(autofocus_back)
                            .on_click(|_, _, cx| cx.navigate_back()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(rems(0.5))
                            .font_weight(FontWeight::BOLD)
                            .text_size(rems(1.125))
                            .line_height(relative(1.))
                            .when_some(self.icon.clone(), |div, icon| {
                                div.child(img(ImageSource::Image(icon)).size(rems(1.5)))
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
                                .when(
                                    !self.component.can(Operation::Configure) || !self.is_valid(),
                                    |btn| btn.disabled(),
                                )
                                .when(
                                    self.component.is_running(Operation::Configure),
                                    Button::loading,
                                )
                                .size_md()
                                .variant_outline()
                                .child("Save")
                                .w(rems(10.))
                                .on_click(cx.listener(Self::on_save)),
                        )
                    })
                    .when(has_login && self.status.can_login(), |div| {
                        div.child(
                            Button::new("login")
                                .when(!self.component.can(Operation::Login), |btn| btn.disabled())
                                .when(self.component.is_running(Operation::Login), Button::loading)
                                .variant_accent()
                                .size_md()
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
                                .when(
                                    self.component.is_running(Operation::Logout),
                                    Button::loading,
                                )
                                .variant_outline()
                                .size_md()
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
        let theme = cx.theme();
        let can_configure = self.component.can(Operation::Configure);

        let sections = self.sections.iter().map(|s| {
            let elements = s.elements.iter().map(|element| match element {
                Element::Field(f) => render_field(
                    f,
                    &self.values,
                    can_configure,
                    autofocus_field.as_ref(),
                    &self.scroll_handle,
                    cx,
                ),
                Element::Hbox(fields) => div()
                    .flex()
                    .flex_wrap()
                    .gap(rems(1.))
                    .children(fields.iter().map(|f| {
                        render_field(
                            f,
                            &self.values,
                            can_configure,
                            autofocus_field.as_ref(),
                            &self.scroll_handle,
                            cx,
                        )
                    }))
                    .into_any_element(),
            });

            div()
                .flex()
                .flex_col()
                .text_size(rems(0.875))
                .line_height(relative(1.))
                .font_weight(FontWeight::BOLD)
                .text_color(theme.colors.tertiary)
                .gap(rems(0.875))
                .child(s.name.clone())
                .child(div().flex().flex_col().gap(rems(1.5)).children(elements))
        });

        div()
            .flex()
            .flex_col()
            .flex_grow_1()
            .min_w_0()
            .h_0()
            .gap(rems(1.5))
            .id("config-form")
            .overflow_y_scrollbar()
            .handle(&self.scroll_handle)
            .when_some(self.local_error.clone(), |div, error| {
                div.child(Alert::new(error))
            })
            .when_some(self.status_error.clone(), |div, error| {
                div.child(Alert::new(error))
            })
            .when(self.sections.is_empty(), |div| {
                div.child(Empty::new("This plugin has no configuration options."))
            })
            .children(sections)
    }
}

fn submit_login_form(
    component: ComponentHandle,
    permit: Permit,
    fields: HashMap<String, String>,
    form_entity: Entity<LoginFormState>,
    config_entity: WeakEntity<Config>,
    window: &mut Window,
    cx: &mut App,
) {
    let window_handle = window.window_handle();
    let task =
        cx.background_spawn(
            async move { component.login(permit, LoginRequest::Form { fields }).await },
        );

    cx.spawn(async move |cx| {
        let result = task.await;

        let _ = cx.update_window(window_handle, move |_, window, cx| match result {
            Ok(()) => {
                if let Some(entity) = config_entity.upgrade() {
                    entity.update(cx, |config, cx| {
                        config.login_form_view = None;
                        cx.notify();
                    });
                }
                window.close_dialog(cx);
            }
            Err(err) => {
                form_entity.update(cx, |form, cx| {
                    form.submitting = false;
                    form.submit_error = Some(err.to_string().into());
                    cx.notify();
                });
            }
        });
    })
    .detach();
}

fn render_field(
    field: &Field,
    values: &HashMap<SharedString, SharedString>,
    can_configure: bool,
    autofocus_field: Option<&SharedString>,
    scroll_handle: &ScrollHandle,
    cx: &mut Context<Config>,
) -> AnyElement {
    let field_id = field.id.clone();
    input(field.id.clone())
        .when(!can_configure, |this| this.disabled())
        .auto_focus(autofocus_field == Some(&field.id))
        .reveal_on_focus(scroll_handle)
        .label(field.label.clone())
        .w(rems(20.))
        .when_some(field.placeholder.clone(), |input, placeholder| {
            input.placeholder(placeholder)
        })
        .value(values.get(field.id.as_str()).cloned().unwrap_or_default())
        .on_input(cx.listener(move |config, event: &InputEvent, _, _| {
            config.values.insert(field_id.clone(), event.value.clone());
        }))
        .into_any_element()
}

impl Render for Config {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let autofocus_field = self
            .component
            .can(Operation::Configure)
            .then(|| {
                self.sections.iter().find_map(|s| {
                    s.elements.iter().find_map(|e| match e {
                        Element::Field(f) => Some(f),
                        Element::Hbox(fields) => fields.first(),
                    })
                })
            })
            .flatten()
            .map(|f| f.id.clone());

        div()
            .flex_grow_1()
            .flex()
            .flex_col()
            .gap(rems(2.))
            .child(self.render_header(autofocus_field.is_none(), cx))
            .child(self.render_body(autofocus_field, cx))
    }
}

struct LoginFormState {
    fields: Vec<Field>,
    values: HashMap<SharedString, SharedString>,
    submitting: bool,
    submit_error: Option<SharedString>,
}

impl LoginFormState {
    pub fn new(login_form: LoginForm) -> Self {
        let fields = flatten_fields(login_form.fields)
            .into_iter()
            .map(text_to_field)
            .collect();

        Self {
            fields,
            values: HashMap::new(),
            submitting: false,
            submit_error: None,
        }
    }

    pub fn is_valid(&self) -> bool {
        fields_valid(&self.fields, &self.values)
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
        let fields = self.fields.iter().enumerate().map(|(i, f)| {
            let field_id = f.id.clone();
            input(f.id.clone())
                .size_lg()
                .label(f.label.clone())
                .w_full()
                .auto_focus(i == 0)
                .when(self.submitting, |field| field.disabled())
                .when_some(f.placeholder.clone(), |input, placeholder| {
                    input.placeholder(placeholder)
                })
                .value(self.values.get(f.id.as_str()).cloned().unwrap_or_default())
                .on_input(cx.listener(move |this, event: &InputEvent, _, _| {
                    this.values.insert(field_id.clone(), event.value.clone());
                }))
        });

        div().flex().flex_col().gap(rems(1.)).children(fields)
    }
}

impl From<ConfigSection> for Section {
    fn from(section: ConfigSection) -> Self {
        Section {
            name: section.name.to_uppercase().into(),
            elements: section.elements.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ConfigElement> for Element {
    fn from(element: ConfigElement) -> Self {
        match element {
            text @ ConfigElement::Text { .. } => Element::Field(text_to_field(text)),
            ConfigElement::Hbox { elements } => Element::Hbox(
                flatten_fields(elements)
                    .into_iter()
                    .map(text_to_field)
                    .collect(),
            ),
        }
    }
}

fn flatten_fields(elements: Vec<ConfigElement>) -> Vec<ConfigElement> {
    elements
        .into_iter()
        .flat_map(|element| match element {
            ConfigElement::Hbox { elements } => flatten_fields(elements),
            text => vec![text],
        })
        .collect()
}

fn text_to_field(element: ConfigElement) -> Field {
    match element {
        ConfigElement::Text {
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
            required,
        },
        ConfigElement::Hbox { .. } => unreachable!("flatten_fields removes Hbox"),
    }
}

fn fields_valid<'a>(
    fields: impl IntoIterator<Item = &'a Field>,
    values: &HashMap<SharedString, SharedString>,
) -> bool {
    fields
        .into_iter()
        .filter(|field| field.required)
        .all(|field| values.get(&field.id).is_some_and(|value| !value.is_empty()))
}
