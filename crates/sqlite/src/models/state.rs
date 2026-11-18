use crate::schema::app_state;
use diesel::prelude::*;
use serde_json::Value;

#[derive(Insertable)]
#[diesel(table_name = app_state)]
pub struct NewAppStateRow<'a> {
    pub key: &'a str,
    pub value: &'a Value,
}
