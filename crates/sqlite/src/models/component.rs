use crate::schema::{component_config, component_data};
use diesel::prelude::*;

#[derive(Insertable)]
#[diesel(table_name = component_data)]
pub struct NewComponentDataRow<'a> {
    pub component_id: &'a str,
    pub key: &'a str,
    pub value: &'a [u8],
}

#[derive(Insertable)]
#[diesel(table_name = component_config)]
pub struct NewComponentConfigRow<'a> {
    pub component_id: &'a str,
    pub field_id: &'a str,
    pub value: &'a str,
}
