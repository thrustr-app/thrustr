use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ConfigItem {
    Section {
        name: String,
        items: Vec<ConfigItem>,
    },
    Text {
        id: String,
        label: String,
    },
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub items: Vec<ConfigItem>,
}
