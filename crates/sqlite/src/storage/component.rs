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
use domain::component::ComponentStorage;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

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

    fn list_data(&self, component_id: &str, prefix: Option<&str>) -> Result<Vec<String>> {
        use crate::schema::component_data::dsl;

        let mut conn = self.conn()?;
        let now = unix_millis(SystemTime::now());
        let mut query = dsl::component_data
            .filter(dsl::component_id.eq(component_id))
            .filter(dsl::expires_at.is_null().or(dsl::expires_at.gt(now)))
            .select(dsl::key)
            .into_boxed();

        if let Some(p) = prefix {
            let escaped = p
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            query = query.filter(dsl::key.like(format!("{escaped}%")).escape('\\'));
        }

        Ok(query.load::<String>(&mut conn)?)
    }

    fn get_config_value(&self, component_id: &str, field_id: &str) -> Result<Option<String>> {
        use crate::schema::component_config::dsl;

        let mut conn = self.conn()?;
        let result = dsl::component_config
            .find((component_id, field_id))
            .select(dsl::value)
            .first::<String>(&mut conn)
            .optional()?;

        Ok(result)
    }

    fn get_config_values(&self, component_id: &str) -> Result<HashMap<String, String>> {
        use crate::schema::component_config::dsl;
        let mut conn = self.conn()?;
        Ok(dsl::component_config
            .select((dsl::field_id, dsl::value))
            .filter(dsl::component_id.eq(component_id))
            .load::<(String, String)>(&mut conn)?
            .into_iter()
            .collect())
    }

    fn set_config_values(
        &self,
        component_id: &str,
        fields: &HashMap<String, String>,
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

fn upsert_config_value(
    conn: &mut SqliteConnection,
    component_id: &str,
    field_id: &str,
    value: &str,
) -> QueryResult<usize> {
    use crate::schema::component_config::dsl;

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
