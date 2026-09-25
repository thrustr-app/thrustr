use crate::component::{Error, FormElement, TextField, text_fields};
use serde::Deserialize;
use std::collections::HashMap;

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

impl ComponentConfig {
    pub fn text_fields(&self) -> impl Iterator<Item = &TextField> {
        self.sections.iter().flat_map(ConfigSection::text_fields)
    }

    /// Checks every required field has a value and drops values for
    /// undeclared fields.
    pub fn validate(&self, values: &mut HashMap<String, String>) -> Result<(), Error> {
        values.retain(|id, _| self.text_fields().any(|field| field.id == *id));

        match self
            .text_fields()
            .filter(|field| field.required)
            .find(|field| values.get(&field.id).is_none_or(String::is_empty))
        {
            Some(field) => Err(Error::Config(format!("{} is required", field.label))),
            None => Ok(()),
        }
    }
}

impl ConfigSection {
    pub fn text_fields(&self) -> impl Iterator<Item = &TextField> {
        text_fields(&self.elements)
    }
}
