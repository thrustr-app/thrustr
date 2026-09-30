use serde::Deserialize;
use std::{borrow::Borrow, collections::HashMap, hash::Hash};

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FormElement {
    Text(TextField),
    Hbox { elements: Vec<FormElement> },
}

#[derive(Deserialize, Clone, Debug)]
pub struct TextField {
    pub id: String,
    pub label: String,
    pub placeholder: Option<String>,
    #[serde(default)]
    pub required: bool,
}

impl FormElement {
    pub fn text_fields(&self) -> Box<dyn Iterator<Item = &TextField> + '_> {
        match self {
            Self::Text(field) => Box::new(std::iter::once(field)),
            Self::Hbox { elements } => Box::new(elements.text_fields()),
        }
    }

    fn find_field<'a>(
        &'a self,
        predicate: &mut impl FnMut(&TextField) -> bool,
    ) -> Option<&'a TextField> {
        match self {
            Self::Text(field) => predicate(field).then_some(field),
            Self::Hbox { elements } => elements
                .iter()
                .find_map(|element| element.find_field(predicate)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{label} is required")]
pub struct MissingFieldError {
    pub id: String,
    pub label: String,
}

pub trait Form {
    /// Top level elements.
    fn elements(&self) -> impl Iterator<Item = &FormElement>;

    fn text_fields(&self) -> impl Iterator<Item = &TextField> {
        self.elements().flat_map(FormElement::text_fields)
    }

    /// Returns the first required field without a value.
    fn missing_field<K, V>(&self, values: &HashMap<K, V>) -> Option<&TextField>
    where
        K: Borrow<str> + Hash + Eq,
        V: AsRef<str>,
    {
        find_field(self.elements(), |field| {
            field.required
                && values
                    .get(field.id.as_str())
                    .is_none_or(|value| value.as_ref().is_empty())
        })
    }

    /// Checks required fields have a value and drops values for undeclared fields.
    fn check(&self, values: &mut HashMap<String, String>) -> Result<(), MissingFieldError> {
        values.retain(|id, _| find_field(self.elements(), |field| field.id == *id).is_some());

        match self.missing_field(values) {
            Some(field) => Err(MissingFieldError {
                id: field.id.clone(),
                label: field.label.clone(),
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

fn find_field<'a>(
    mut elements: impl Iterator<Item = &'a FormElement>,
    mut predicate: impl FnMut(&TextField) -> bool,
) -> Option<&'a TextField> {
    elements.find_map(|element| element.find_field(&mut predicate))
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

    fn text(id: &str, required: bool) -> FormElement {
        FormElement::Text(TextField {
            id: id.to_owned(),
            label: id.to_uppercase(),
            placeholder: None,
            required,
        })
    }

    fn hbox<const N: usize>(elements: [FormElement; N]) -> FormElement {
        FormElement::Hbox {
            elements: elements.into(),
        }
    }

    #[track_caller]
    fn check(
        elements: &[FormElement],
        values: &[(&str, &str)],
        expected: Result<&[(&str, &str)], &str>,
    ) {
        let owned = |values: &[(&str, &str)]| -> HashMap<String, String> {
            values
                .iter()
                .map(|(id, value)| (id.to_string(), value.to_string()))
                .collect()
        };

        let mut actual = owned(values);
        match (elements.check(&mut actual), expected) {
            (Ok(()), Ok(kept)) => assert_eq!(actual, owned(kept)),
            (Err(error), Err(id)) => assert_eq!(error.id, id),
            (actual, expected) => panic!("expected {expected:?}, got {actual:?}"),
        }
    }

    #[test]
    fn required_fields_must_have_a_value() {
        let elements = [required("user"), optional("region")];

        check(
            &elements,
            &[("user", "username")],
            Ok(&[("user", "username")]),
        );
        check(&elements, &[("region", "eu")], Err("user"));
        check(&elements, &[("user", ""), ("region", "eu")], Err("user"));
    }

    #[test]
    fn optional_fields_may_be_empty_or_missing() {
        let elements = [optional("region")];

        check(&elements, &[], Ok(&[]));
        check(&elements, &[("region", "")], Ok(&[("region", "")]));
    }

    #[test]
    fn undeclared_values_are_dropped() {
        let elements = [optional("region")];

        check(
            &elements,
            &[("region", "eu"), ("stale", "value")],
            Ok(&[("region", "eu")]),
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
            &[("region", "eu"), ("host", "localhost"), ("port", "8080")],
            Ok(&[("region", "eu"), ("host", "localhost"), ("port", "8080")]),
        );
        check(&elements, &[("host", "localhost")], Err("port"));
    }
}
