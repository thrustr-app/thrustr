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
    options: Vec<ComboboxOption>,
    default: Vec<String>,
}

impl ComboboxField {
    pub fn single(
        id: impl Into<String>,
        label: impl Into<String>,
        options: Vec<ComboboxOption>,
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
        options: Vec<ComboboxOption>,
        default: Vec<String>,
    ) -> Result<Self, InvalidFormError> {
        Self::new(id.into(), label.into(), options, true, default)
    }

    fn new(
        id: String,
        label: String,
        options: Vec<ComboboxOption>,
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

    pub fn options(&self) -> &[ComboboxOption] {
        &self.options
    }

    pub fn option(&self, value: &str) -> Option<&ComboboxOption> {
        self.options.iter().find(|option| option.value == value)
    }

    pub fn option_by_label(&self, label: &str) -> Option<&ComboboxOption> {
        self.options.iter().find(|option| option.label == label)
    }

    /// The field value for `values`, keeping only declared options. Single
    /// mode takes the first one as text.
    pub fn value(&self, values: impl IntoIterator<Item = String>) -> FieldValue {
        let mut values = values
            .into_iter()
            .filter(|value| self.option(value).is_some());
        match self.multiple {
            true => FieldValue::List(values.collect()),
            false => FieldValue::Text(values.next().unwrap_or_default()),
        }
    }

    fn validate(&self) -> Result<(), InvalidFormError> {
        let error = |kind| InvalidFormError::Combobox {
            id: self.id.clone(),
            kind,
        };
        let mut values = HashSet::new();
        let mut labels = HashSet::new();
        for option in &self.options {
            if option.value.is_empty() {
                return Err(error(InvalidCombobox::EmptyOption));
            }
            if !values.insert(&option.value) {
                return Err(error(InvalidCombobox::DuplicateOption(
                    option.value.clone(),
                )));
            }
            if !labels.insert(&option.label) {
                return Err(error(InvalidCombobox::DuplicateLabel(option.label.clone())));
            }
        }
        if let Some(value) = self
            .default
            .iter()
            .find(|value| self.option(value).is_none())
        {
            return Err(error(InvalidCombobox::UnknownDefault(value.clone())));
        }
        Ok(())
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

    fn options(values: &[&str]) -> Vec<ComboboxOption> {
        values
            .iter()
            .map(|value| ComboboxOption {
                value: value.to_string(),
                label: value.to_uppercase(),
            })
            .collect()
    }

    fn combobox(id: &str, multiple: bool, required: bool) -> FormElement {
        let options = options(&["a", "b", "c"]);
        let label = id.to_uppercase();
        let mut field = match multiple {
            true => ComboboxField::multiple(id, label, options, vec![]),
            false => ComboboxField::single(id, label, options, None),
        }
        .unwrap();
        field.required = required;
        FormElement::Combobox(field)
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
    fn nested_fields_are_flattened() {
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
        let elements = [combobox("kinds", true, false)];

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
        let single = [combobox("kind", false, false)];
        let multiple = [combobox("kinds", true, false)];

        check(
            &single,
            &[("kind", list(&["b", "c"]))],
            Ok(&[("kind", t("b"))]),
        );
        check(
            &multiple,
            &[("kinds", t("b"))],
            Ok(&[("kinds", list(&["b"]))]),
        );
    }

    #[test]
    fn required_combobox_needs_an_option() {
        let elements = [combobox("kinds", true, true)];

        check(
            &elements,
            &[("kinds", list(&["a"]))],
            Ok(&[("kinds", list(&["a"]))]),
        );
        check(&elements, &[("kinds", list(&["x"]))], Err("kinds"));
    }

    #[test]
    fn resolve_fills_defaults_for_missing_values_only() {
        let kinds = ComboboxField::multiple(
            "kinds",
            "Kinds",
            options(&["a", "b"]),
            vec!["a".to_owned(), "b".to_owned()],
        );
        let FormElement::Text(mut region) = optional("region") else {
            unreachable!()
        };
        region.default = Some("eu".to_owned());
        let elements = [
            FormElement::Combobox(kinds.unwrap()),
            FormElement::Text(region),
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
        #[track_caller]
        fn check(
            options: Vec<ComboboxOption>,
            default: &[&str],
            expected: Result<(), InvalidCombobox>,
        ) {
            let default = default.iter().map(|value| value.to_string()).collect();
            let actual = ComboboxField::multiple("k", "K", options, default).map(drop);
            let expected = expected.map_err(|kind| InvalidFormError::Combobox {
                id: "k".to_owned(),
                kind,
            });
            assert_eq!(actual, expected);
        }

        let option = |value: &str, label: &str| ComboboxOption {
            value: value.to_owned(),
            label: label.to_owned(),
        };

        check(options(&["a", "b"]), &["a"], Ok(()));
        check(options(&[""]), &[], Err(InvalidCombobox::EmptyOption));
        check(
            vec![option("a", "A"), option("a", "B")],
            &[],
            Err(InvalidCombobox::DuplicateOption("a".to_owned())),
        );
        check(
            vec![option("a", "A"), option("b", "A")],
            &[],
            Err(InvalidCombobox::DuplicateLabel("A".to_owned())),
        );
        check(
            options(&["a", "b"]),
            &["x"],
            Err(InvalidCombobox::UnknownDefault("x".to_owned())),
        );
    }
}
