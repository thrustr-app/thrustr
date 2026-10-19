use crate::component::{
    Error, FieldValue, Form, FormElement, InvalidFormError, Operation, form::unique_ids,
};
use async_trait::async_trait;
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
    async fn validate(&self, fields: HashMap<String, FieldValue>) -> Result<(), Error>;
}

#[derive(Clone, Debug)]
pub struct ConfigSchema {
    sections: Vec<ConfigSection>,
}

impl ConfigSchema {
    pub fn new(sections: Vec<ConfigSection>) -> Result<Self, InvalidFormError> {
        let schema = Self { sections };
        unique_ids(&schema)?;
        Ok(schema)
    }

    pub fn sections(&self) -> &[ConfigSection] {
        &self.sections
    }
}

#[derive(Clone, Debug)]
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
    use crate::component::form::tests::{optional, required, t};

    fn schema<const N: usize>(
        sections: [Vec<FormElement>; N],
    ) -> Result<ConfigSchema, InvalidFormError> {
        ConfigSchema::new(
            sections
                .into_iter()
                .map(|elements| ConfigSection {
                    name: "section".to_owned(),
                    elements,
                })
                .collect(),
        )
    }

    #[test]
    fn field_ids_are_unique() {
        assert_eq!(
            schema([vec![optional("port")], vec![required("port")]]).unwrap_err(),
            InvalidFormError::DuplicateField("port".to_owned())
        );
    }

    #[test]
    fn fields_are_collected_from_every_section() {
        let config = schema([vec![optional("region")], vec![required("port")]]).unwrap();

        let mut values = HashMap::from([("region".to_owned(), t("eu"))]);
        assert_eq!(config.retain_valid(&mut values).unwrap_err().id, "port");

        values.insert("port".to_owned(), t("8080"));
        assert_eq!(config.retain_valid(&mut values), Ok(()));
        assert_eq!(values.len(), 2);
    }
}
