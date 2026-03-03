use diesel::{prelude::*, sqlite::Sqlite};

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::component_data)]
#[diesel(check_for_backend(Sqlite))]
pub struct ComponentData {
    pub component_id: String,
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::component_config)]
#[diesel(check_for_backend(Sqlite))]
pub struct ComponentConfig {
    pub component_id: String,
    pub field_id: String,
    pub value: String,
}
