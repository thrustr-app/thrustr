use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    mem,
};

pub trait Form {
    /// Top level elements.
    fn elements(&self) -> impl Iterator<Item = &FormElement>;

    fn fields(&self) -> impl Iterator<Item = Field<'_>> {
        self.elements().flat_map(FormElement::fields)
    }

    fn field(&self, id: &str) -> Option<Field<'_>> {
        self.fields().find(|field| field.id() == id)
    }

    /// Returns the first required field without a value.
    fn missing_field(&self, values: &HashMap<String, FieldValue>) -> Option<Field<'_>> {
        self.fields().find(|field| {
            field.required() && values.get(field.id()).is_none_or(FieldValue::is_empty)
        })
    }

    /// Stored values made valid for the form, with defaults for missing ones.
    fn resolve(&self, mut stored: HashMap<String, FieldValue>) -> HashMap<String, FieldValue> {
        self.fields()
            .filter_map(|field| {
                let (id, value) = stored.remove_entry(field.id()).unzip();
                let value = field.resolve(value)?;
                Some((id.unwrap_or_else(|| field.id().to_owned()), value))
            })
            .collect()
    }

    /// Drops values for undeclared fields and options, and checks required
    /// fields have a value.
    fn retain_valid(
        &self,
        values: &mut HashMap<String, FieldValue>,
    ) -> Result<(), MissingFieldError> {
        let mut given = mem::take(values);
        *values = self
            .fields()
            .filter_map(|field| {
                let (id, value) = given.remove_entry(field.id())?;
                Some((id, field.accept(value)?))
            })
            .collect();

        match self.missing_field(values) {
            Some(field) => Err(MissingFieldError {
                id: field.id().to_owned(),
                label: field.label().to_owned(),
            }),
            None => Ok(()),
        }
    }
}

impl Form for [FormElement] {
    fn elements(&self) -> impl Iterator<Item = &FormElement> {
        self.iter()
    }
}

pub(crate) fn unique_ids(form: &(impl Form + ?Sized)) -> Result<(), InvalidFormError> {
    let mut ids = HashSet::new();
    match form.fields().find(|field| !ids.insert(field.id())) {
        Some(field) => Err(InvalidFormError::DuplicateField(field.id().to_owned())),
        None => Ok(()),
    }
}

#[derive(Clone, Debug)]
pub enum FormElement {
    Text(TextField),
    Combobox(ComboboxField),
    Hbox { elements: Vec<FormElement> },
}

