use crate::schema::{component_config, component_data};
use diesel::{prelude::*, sqlite::Sqlite};

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = component_data)]
#[diesel(check_for_backend(Sqlite))]
pub struct ComponentDataRow {
    pub component_id: String,
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Insertable)]
#[diesel(table_name = component_data)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewComponentDataRow<'a> {
    pub component_id: &'a str,
    pub key: &'a str,
    pub value: &'a [u8],
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = component_config)]
#[diesel(check_for_backend(Sqlite))]
pub struct ComponentConfigRow {
    pub component_id: String,
    pub field_id: String,
    pub value: String,
}

#[derive(Insertable)]
#[diesel(table_name = component_config)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewComponentConfigRow<'a> {
    pub component_id: &'a str,
    pub field_id: &'a str,
    pub value: &'a str,
}
