use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Field {
    Text {
        id: String,
        label: String,
        placeholder: Option<String>,
        #[serde(default)]
        required: bool,
    },
}

impl Field {
    pub fn id(&self) -> &str {
        match self {
            Field::Text { id, .. } => id,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct Section {
    pub name: String,
    #[serde(rename = "field")]
    pub fields: Vec<Field>,
}

#[derive(Deserialize, Debug)]
pub struct ComponentConfig {
    #[serde(rename = "section")]
    pub sections: Vec<Section>,
}