impl FormElement {
    pub fn fields(&self) -> Box<dyn Iterator<Item = Field<'_>> + '_> {
        match self {
            Self::Text(field) => Box::new(std::iter::once(Field::Text(field))),
            Self::Combobox(field) => Box::new(std::iter::once(Field::Combobox(field))),
            Self::Hbox { elements } => Box::new(elements.fields()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Field<'a> {
    Text(&'a TextField),
    Combobox(&'a ComboboxField),
}

impl<'a> Field<'a> {
    pub fn id(self) -> &'a str {
        match self {
            Self::Text(field) => &field.id,
            Self::Combobox(field) => &field.id,
        }
    }

    pub fn label(self) -> &'a str {
        match self {
            Self::Text(field) => &field.label,
            Self::Combobox(field) => &field.label,
        }
    }

    pub fn description(self) -> Option<&'a str> {
        match self {
            Self::Text(field) => field.description.as_deref(),
            Self::Combobox(field) => field.description.as_deref(),
        }
    }

    pub fn placeholder(self) -> Option<&'a str> {
        match self {
            Self::Text(field) => field.placeholder.as_deref(),
            Self::Combobox(field) => field.placeholder.as_deref(),
        }
    }

    pub fn required(self) -> bool {
        match self {
            Self::Text(field) => field.required,
            Self::Combobox(field) => field.required,
        }
    }

    pub fn is_dynamic(self) -> bool {
        matches!(self, Self::Combobox(field) if field.is_dynamic())
    }

    pub fn default_value(self) -> Option<FieldValue> {
        match self {
            Self::Text(field) => field.default.clone().map(FieldValue::Text),
            Self::Combobox(field) => {
                (!field.default.is_empty()).then(|| field.value(field.default.iter().cloned()))
            }
        }
    }

    pub fn empty_value(self) -> FieldValue {
        match self {
            Self::Text(_) => FieldValue::Text(String::new()),
            Self::Combobox(field) => field.value([]),
        }
    }

    /// The stored value made valid for the field, or its default.
    pub fn resolve(self, stored: Option<FieldValue>) -> Option<FieldValue> {
        stored
            .and_then(|value| self.accept(value))
            .or_else(|| self.default_value())
    }

    /// `value` in the shape the field expects, if it has one.
    fn accept(self, value: FieldValue) -> Option<FieldValue> {
        match (self, value) {
            (Self::Text(_), FieldValue::Text(text)) => Some(FieldValue::Text(text)),
            (Self::Text(_), FieldValue::List(_)) => None,
            (Self::Combobox(field), value) => Some(field.value(value.into_list())),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextField {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub placeholder: Option<String>,
    pub required: bool,
    pub default: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ComboboxField {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub placeholder: Option<String>,
    pub required: bool,
    multiple: bool,
    options: ComboboxOptions,
    default: Vec<String>,
}

impl ComboboxField {
    pub fn single(
        id: impl Into<String>,
        label: impl Into<String>,
        options: ComboboxOptions,
        default: Option<String>,
    ) -> Result<Self, InvalidFormError> {
        Self::new(
            id.into(),
            label.into(),
            options,
            false,
            default.into_iter().collect(),
        )
    }

    pub fn multiple(
        id: impl Into<String>,
        label: impl Into<String>,
        options: ComboboxOptions,
        default: Vec<String>,
    ) -> Result<Self, InvalidFormError> {
        Self::new(id.into(), label.into(), options, true, default)
    }

    fn new(
        id: String,
        label: String,
        options: ComboboxOptions,
        multiple: bool,
        default: Vec<String>,
    ) -> Result<Self, InvalidFormError> {
        let field = Self {
            id,
            label,
            description: None,
            placeholder: None,
            required: false,
            multiple,
            options,
            default,
        };
        field.validate()?;
        Ok(field)
    }

    pub fn is_multiple(&self) -> bool {
        self.multiple
    }

    pub fn options(&self) -> &ComboboxOptions {
        &self.options
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self.options, ComboboxOptions::Dynamic)
    }

    /// The field value for `values`, keeping only declared options, or any
    /// non-empty value when options are dynamic. Single mode takes the first
    /// one as text.
    pub fn value(&self, values: impl IntoIterator<Item = String>) -> FieldValue {
        let mut values = values
            .into_iter()
            .filter(|value| self.options.allows(value));
        match self.multiple {
            true => FieldValue::List(values.collect()),
            false => FieldValue::Text(values.next().unwrap_or_default()),
        }
    }

    /// Checks options loaded for a field with dynamic options. Unlike
    /// declared options, labels may repeat.
    pub fn check_dynamic_options(
        &self,
        options: &[ComboboxOption],
    ) -> Result<(), InvalidFormError> {
        validate_values(options).map_err(|kind| self.error(kind))
    }

    fn validate(&self) -> Result<(), InvalidFormError> {
        let result = match &self.options {
            ComboboxOptions::Static(options) => validate_static(options, &self.default),
            ComboboxOptions::Dynamic if self.default.is_empty() => Ok(()),
            ComboboxOptions::Dynamic => Err(InvalidCombobox::DynamicDefault),
        };
        result.map_err(|kind| self.error(kind))
    }

    fn error(&self, kind: InvalidCombobox) -> InvalidFormError {
        InvalidFormError::Combobox {
            id: self.id.clone(),
            kind,
        }
    }
}

fn validate_static(options: &[ComboboxOption], default: &[String]) -> Result<(), InvalidCombobox> {
    validate_values(options)?;
    let mut labels = HashSet::new();
    if let Some(option) = options.iter().find(|option| !labels.insert(&option.label)) {
        return Err(InvalidCombobox::DuplicateLabel(option.label.clone()));
    }
    match default
        .iter()
        .find(|value| !options.iter().any(|option| &option.value == *value))
    {
        Some(value) => Err(InvalidCombobox::UnknownDefault(value.clone())),
        None => Ok(()),
    }
}

fn validate_values(options: &[ComboboxOption]) -> Result<(), InvalidCombobox> {
    let mut values = HashSet::new();
    for option in options {
        if option.value.is_empty() {
            return Err(InvalidCombobox::EmptyOption);
        }
        if !values.insert(&option.value) {
            return Err(InvalidCombobox::DuplicateOption(option.value.clone()));
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub enum ComboboxOptions {
    Static(Vec<ComboboxOption>),
    Dynamic,
}

impl ComboboxOptions {
    /// Whether `value` can be selected.
    fn allows(&self, value: &str) -> bool {
        match self {
            Self::Static(options) => options.iter().any(|option| option.value == value),
            Self::Dynamic => !value.is_empty(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ComboboxOption {
    pub value: String,
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum FieldValue {
    Text(String),
    List(Vec<String>),
}

impl FieldValue {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Text(text) => text.is_empty(),
            Self::List(values) => values.is_empty(),
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::List(_) => None,
        }
    }

    pub fn into_list(self) -> Vec<String> {
        match self {
            Self::Text(text) if text.is_empty() => Vec::new(),
            Self::Text(text) => vec![text],
            Self::List(values) => values,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{label} is required")]
pub struct MissingFieldError {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidFormError {
    #[error("duplicate field `{0}`")]
    DuplicateField(String),
    #[error("field `{0}` cannot have dynamic options")]
    DynamicFieldNotAllowed(String),
    #[error("`{id}` {kind}")]
    Combobox { id: String, kind: InvalidCombobox },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidCombobox {
    #[error("option values must be non-empty")]
    EmptyOption,
    #[error("has duplicate option `{0}`")]
    DuplicateOption(String),
    #[error("has duplicate label `{0}`")]
    DuplicateLabel(String),
    #[error("default `{0}` is not an option")]
    UnknownDefault(String),
    #[error("cannot have defaults when its options are dynamic")]
    DynamicDefault,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn required(id: &str) -> FormElement {
        text(id, true)
    }

    pub(crate) fn optional(id: &str) -> FormElement {
        text(id, false)
    }

    pub(crate) fn t(text: &str) -> FieldValue {
        FieldValue::Text(text.to_owned())
    }

    fn list(values: &[&str]) -> FieldValue {
        FieldValue::List(values.iter().map(|value| value.to_string()).collect())
    }

    fn text(id: &str, required: bool) -> FormElement {
        FormElement::Text(TextField {
            id: id.to_owned(),
            label: id.to_uppercase(),
            description: None,
            placeholder: None,
            required,
            default: None,
        })
    }

    fn option(value: &str, label: &str) -> ComboboxOption {
        ComboboxOption {
            value: value.to_owned(),
            label: label.to_owned(),
        }
    }

    fn options(values: &[&str]) -> ComboboxOptions {
        ComboboxOptions::Static(
            values
                .iter()
                .map(|value| option(value, &value.to_uppercase()))
                .collect(),
        )
    }

    fn single(id: &str, options: ComboboxOptions) -> FormElement {
        let field = ComboboxField::single(id, id.to_uppercase(), options, None);
        FormElement::Combobox(field.unwrap())
    }

    fn multiple(id: &str, options: ComboboxOptions) -> FormElement {
        let field = ComboboxField::multiple(id, id.to_uppercase(), options, vec![]);
        FormElement::Combobox(field.unwrap())
    }

    fn require(mut element: FormElement) -> FormElement {
        let FormElement::Combobox(field) = &mut element else {
            unreachable!()
        };
        field.required = true;
        element
    }

    fn with_default(mut element: FormElement, default: FieldValue) -> FormElement {
        match &mut element {
            FormElement::Text(field) => field.default = Some(default.as_text().unwrap().to_owned()),
            FormElement::Combobox(field) => {
                field.default = default.into_list();
                field.validate().unwrap();
            }
            FormElement::Hbox { .. } => unreachable!(),
        }
        element
    }

    fn hbox<const N: usize>(elements: [FormElement; N]) -> FormElement {
        FormElement::Hbox {
            elements: elements.into(),
        }
    }

    fn owned(values: &[(&str, FieldValue)]) -> HashMap<String, FieldValue> {
        values
            .iter()
            .map(|(id, value)| (id.to_string(), value.clone()))
            .collect()
    }

    fn combobox_error(id: &str, kind: InvalidCombobox) -> InvalidFormError {
        InvalidFormError::Combobox {
            id: id.to_owned(),
            kind,
        }
    }

    #[track_caller]
    fn check(
        elements: &[FormElement],
        values: &[(&str, FieldValue)],
        expected: Result<&[(&str, FieldValue)], &str>,
    ) {
        let mut actual = owned(values);
        match (elements.retain_valid(&mut actual), expected) {
            (Ok(()), Ok(kept)) => assert_eq!(actual, owned(kept)),
            (Err(error), Err(id)) => assert_eq!(error.id, id),
            (actual, expected) => panic!("expected {expected:?}, got {actual:?}"),
        }
    }

    #[track_caller]
    fn check_resolve(
        elements: &[FormElement],
        stored: &[(&str, FieldValue)],
        expected: &[(&str, FieldValue)],
    ) {
        assert_eq!(elements.resolve(owned(stored)), owned(expected));
    }

    #[track_caller]
    fn check_definition(
        options: ComboboxOptions,
        default: &[&str],
        expected: Result<(), InvalidCombobox>,
    ) {
        let default = default.iter().map(|value| value.to_string()).collect();
        let actual = ComboboxField::multiple("k", "K", options, default).map(drop);
        assert_eq!(actual, expected.map_err(|kind| combobox_error("k", kind)));
    }

    #[track_caller]
    fn check_loaded_options(options: &[ComboboxOption], expected: Result<(), InvalidCombobox>) {
        let field = ComboboxField::multiple("k", "K", ComboboxOptions::Dynamic, vec![]).unwrap();
        assert_eq!(
            field.check_dynamic_options(options),
            expected.map_err(|kind| combobox_error("k", kind))
        );
    }

    #[test]
    fn required_fields_must_have_a_value() {
        let elements = [required("user"), optional("region")];

        check(
            &elements,
            &[("user", t("username"))],
            Ok(&[("user", t("username"))]),
        );
        check(&elements, &[("region", t("eu"))], Err("user"));
        check(
            &elements,
            &[("user", t("")), ("region", t("eu"))],
            Err("user"),
        );
    }

    #[test]
    fn optional_fields_may_be_empty_or_missing() {
        let elements = [optional("region")];

        check(&elements, &[], Ok(&[]));
        check(&elements, &[("region", t(""))], Ok(&[("region", t(""))]));
    }

    #[test]
    fn undeclared_values_are_dropped() {
        let elements = [optional("region")];

        check(
            &elements,
            &[("region", t("eu")), ("stale", t("value"))],
            Ok(&[("region", t("eu"))]),
        );
    }

    #[test]
    fn fields_nested_in_hboxes_are_validated() {
        let elements = [
            optional("region"),
            hbox([optional("host"), hbox([required("port")])]),
        ];

        check(
            &elements,
            &[
                ("region", t("eu")),
                ("host", t("localhost")),
                ("port", t("8080")),
            ],
            Ok(&[
                ("region", t("eu")),
                ("host", t("localhost")),
                ("port", t("8080")),
            ]),
        );
        check(&elements, &[("host", t("localhost"))], Err("port"));
    }

    #[test]
    fn text_fields_reject_lists() {
        let elements = [optional("region")];

        check(&elements, &[("region", list(&["eu"]))], Ok(&[]));
    }

    #[test]
    fn combobox_keeps_only_declared_options() {
        let elements = [multiple("kinds", options(&["a", "b", "c"]))];

        check(
            &elements,
            &[("kinds", list(&["a", "x", "c"]))],
            Ok(&[("kinds", list(&["a", "c"]))]),
        );
        check(
            &elements,
            &[("kinds", list(&["x"]))],
            Ok(&[("kinds", list(&[]))]),
        );
    }

    #[test]
    fn combobox_values_depend_on_mode() {
        let single = [single("kind", options(&["a", "b", "c"]))];
        let multiple = [multiple("kinds", options(&["a", "b", "c"]))];

        check(&single, &[("kind", t("b"))], Ok(&[("kind", t("b"))]));
        check(&single, &[("kind", t(""))], Ok(&[("kind", t(""))]));
        check(
            &single,
            &[("kind", list(&["b", "c"]))],
            Ok(&[("kind", t("b"))]),
        );
        check(&single, &[("kind", list(&[]))], Ok(&[("kind", t(""))]));
        check(
            &multiple,
            &[("kinds", t("b"))],
            Ok(&[("kinds", list(&["b"]))]),
        );
        check(&multiple, &[("kinds", t(""))], Ok(&[("kinds", list(&[]))]));
        check(
            &multiple,
            &[("kinds", list(&["b", "c"]))],
            Ok(&[("kinds", list(&["b", "c"]))]),
        );
        check(
            &multiple,
            &[("kinds", list(&[]))],
            Ok(&[("kinds", list(&[]))]),
        );
    }

    #[test]
    fn required_combobox_needs_an_option() {
        let elements = [require(multiple("kinds", options(&["a", "b", "c"])))];

        check(
            &elements,
            &[("kinds", list(&["a"]))],
            Ok(&[("kinds", list(&["a"]))]),
        );
        check(&elements, &[("kinds", list(&["x"]))], Err("kinds"));
    }

    #[test]
    fn defaults_fill_only_missing_values() {
        let elements = [
            with_default(multiple("kinds", options(&["a", "b"])), list(&["a", "b"])),
            with_default(optional("region"), t("eu")),
        ];

        check_resolve(
            &elements,
            &[("region", t("us"))],
            &[("kinds", list(&["a", "b"])), ("region", t("us"))],
        );
        check_resolve(
            &elements,
            &[("kinds", list(&[])), ("stale", t("value"))],
            &[("kinds", list(&[])), ("region", t("eu"))],
        );
    }

    #[test]
    fn field_ids_must_be_unique() {
        let unique = [optional("host"), hbox([required("port")])];
        let duplicate = [optional("port"), hbox([required("port")])];

        assert_eq!(unique_ids(unique.as_slice()), Ok(()));
        assert_eq!(
            unique_ids(duplicate.as_slice()),
            Err(InvalidFormError::DuplicateField("port".to_owned()))
        );
    }

    #[test]
    fn invalid_combobox_definitions_are_rejected() {
        check_definition(options(&["a", "b"]), &["a"], Ok(()));
        check_definition(options(&[""]), &[], Err(InvalidCombobox::EmptyOption));
        check_definition(
            ComboboxOptions::Static(vec![option("a", "A"), option("a", "B")]),
            &[],
            Err(InvalidCombobox::DuplicateOption("a".to_owned())),
        );
        check_definition(
            ComboboxOptions::Static(vec![option("a", "A"), option("b", "A")]),
            &[],
            Err(InvalidCombobox::DuplicateLabel("A".to_owned())),
        );
        check_definition(
            options(&["a", "b"]),
            &["x"],
            Err(InvalidCombobox::UnknownDefault("x".to_owned())),
        );
    }

    #[test]
    fn dynamic_combobox_keeps_any_value() {
        let single = [require(single("kind", ComboboxOptions::Dynamic))];
        let multiple = [multiple("kinds", ComboboxOptions::Dynamic)];

        check(
            &multiple,
            &[("kinds", list(&["x", "", "y"]))],
            Ok(&[("kinds", list(&["x", "y"]))]),
        );
        check(
            &single,
            &[("kind", list(&["", "x"]))],
            Ok(&[("kind", t("x"))]),
        );
        check(&single, &[("kind", t(""))], Err("kind"));
    }

    #[test]
    fn dynamic_combobox_has_no_defaults() {
        check_definition(ComboboxOptions::Dynamic, &[], Ok(()));
        check_definition(
            ComboboxOptions::Dynamic,
            &["a"],
            Err(InvalidCombobox::DynamicDefault),
        );
    }

    #[test]
    fn loaded_options_need_unique_values() {
        check_loaded_options(&[option("a", "A"), option("b", "A")], Ok(()));
        check_loaded_options(
            &[option("a", "A"), option("a", "B")],
            Err(InvalidCombobox::DuplicateOption("a".to_owned())),
        );
        check_loaded_options(&[option("", "A")], Err(InvalidCombobox::EmptyOption));
    }
}
