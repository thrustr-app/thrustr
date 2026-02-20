use diesel::{prelude::*, sqlite::Sqlite};

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::extension_data)]
#[diesel(check_for_backend(Sqlite))]
pub struct ExtensionData {
    pub extension_id: String,
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::extension_config)]
#[diesel(check_for_backend(Sqlite))]
pub struct ExtensionConfig {
    pub extension_id: String,
    pub field_id: String,
    pub value: String,
}
