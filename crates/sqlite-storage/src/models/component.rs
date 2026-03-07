use diesel::{prelude::*, sqlite::Sqlite};

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::component_data)]
#[diesel(check_for_backend(Sqlite))]
pub struct ComponentData {
    pub component_id: String,
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::component_data)]
pub struct NewComponentData<'a> {
    pub component_id: &'a str,
    pub key: &'a str,
    pub value: &'a [u8],
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::component_config)]
#[diesel(check_for_backend(Sqlite))]
pub struct ComponentConfig {
    pub component_id: String,
    pub field_id: String,
    pub value: String,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::component_config)]
pub struct NewComponentConfig<'a> {
    pub component_id: &'a str,
    pub field_id: &'a str,
    pub value: &'a str,
}
