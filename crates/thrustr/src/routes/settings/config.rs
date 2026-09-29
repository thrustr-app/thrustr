use super::status_label;
use crate::{
    auth_webview::open_auth_webview,
    context::{EventListenerExt, SpawnTaskExt},
    globals::ComponentRegistryExt,
    navigation::NavigatorExt,
};
use component::{AuthHandle, ComponentHandle, ConfigHandle};
use domain::component::{
    AuthFlow, AuthOperation, ConfigOperation, ConfigSection, Form, FormElement, LoginForm,
    LoginMethod, LoginRequest, Operation, Status, TextField,
};
use event::Topic;
use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, Div, Entity, FontWeight, Image, ImageSource,
    InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle, SharedString, Styled,
    Task, Window, div, img, prelude::FluentBuilder, relative, rems,
};
use std::{collections::HashMap, sync::Arc};
use theme::ThemeExt;
use ui::{
    Alert, Button, Dialog, Empty, Icon, Input, InputEvent, PortalContext, WithFocus, WithScrollbar,
    WithSize, WithVariant, input,
};

type Values = HashMap<SharedString, SharedString>;

struct Field {
    id: SharedString,
    label: SharedString,
    placeholder: Option<SharedString>,
}

impl Field {
    fn input<T: 'static>(
        &self,
        values: &Values,
        values_mut: fn(&mut T) -> &mut Values,
        cx: &mut Context<T>,
    ) -> Input {
        let id = self.id.clone();
        input(self.id.clone())
            .label(self.label.clone())
            .when_some(self.placeholder.clone(), Input::placeholder)
            .value(values.get(&self.id).cloned().unwrap_or_default())
            .on_input(cx.listener(move |this, event: &InputEvent, _, cx| {
                values_mut(this).insert(id.clone(), event.value.clone());
                cx.notify();
            }))
    }
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

struct AuthState {
    handle: AuthHandle,
    method: Option<LoginMethod>,
}

pub struct Config {
    name: SharedString,
    icon: Option<Arc<Image>>,
    component: ComponentHandle,
    auth: Option<AuthState>,
    config: Option<ConfigHandle>,
    sections: Vec<Section>,
    values: Values,
    values_loaded: bool,
    status: Status,
    local_error: Option<SharedString>,
    status_error: Option<SharedString>,
    scroll_handle: ScrollHandle,
    _tasks: Vec<Task<()>>,
}

impl Config {
    pub fn new(component: ComponentHandle, cx: &mut Context<Self>) -> Self {
        let icon = cx.component_icon(component.id());

        let auth = component.auth().map(|handle| AuthState {
            handle,
            method: None,
        });
        let config = component.config();

        let (values, local_error) = match config
            .as_ref()
            .map_or(Ok(HashMap::new()), ConfigHandle::values)
        {
            Ok(values) => (
                values
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
                None,
            ),
            Err(err) => (HashMap::new(), Some(err.to_string().into())),
        };
        let values_loaded = local_error.is_none();

        let sections = config
            .iter()
            .flat_map(|c| &c.schema().sections)
            .map(Into::into)
            .collect();

        let _tasks = vec![cx.listen(Topic::Component, Self::refresh_status)];

        let status = component.status();
        let mut page = Self {
            name: component.metadata().name.into(),
            icon,
            status_error: status_error(&status),
            status,
            component,
            auth,
            config,
            sections,
            values,
            values_loaded,
            local_error,
            scroll_handle: ScrollHandle::new(),
            _tasks,
        };

        page.load_login_method(cx);
        page
    }

    fn refresh_status(&mut self, cx: &mut Context<Self>) {
        let status = self.component.status();
        self.status_error = status_error(&status);
        self.status = status;
        cx.notify();
    }

    fn load_login_method(&mut self, cx: &mut Context<Self>) {
        let Some(auth) = self.auth.as_ref().map(|a| a.handle.clone()) else {
            return;
        };
        let task = cx.spawn_and_update(
            async move { auth.login_method().await },
            |this, result, _| match result {
                Ok(method) => {
                    if let Some(auth) = &mut this.auth {
                        auth.method = Some(method);
                    }
                }
                Err(err) => this.local_error = Some(err.to_string().into()),
            },
        );
        self._tasks.push(task);
    }

    fn on_save(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.config.clone() else {
            return;
        };
        let fields = to_owned_values(&self.values);
        cx.spawn_and_update(
            async move { config.save(fields).await },
            |this, result, _| {
                this.local_error = result.err().map(|e| e.to_string().into());
            },
        )
        .detach();
    }

