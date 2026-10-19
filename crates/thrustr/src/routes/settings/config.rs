use super::status_label;
use crate::{
    auth_webview::open_auth_webview,
    context::{EventListenerExt, SpawnTaskExt},
    globals::ComponentRegistryExt,
    navigation::NavigatorExt,
};
use component::{AuthHandle, ComponentHandle, ConfigHandle};
use domain::component::{
    AuthOperation, ComboboxField, ConfigOperation, ConfigSection, Field as FormField, FieldValue,
    Form, FormElement, LoginFlow, LoginForm, LoginMethod, LoginRequest, Operation, Status,
};
use event::Topic;
use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, Div, Entity, FontWeight, Image, ImageSource,
    InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle, SharedString,
    StyleRefinement, Styled, Subscription, Task, Window, div, img, prelude::FluentBuilder,
    relative, rems,
};
use std::{collections::HashMap, sync::Arc};
use theme::ThemeExt;
use ui::{
    Alert, Button, Combobox, ComboboxEvent, ComboboxState, Dialog, Empty, FieldProps, FocusProps,
    Icon, Input, InputEvent, InputState, PortalContext, SelectionMode, Size, WithField, WithFocus,
    WithRadius, WithScrollbar, WithSize, WithVariant,
};

type Values = HashMap<String, FieldValue>;

struct Field {
    id: SharedString,
    label: SharedString,
    description: Option<SharedString>,
    control: Control,
}

enum Control {
    Input(Entity<InputState>),
    Combobox(Entity<ComboboxState>, Arc<ComboboxField>),
}

impl Field {
    fn new(
        field: FormField,
        value: Option<&FieldValue>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let placeholder = SharedString::from(field.placeholder().unwrap_or_default().to_owned());
        let control = match field {
            FormField::Text(_) => {
                let text = value.and_then(FieldValue::as_text).unwrap_or_default();
                Control::Input(cx.new(|cx| {
                    InputState::new(window, cx)
                        .with_value(SharedString::from(text.to_owned()))
                        .with_placeholder(placeholder)
                }))
            }
            FormField::Combobox(field) => {
                let mode = match field.is_multiple() {
                    true => SelectionMode::Multiple,
                    false => SelectionMode::Single,
                };
                let values = value.cloned().map(FieldValue::into_list);
                let selected: Vec<SharedString> = values
                    .iter()
                    .flatten()
                    .filter_map(|value| field.option(value))
                    .map(|option| option.label.clone().into())
                    .collect();
                let state = cx.new(|cx| {
                    ComboboxState::new_with_mode(mode, window, cx)
                        .with_items(field.options().iter().map(|option| option.label.clone()))
                        .with_values(selected)
                        .with_placeholder(placeholder)
                });
                Control::Combobox(state, Arc::new(field.clone()))
            }
        };

        Field {
            label: if field.required() {
                format!("{} *", field.label()).into()
            } else {
                field.label().to_owned().into()
            },
            id: field.id().to_owned().into(),
            description: field.description().map(|text| text.to_owned().into()),
            control,
        }
    }

    fn control(&self) -> FieldControl {
        let control = match &self.control {
            Control::Input(state) => FieldControl::Input(Input::new(state)),
            Control::Combobox(state, _) => FieldControl::Combobox(Combobox::new(state)),
        };
        control
            .label(self.label.clone())
            .when_some(self.description.clone(), WithField::description)
    }

    fn bind<T: 'static>(
        &self,
        values_mut: fn(&mut T) -> &mut Values,
        cx: &mut Context<T>,
    ) -> Subscription {
        let id = self.id.clone();
        match &self.control {
            Control::Input(state) => cx.subscribe(state, move |this, state, event, cx| {
                if *event == InputEvent::Input {
                    let value = FieldValue::Text(state.read(cx).value().to_string());
                    values_mut(this).insert(id.to_string(), value);
                    cx.notify();
                }
            }),
            Control::Combobox(state, field) => {
                let field = field.clone();
                cx.subscribe(state, move |this, state, event, cx| match event {
                    ComboboxEvent::Change => {
                        let value = field.value(
                            state
                                .read(cx)
                                .values()
                                .iter()
                                .filter_map(|label| field.option_by_label(label))
                                .map(|option| option.value.clone()),
                        );
                        values_mut(this).insert(id.to_string(), value);
                        cx.notify();
                    }
                })
            }
        }
    }
}

