use crate::{
    SqliteStorage,
    models::{NewComponentConfigRow, NewComponentDataRow},
};
use anyhow::Result;
use diesel::{
    BoolExpressionMethods, Connection, EscapeExpressionMethods, ExpressionMethods,
    OptionalExtension, QueryDsl, QueryResult, RunQueryDsl, SqliteConnection, TextExpressionMethods,
    upsert::excluded,
};
use domain::component::{ComponentStorage, FieldValue};
use serde_json::Value;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::warn;

impl ComponentStorage for SqliteStorage {
    fn get_data(&self, component_id: &str, key: &str) -> Result<Option<Vec<u8>>> {
        use crate::schema::component_data::dsl;

        let mut conn = self.conn()?;
        let now = unix_millis(SystemTime::now());
        let result = dsl::component_data
            .find((component_id, key))
            .filter(dsl::expires_at.is_null().or(dsl::expires_at.gt(now)))
            .select(dsl::value)
            .first::<Vec<u8>>(&mut conn)
            .optional()?;

        Ok(result)
    }

    fn set_data(
        &self,
        component_id: &str,
        key: &str,
        data: &[u8],
        expires_at: Option<SystemTime>,
    ) -> Result<()> {
        use crate::schema::component_data::dsl;

        let mut conn = self.conn()?;
        diesel::insert_into(dsl::component_data)
            .values(NewComponentDataRow {
                component_id,
                key,
                value: data,
                expires_at: expires_at.map(unix_millis),
            })
            .on_conflict((dsl::component_id, dsl::key))
            .do_update()
            .set((
                dsl::value.eq(excluded(dsl::value)),
                dsl::expires_at.eq(excluded(dsl::expires_at)),
            ))
            .execute(&mut conn)?;

        Ok(())
    }

    fn delete_data(&self, component_id: &str, key: &str) -> Result<()> {
        use crate::schema::component_data::dsl;

        let mut conn = self.conn()?;
        diesel::delete(dsl::component_data.find((component_id, key))).execute(&mut conn)?;

        Ok(())
    }

    fn list_data(&self, component_id: &str, prefix: &str) -> Result<Vec<String>> {
        use crate::schema::component_data::dsl;

        let escaped = prefix
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");

        let mut conn = self.conn()?;
        let now = unix_millis(SystemTime::now());
        Ok(dsl::component_data
            .filter(dsl::component_id.eq(component_id))
            .filter(dsl::expires_at.is_null().or(dsl::expires_at.gt(now)))
            .filter(dsl::key.like(format!("{escaped}%")).escape('\\'))
            .select(dsl::key)
            .load::<String>(&mut conn)?)
    }

    fn get_config_value(&self, component_id: &str, field_id: &str) -> Result<Option<FieldValue>> {
        use crate::schema::component_config::dsl;

        let mut conn = self.conn()?;
        let value = dsl::component_config
            .find((component_id, field_id))
            .select(dsl::value)
            .first::<Value>(&mut conn)
            .optional()?;

        Ok(value.and_then(|value| config_value(component_id, field_id, value)))
    }

    fn get_config_values(&self, component_id: &str) -> Result<HashMap<String, FieldValue>> {
        use crate::schema::component_config::dsl;
        let mut conn = self.conn()?;
        Ok(dsl::component_config
            .select((dsl::field_id, dsl::value))
            .filter(dsl::component_id.eq(component_id))
            .load::<(String, Value)>(&mut conn)?
            .into_iter()
            .filter_map(|(field_id, value)| {
                let value = config_value(component_id, &field_id, value)?;
                Some((field_id, value))
            })
            .collect())
    }

    fn set_config_values(
        &self,
        component_id: &str,
        fields: &HashMap<String, FieldValue>,
    ) -> Result<()> {
        let mut conn = self.conn()?;
        // TODO: this should probably delete the whole config first
        // so old fields are not kept around.
        conn.transaction(|conn| {
            for (field_id, value) in fields {
                upsert_config_value(conn, component_id, field_id, value)?;
            }
            Ok(())
        })
    }
}

/// Deletes entries that have expired.
pub(crate) fn delete_expired_data(conn: &mut SqliteConnection) -> QueryResult<usize> {
    use crate::schema::component_data::dsl;

    let now = unix_millis(SystemTime::now());
    diesel::delete(
        dsl::component_data.filter(dsl::expires_at.is_not_null().and(dsl::expires_at.le(now))),
    )
    .execute(conn)
}

fn unix_millis(time: SystemTime) -> i64 {
    let millis = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

fn config_value(component_id: &str, field_id: &str, value: Value) -> Option<FieldValue> {
    serde_json::from_value(value)
        .inspect_err(|err| {
            warn!(
                component_id,
                field_id, "skipping invalid config value: {err}"
            )
        })
        .ok()
}

fn upsert_config_value(
    conn: &mut SqliteConnection,
    component_id: &str,
    field_id: &str,
    value: &FieldValue,
) -> QueryResult<usize> {
    use crate::schema::component_config::dsl;

    let value = serde_json::to_value(value)
        .map_err(|err| diesel::result::Error::SerializationError(err.into()))?;

    diesel::insert_into(dsl::component_config)
        .values(NewComponentConfigRow {
            component_id,
            field_id,
            value,
        })
        .on_conflict((dsl::component_id, dsl::field_id))
        .do_update()
        .set(dsl::value.eq(excluded(dsl::value)))
        .execute(conn)
}
