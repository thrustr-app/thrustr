use diesel::{prelude::*, sqlite::Sqlite};

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::plugin_data)]
#[diesel(check_for_backend(Sqlite))]
pub struct PluginData {
    pub plugin_id: String,
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::plugin_config)]
#[diesel(check_for_backend(Sqlite))]
pub struct PluginConfig {
    pub plugin_id: String,
    pub field_id: String,
    pub value: String,
}
