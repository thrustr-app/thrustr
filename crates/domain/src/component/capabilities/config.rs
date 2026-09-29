use crate::component::{Error, Form, FormElement, Operation};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
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
    pub sections: Vec<ConfigSection>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ConfigSection {
    pub name: String,
    pub elements: Vec<FormElement>,
}

impl Form for ConfigSchema {
    fn elements(&self) -> impl Iterator<Item = &FormElement> {
        self.sections
            .iter()
            .flat_map(|section| section.elements.iter())
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
