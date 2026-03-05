use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Field {
    Text { id: String, label: String },
}

#[derive(Deserialize, Debug)]
pub struct Section {
    pub name: String,
    #[serde(rename = "field")]
    pub fields: Vec<Field>,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(rename = "section")]
    pub sections: Vec<Section>,
}
