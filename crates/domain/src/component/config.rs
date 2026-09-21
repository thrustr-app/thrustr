use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Element {
    Text {
        id: String,
        label: String,
        placeholder: Option<String>,
        #[serde(default)]
        required: bool,
    },
    Hbox {
        #[serde(rename = "field")]
        elements: Vec<Element>,
    },
}

impl Element {
    pub fn id(&self) -> Option<&str> {
        match self {
            Element::Text { id, .. } => Some(id),
            Element::Hbox { .. } => None,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct ConfigSection {
    pub name: String,
    #[serde(rename = "field")]
    pub elements: Vec<Element>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ComponentConfig {
    #[serde(rename = "section")]
    pub sections: Vec<ConfigSection>,
}
