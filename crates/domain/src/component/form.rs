use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FormElement {
    Text(TextField),
    Hbox {
        #[serde(rename = "field")]
        elements: Vec<FormElement>,
    },
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
            Self::Hbox { elements } => Box::new(text_fields(elements)),
        }
    }
}

pub(crate) fn text_fields(elements: &[FormElement]) -> impl Iterator<Item = &TextField> {
    elements.iter().flat_map(FormElement::text_fields)
}