#[allow(clippy::large_enum_variant)]
enum FieldControl {
    Input(Input),
    Combobox(Combobox),
}

impl FieldControl {
    fn disabled(self) -> Self {
        match self {
            Self::Input(input) => Self::Input(input.disabled()),
            Self::Combobox(combobox) => Self::Combobox(combobox.disabled()),
        }
    }
}

impl Styled for FieldControl {
    fn style(&mut self) -> &mut StyleRefinement {
        match self {
            Self::Input(input) => input.style(),
            Self::Combobox(combobox) => combobox.style(),
        }
    }
}

impl WithSize for FieldControl {
    fn size(self, size: Size) -> Self {
        match self {
            Self::Input(input) => Self::Input(WithSize::size(input, size)),
            Self::Combobox(combobox) => Self::Combobox(WithSize::size(combobox, size)),
        }
    }
}

impl WithFocus for FieldControl {
    fn focus_props(&mut self) -> &mut FocusProps {
        match self {
            Self::Input(input) => input.focus_props(),
            Self::Combobox(combobox) => combobox.focus_props(),
        }
    }
}

impl WithField for FieldControl {
    fn field_props(&mut self) -> &mut FieldProps {
        match self {
            Self::Input(input) => input.field_props(),
            Self::Combobox(combobox) => combobox.field_props(),
        }
    }
}

impl IntoElement for FieldControl {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        match self {
            Self::Input(input) => input.into_any_element(),
            Self::Combobox(combobox) => combobox.into_any_element(),
        }
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
    local_error: Option<SharedString>,
    status_error: Option<SharedString>,
    scroll_handle: ScrollHandle,
    _tasks: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Config {
    pub fn new(component: ComponentHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            Ok(values) => (values, None),
            Err(err) => (HashMap::new(), Some(err.to_string().into())),
        };
        let values_loaded = local_error.is_none();

        let sections: Vec<Section> = config
            .iter()
            .flat_map(|c| c.schema().sections())
            .map(|section| Section::new(section, &values, window, cx))
            .collect();
        let _subscriptions = sections
            .iter()
            .flat_map(|s| &s.elements)
            .flat_map(Element::fields)
            .map(|field| field.bind(|this: &mut Self| &mut this.values, cx))
            .collect();

        let _tasks = vec![cx.listen_any(
            &[Topic::ComponentRegistered, Topic::ComponentState],
            Self::refresh_status_error,
        )];

        let status_error = status_error(&component.status());
        let mut page = Self {
            name: component.metadata().name.into(),
            icon,
            component,
            auth,
            config,
            sections,
            values,
            values_loaded,
            local_error,
            status_error,
            scroll_handle: ScrollHandle::new(),
            _tasks,
            _subscriptions,
        };

        page.load_login_method(cx);
        page
    }

