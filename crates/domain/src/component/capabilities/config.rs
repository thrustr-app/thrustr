use crate::component::{
    Error, FormElement, MissingFieldError, Operation, TextField, form, text_fields,
};
use async_trait::async_trait;
use serde::Deserialize;
use std::{borrow::Borrow, collections::HashMap, hash::Hash};
use strum::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum ConfigOperation {
    #[strum(to_string = "config")]
    Save,
}

impl From<ConfigOperation> for Operation {
    fn from(operation: ConfigOperation) -> Self {
        Self::Config(operation)
    }
}

#[async_trait]
pub trait Config: Send + Sync {
    fn schema(&self) -> &ConfigSchema;
    async fn validate(&self, fields: HashMap<String, String>) -> Result<(), Error>;
}

#[derive(Deserialize, Clone, Debug)]
pub struct ConfigSchema {
    #[serde(rename = "section")]
    pub sections: Vec<ConfigSection>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ConfigSection {
    pub name: String,
    #[serde(rename = "field")]
    pub elements: Vec<FormElement>,
}

impl ConfigSchema {
    pub fn text_fields(&self) -> impl Iterator<Item = &TextField> {
        self.sections.iter().flat_map(ConfigSection::text_fields)
    }

    /// Checks every required field has a value and drops values for
    /// undeclared fields.
    pub fn check(&self, values: &mut HashMap<String, String>) -> Result<(), MissingFieldError> {
        form::check(|| self.text_fields(), values)
    }

    /// Returns the first required field without a value.
    pub fn missing_field<K, V>(&self, values: &HashMap<K, V>) -> Option<&TextField>
    where
        K: Borrow<str> + Hash + Eq,
        V: AsRef<str>,
    {
        form::missing_field(self.text_fields(), values)
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
    use crate::component::form::tests::{optional, required};

    #[test]
    fn fields_are_collected_from_every_section() {
        let config = ConfigSchema {
            sections: [vec![optional("region")], vec![required("port")]]
                .into_iter()
                .map(|elements| ConfigSection {
                    name: "section".to_owned(),
                    elements,
                })
                .collect(),
        };

        let mut values = HashMap::from([("region".to_owned(), "eu".to_owned())]);
        assert_eq!(config.check(&mut values).unwrap_err().id, "port");

        values.insert("port".to_owned(), "8080".to_owned());
        assert_eq!(config.check(&mut values), Ok(()));
        assert_eq!(values.len(), 2);
    }
}
