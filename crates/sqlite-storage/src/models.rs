use diesel::{prelude::*, sqlite::Sqlite};
use serde_json::Value;

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::plugin_data)]
#[diesel(check_for_backend(Sqlite))]
pub struct PluginData {
    pub plugin_id: String,
    pub data: Value,
}
