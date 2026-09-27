use crate::component::{FormElement, TextField, text_fields};
use serde::Deserialize;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Deserialize, Clone, Debug)]
pub struct ComponentConfig {
    #[serde(rename = "section")]
    pub sections: Vec<ConfigSection>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ConfigSection {
    pub name: String,
    #[serde(rename = "field")]
    pub elements: Vec<FormElement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{label} is required")]
pub struct MissingFieldError {
    pub id: String,
    pub label: String,
}

impl ComponentConfig {
    pub fn text_fields(&self) -> impl Iterator<Item = &TextField> {
        self.sections.iter().flat_map(ConfigSection::text_fields)
    }

    /// Checks every required field has a value and drops values for
    /// undeclared fields.
    pub fn validate(&self, values: &mut HashMap<String, String>) -> Result<(), MissingFieldError> {
        values.retain(|id, _| self.text_fields().any(|field| field.id == *id));

        match self
            .text_fields()
            .filter(|field| field.required)
            .find(|field| values.get(&field.id).is_none_or(String::is_empty))
        {
            Some(field) => Err(MissingFieldError {
                id: field.id.clone(),
                label: field.label.clone(),
            }),
            None => Ok(()),
        }
    }
}

impl ConfigSection {
    pub fn text_fields(&self) -> impl Iterator<Item = &TextField> {
        text_fields(&self.elements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required(id: &str) -> FormElement {
        text(id, true)
    }

    fn optional(id: &str) -> FormElement {
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

    fn config<const N: usize>(sections: [Vec<FormElement>; N]) -> ComponentConfig {
        ComponentConfig {
            sections: sections
                .into_iter()
                .map(|elements| ConfigSection {
                    name: "section".to_owned(),
                    elements,
                })
                .collect(),
        }
    }

    #[track_caller]
    fn check_validate(
        config: &ComponentConfig,
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
        match (config.validate(&mut actual), expected) {
            (Ok(()), Ok(kept)) => assert_eq!(actual, owned(kept)),
            (Err(error), Err(id)) => assert_eq!(error.id, id),
            (actual, expected) => panic!("expected {expected:?}, got {actual:?}"),
        }
    }

    #[test]
    fn required_fields_must_have_a_value() {
        let config = config([vec![required("user"), optional("region")]]);

        check_validate(
            &config,
            &[("user", "username")],
            Ok(&[("user", "username")]),
        );
        check_validate(&config, &[("region", "eu")], Err("user"));
        check_validate(&config, &[("user", ""), ("region", "eu")], Err("user"));
    }

    #[test]
    fn optional_fields_may_be_empty_or_missing() {
        let config = config([vec![optional("region")]]);

        check_validate(&config, &[], Ok(&[]));
        check_validate(&config, &[("region", "")], Ok(&[("region", "")]));
    }

    #[test]
    fn undeclared_values_are_dropped() {
        let config = config([vec![optional("region")]]);

        check_validate(
            &config,
            &[("region", "eu"), ("stale", "value")],
            Ok(&[("region", "eu")]),
        );
    }

    #[test]
    fn fields_are_flattened() {
        let config = config([
            vec![optional("region")],
            vec![hbox([optional("host"), hbox([required("port")])])],
        ]);

        check_validate(
            &config,
            &[("region", "eu"), ("host", "localhost"), ("port", "8080")],
            Ok(&[("region", "eu"), ("host", "localhost"), ("port", "8080")]),
        );
        check_validate(&config, &[("host", "localhost")], Err("port"));
    }
}