    fn on_login(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AuthState {
            handle,
            method: Some(method),
        }) = &self.auth
        else {
            return;
        };
        let (auth, method) = (handle.clone(), method.clone());
        match method {
            LoginMethod::Flow(login_flow) => self.login_with_flow(auth, login_flow, cx),
            LoginMethod::Form(login_form) => self.open_login_dialog(auth, login_form, window, cx),
        }
    }

    fn login_with_flow(&mut self, auth: AuthHandle, login_flow: AuthFlow, cx: &mut Context<Self>) {
        let permit = match auth.begin_login() {
            Ok(permit) => permit,
            Err(err) => {
                self.local_error = Some(err.to_string().into());
                cx.notify();
                return;
            }
        };

        cx.spawn_and_update(
            async move {
                let Some((url, body)) = open_auth_webview(login_flow).await? else {
                    return Ok(());
                };
                permit.login(LoginRequest::Flow { url, body }).await?;
                anyhow::Ok(())
            },
            |this, result, _| {
                this.local_error = result.err().map(|e| e.to_string().into());
            },
        )
        .detach();
    }

    fn open_login_dialog(
        &mut self,
        auth: AuthHandle,
        login_form: LoginForm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let login_dialog =
            cx.new(|_| LoginDialog::new(auth, login_form, &self.name, self.icon.clone()));
        window.open_dialog(cx, move |dialog, _, cx| {
            LoginDialog::build(&login_dialog, dialog, cx)
        });
    }

    fn on_logout(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(auth) = self.auth.as_ref().map(|a| a.handle.clone()) else {
            return;
        };
        let permit = match auth.begin_logout() {
            Ok(permit) => permit,
            Err(err) => {
                self.local_error = Some(err.to_string().into());
                cx.notify();
                return;
            }
        };

        cx.spawn_and_update(
            async move {
                if let Some(flow) = auth.logout_flow().await?
                    && open_auth_webview(flow).await?.is_none()
                {
                    return Ok(());
                }
                permit.logout().await?;
                anyhow::Ok(())
            },
            |this, result, _| {
                this.local_error = result.err().map(|e| e.to_string().into());
            },
        )
        .detach();
    }

    fn is_valid(&self) -> bool {
        self.config
            .as_ref()
            .is_none_or(|config| config.schema().missing_field(&self.values).is_none())
    }

    fn operation_button(
        &self,
        id: &'static str,
        label: &'static str,
        operation: impl Into<Operation>,
        enabled: bool,
    ) -> Button {
        Button::new(id)
            .when(!enabled, Button::disabled)
            .when(self.component.is_running(operation), Button::loading)
            .size_md()
            .w(rems(10.))
            .child(label)
    }

    fn render_header(
        &self,
        can_configure: bool,
        autofocus_back: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let login_ready = self.auth.as_ref().is_some_and(|auth| auth.method.is_some());

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
                            .when(autofocus_back, |this| this.auto_focus())
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
                            .child(status_label(&self.status).filled()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(1.))
                    .when(!self.sections.is_empty(), |div| {
                        div.child(
                            self.operation_button(
                                "save",
                                "Save",
                                ConfigOperation::Save,
                                can_configure && self.is_valid(),
                            )
                            .variant_outline()
                            .on_click(cx.listener(Self::on_save)),
                        )
                    })
                    .when(self.component.allows(AuthOperation::Login), |div| {
                        div.child(
                            self.operation_button(
                                "login",
                                "Log In",
                                AuthOperation::Login,
                                login_ready && self.component.can(AuthOperation::Login),
                            )
                            .variant_accent()
                            .on_click(cx.listener(Self::on_login)),
                        )
                    })
                    .when(self.component.allows(AuthOperation::Logout), |div| {
                        div.child(
                            self.operation_button(
                                "logout",
                                "Log Out",
                                AuthOperation::Logout,
                                self.component.can(AuthOperation::Logout),
                            )
                            .variant_outline()
                            .on_click(cx.listener(Self::on_logout)),
                        )
                    }),
            )
    }

    fn render_body(
        &self,
        can_configure: bool,
        autofocus_field: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let sections = self.sections.iter().map(|section| {
            self.render_section(section, can_configure, autofocus_field.as_ref(), cx)
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

    fn render_section(
        &self,
        section: &Section,
        can_configure: bool,
        autofocus_field: Option<&SharedString>,
        cx: &mut Context<Self>,
    ) -> Div {
        let text_color = cx.theme().colors.secondary;
        let elements = section.elements.iter().map(|element| match element {
            Element::Field(f) => self.render_field(f, can_configure, autofocus_field, cx),
            Element::Hbox(fields) => div()
                .flex()
                .flex_wrap()
                .gap(rems(1.))
                .children(
                    fields
                        .iter()
                        .map(|f| self.render_field(f, can_configure, autofocus_field, cx)),
                )
                .into_any_element(),
        });

        div()
            .flex()
            .flex_col()
            .text_size(rems(0.875))
            .line_height(relative(1.))
            .font_weight(FontWeight::BOLD)
            .text_color(text_color)
            .gap(rems(0.875))
            .child(section.name.clone())
            .child(div().flex().flex_col().gap(rems(1.5)).children(elements))
    }

    fn render_field(
        &self,
        field: &Field,
        can_configure: bool,
        autofocus_field: Option<&SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        field
            .input(&self.values, |this| &mut this.values, cx)
            .w(rems(20.))
            .when(!can_configure, Input::disabled)
            .when(autofocus_field == Some(&field.id), |this| this.auto_focus())
            .reveal_on_focus(&self.scroll_handle)
            .into_any_element()
    }
}

impl Render for Config {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let can_configure = self.values_loaded && self.component.can(ConfigOperation::Save);
        let autofocus_field = self
            .sections
            .iter()
            .flat_map(|s| &s.elements)
            .flat_map(Element::fields)
            .next()
            .filter(|_| can_configure)
            .map(|f| f.id.clone());

        div()
            .flex_grow_1()
            .flex()
            .flex_col()
            .gap(rems(2.))
            .child(self.render_header(can_configure, autofocus_field.is_none(), cx))
            .child(self.render_body(can_configure, autofocus_field, cx))
    }
}

fn status_error(status: &Status) -> Option<SharedString> {
    status.error().map(|error| error.to_string().into())
}

fn to_owned_values(values: &Values) -> HashMap<String, String> {
    values
        .iter()
        .map(|(id, value)| (id.to_string(), value.to_string()))
        .collect()
}

struct LoginDialog {
    auth: AuthHandle,
    form: LoginForm,
    title: SharedString,
    icon: Option<Arc<Image>>,
    fields: Vec<Field>,
    values: Values,
    submitting: bool,
    submit_error: Option<SharedString>,
}

impl LoginDialog {
    fn new(auth: AuthHandle, form: LoginForm, name: &str, icon: Option<Arc<Image>>) -> Self {
        let fields = form.text_fields().map(Into::into).collect();

        Self {
            auth,
            form,
            title: format!("Log in to {name}").into(),
            icon,
            fields,
            values: HashMap::new(),
            submitting: false,
            submit_error: None,
        }
    }

    fn build(this: &Entity<Self>, dialog: Dialog, cx: &App) -> Dialog {
        let state = this.read(cx);
        let entity = this.clone();
        dialog
            .w(rems(24.))
            .header(state.render_header(cx))
            .ok_text("Log In")
            .when(!state.is_valid(), Dialog::disabled)
            .when(state.submitting, Dialog::loading)
            .when_some(state.submit_error.clone(), Dialog::error)
            .on_ok(move |_, window, cx| {
                entity.update(cx, |this, cx| this.submit(window, cx));
            })
            .child(this.clone())
    }

    fn render_header(&self, cx: &App) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(rems(1.))
            .when_some(self.icon.clone(), |header, icon| {
                header.child(img(ImageSource::Image(icon)).size(rems(2.75)))
            })
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(cx.theme().text.lg)
                    .line_height(relative(1.))
                    .font_weight(FontWeight::BOLD)
                    .child(self.title.clone()),
            )
    }

    fn is_valid(&self) -> bool {
        self.form.missing_field(&self.values).is_none()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let permit = match self.auth.begin_login() {
            Ok(permit) => permit,
            Err(err) => {
                self.submit_error = Some(err.to_string().into());
                cx.notify();
                return;
            }
        };

        let fields = to_owned_values(&self.values);
        self.submitting = true;
        self.submit_error = None;

        cx.notify();

        let login =
            cx.background_spawn(async move { permit.login(LoginRequest::Form { fields }).await });
        cx.spawn_in(window, async move |this, cx| {
            let result = login.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(()) => window.close_dialog(cx),
                Err(err) => {
                    this.submitting = false;
                    this.submit_error = Some(err.to_string().into());
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for LoginDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fields = self.fields.iter().enumerate().map(|(i, f)| {
            f.input(&self.values, |this| &mut this.values, cx)
                .size_lg()
                .w_full()
                .when(i == 0, |this| this.auto_focus())
                .when(self.submitting, Input::disabled)
        });

        div().flex().flex_col().gap(rems(1.)).children(fields)
    }
}

impl From<&ConfigSection> for Section {
    fn from(section: &ConfigSection) -> Self {
        Section {
            name: section.name.to_uppercase().into(),
            elements: section.elements.iter().map(Into::into).collect(),
        }
    }
}

impl From<&FormElement> for Element {
    fn from(element: &FormElement) -> Self {
        match element {
            FormElement::Text(field) => Element::Field(field.into()),
            FormElement::Hbox { .. } => {
                Element::Hbox(element.text_fields().map(Into::into).collect())
            }
        }
    }
}

impl From<&TextField> for Field {
    fn from(field: &TextField) -> Self {
        Field {
            id: field.id.clone().into(),
            label: if field.required {
                format!("{} *", field.label).into()
            } else {
                field.label.clone().into()
            },
            placeholder: field.placeholder.clone().map(Into::into),
        }
    }
}
