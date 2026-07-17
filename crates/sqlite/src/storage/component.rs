use crate::{
    SqliteStorage,
    models::{NewComponentConfigRow, NewComponentDataRow},
};
use anyhow::Result;
use diesel::{
    Connection, EscapeExpressionMethods, ExpressionMethods, OptionalExtension, QueryDsl,
    QueryResult, RunQueryDsl, SqliteConnection, TextExpressionMethods, upsert::excluded,
};
use domain::component::ComponentStorage;
use std::collections::HashMap;

impl ComponentStorage for SqliteStorage {
    fn get_data(&self, component_id: &str, key: &str) -> Result<Option<Vec<u8>>> {
        use crate::schema::component_data::dsl;

        let mut conn = self.pool.get()?;
        let result = dsl::component_data
            .find((component_id, key))
            .select(dsl::value)
            .first::<Vec<u8>>(&mut conn)
            .optional()?;

        Ok(result)
    }

    fn set_data(&self, component_id: &str, key: &str, data: &[u8]) -> Result<()> {
        use crate::schema::component_data::dsl;

        let mut conn = self.pool.get()?;
        diesel::insert_into(dsl::component_data)
            .values(NewComponentDataRow {
                component_id,
                key,
                value: data,
            })
            .on_conflict((dsl::component_id, dsl::key))
            .do_update()
            .set(dsl::value.eq(excluded(dsl::value)))
            .execute(&mut conn)?;

        Ok(())
    }

    fn delete_data(&self, component_id: &str, key: &str) -> Result<()> {
        use crate::schema::component_data::dsl;

        let mut conn = self.pool.get()?;
        diesel::delete(dsl::component_data.find((component_id, key))).execute(&mut conn)?;

        Ok(())
    }

    fn list_data(&self, component_id: &str, prefix: Option<&str>) -> Result<Vec<String>> {
        use crate::schema::component_data::dsl;

        let mut conn = self.pool.get()?;
        let mut query = dsl::component_data
            .filter(dsl::component_id.eq(component_id))
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

        let mut conn = self.pool.get()?;
        let result = dsl::component_config
            .find((component_id, field_id))
            .select(dsl::value)
            .first::<String>(&mut conn)
            .optional()?;

        Ok(result)
    }

    fn get_config_values(&self, component_id: &str) -> Result<HashMap<String, String>> {
        use crate::schema::component_config::dsl;
        let mut conn = self.pool.get()?;
        Ok(dsl::component_config
            .select((dsl::field_id, dsl::value))
            .filter(dsl::component_id.eq(component_id))
            .load::<(String, String)>(&mut conn)?
            .into_iter()
            .collect())
    }

    fn set_config_value(&self, component_id: &str, field_id: &str, value: &str) -> Result<()> {
        let mut conn = self.pool.get()?;
        upsert_config_value(&mut conn, component_id, field_id, value)?;

        Ok(())
    }

    fn set_config_values(
        &self,
        component_id: &str,
        fields: &HashMap<String, String>,
    ) -> Result<()> {
        let mut conn = self.pool.get()?;
        conn.transaction(|conn| {
            for (field_id, value) in fields {
                upsert_config_value(conn, component_id, field_id, value)?;
            }
            Ok(())
        })
    }
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