    fn refresh_status_error(&mut self, cx: &mut Context<Self>) {
        self.status_error = status_error(&self.component.status());
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
        let fields = self.values.clone();
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

    fn login_with_flow(&mut self, auth: AuthHandle, login_flow: LoginFlow, cx: &mut Context<Self>) {
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
        let login_dialog = cx.new(|cx| {
            LoginDialog::new(auth, login_form, &self.name, self.icon.clone(), window, cx)
        });
        window.open_dialog(cx, move |dialog, _, cx| {
            LoginDialog::build(&login_dialog, dialog, cx)
        });
    }

    fn on_logout(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(auth) = self.auth.as_ref().map(|a| a.handle.clone()) else {
            return;
        };
        cx.spawn_and_update(async move { auth.logout().await }, |this, result, _| {
            this.local_error = result.err().map(|e| e.to_string().into());
        })
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
            .radius_pill()
            .w(rems(10.))
            .child(label)
    }

    fn render_header(
        &self,
        status: &Status,
        can_configure: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let login_ready = self.auth.as_ref().is_some_and(|auth| auth.method.is_some());

        div()
            .flex()
            .justify_between()
            .items_center()
            .min_h(rems(2.375))
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
                            .radius_pill()
                            .on_click(|_, _, cx| cx.navigate_back()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(rems(0.5))
                            .font_weight(FontWeight::BOLD)
                            .text_size(theme.text.lg)
                            .line_height(relative(1.))
                            .when_some(self.icon.clone(), |div, icon| {
                                div.child(img(ImageSource::Image(icon)).size(rems(1.5)))
                            })
                            .child(self.name.clone())
                            .child(status_label(status).filled()),
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
        let theme = cx.theme();
        let elements = section.elements.iter().map(|element| match element {
            Element::Field(f) => self.render_field(f, can_configure, autofocus_field),
            Element::Hbox(fields) => div()
                .flex()
                .flex_wrap()
                .gap(rems(1.))
                .children(
                    fields
                        .iter()
                        .map(|f| self.render_field(f, can_configure, autofocus_field)),
                )
                .into_any_element(),
        });

        div()
            .flex()
            .flex_col()
            .text_size(theme.text.md)
            .line_height(relative(1.))
            .font_weight(FontWeight::BOLD)
            .text_color(theme.colors.secondary)
            .gap(rems(0.875))
            .child(section.name.clone())
            .child(div().flex().flex_col().gap(rems(1.5)).children(elements))
    }

    fn render_field(
        &self,
        field: &Field,
        can_configure: bool,
        autofocus_field: Option<&SharedString>,
    ) -> AnyElement {
        field
            .control()
            .w(rems(20.))
            .when(!can_configure, FieldControl::disabled)
            .when(autofocus_field == Some(&field.id), |this| this.auto_focus())
            .reveal_on_focus(&self.scroll_handle)
            .into_any_element()
    }
}

impl Render for Config {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self.component.status();
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
            .child(self.render_header(&status, can_configure, cx))
            .child(self.render_body(can_configure, autofocus_field, cx))
    }
}

fn status_error(status: &Status) -> Option<SharedString> {
    status.error().map(|error| error.to_string().into())
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
    _subscriptions: Vec<Subscription>,
}

impl LoginDialog {
    fn new(
        auth: AuthHandle,
        form: LoginForm,
        name: &str,
        icon: Option<Arc<Image>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let fields: Vec<Field> = form
            .fields()
            .map(|field| Field::new(field, None, window, cx))
            .collect();
        let _subscriptions = fields
            .iter()
            .map(|field| field.bind(|this: &mut Self| &mut this.values, cx))
            .collect();

        Self {
            auth,
            form,
            title: format!("Log in to {name}").into(),
            icon,
            fields,
            values: Values::new(),
            submitting: false,
            submit_error: None,
            _subscriptions,
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

        let fields = self.values.clone();
        self.submitting = true;
        self.submit_error = None;

        cx.notify();

        cx.spawn_and_update_in(
            window,
            async move { permit.login(LoginRequest::Form { fields }).await },
            |this, result, window, cx| match result {
                Ok(()) => window.close_dialog(cx),
                Err(err) => {
                    this.submitting = false;
                    this.submit_error = Some(err.to_string().into());
                }
            },
        )
        .detach();
    }
}

impl Render for LoginDialog {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let fields = self.fields.iter().enumerate().map(|(i, f)| {
            f.control()
                .size_lg()
                .w_full()
                .when(i == 0, |this| this.auto_focus())
                .when(self.submitting, FieldControl::disabled)
        });

        div().flex().flex_col().gap(rems(1.)).children(fields)
    }
}

impl Section {
    fn new(section: &ConfigSection, values: &Values, window: &mut Window, cx: &mut App) -> Self {
        Section {
            name: section.name.to_uppercase().into(),
            elements: section
                .elements
                .iter()
                .map(|element| Element::new(element, values, window, cx))
                .collect(),
        }
    }
}

impl Element {
    fn new(element: &FormElement, values: &Values, window: &mut Window, cx: &mut App) -> Self {
        let mut fields = element
            .fields()
            .map(|field| Field::new(field, values.get(field.id()), window, cx));
        match element {
            FormElement::Hbox { .. } => Element::Hbox(fields.collect()),
            FormElement::Text(_) | FormElement::Combobox(_) => {
                Element::Field(fields.next().expect("a field element has one field"))
            }
        }
    }
}
