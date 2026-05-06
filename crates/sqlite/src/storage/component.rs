use crate::{
    SqliteStorage,
    models::{ComponentConfigRow, ComponentDataRow, NewComponentConfigRow, NewComponentDataRow},
};
use anyhow::Result;
use diesel::{
    Connection, ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, TextExpressionMethods,
    upsert::excluded,
};
use domain::component::ComponentStorage;

impl ComponentStorage for SqliteStorage {
    fn get_data(&self, component_id: &str, key: &str) -> Result<Option<Vec<u8>>> {
        use crate::schema::component_data::dsl;

        let mut conn = self.pool.get()?;
        let result: Option<ComponentDataRow> = dsl::component_data
            .filter(dsl::component_id.eq(component_id))
            .filter(dsl::key.eq(key))
            .first::<ComponentDataRow>(&mut conn)
            .optional()?;

        Ok(result.map(|pd| pd.value))
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
        diesel::delete(
            dsl::component_data
                .filter(dsl::component_id.eq(component_id))
                .filter(dsl::key.eq(key)),
        )
        .execute(&mut conn)?;

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
            query = query.filter(dsl::key.like(format!("{p}%")));
        }

        Ok(query.load::<String>(&mut conn)?)
    }

    fn get_config_value(&self, component_id: &str, field_id: &str) -> Result<Option<String>> {
        use crate::schema::component_config::dsl;

        let mut conn = self.pool.get()?;
        let result = dsl::component_config
            .filter(dsl::component_id.eq(component_id))
            .filter(dsl::field_id.eq(field_id))
            .first::<ComponentConfigRow>(&mut conn)
            .optional()?;

        Ok(result.map(|pd| pd.value))
    }

    fn get_config_values(&self, component_id: &str) -> Result<Vec<(String, String)>> {
        use crate::schema::component_config::dsl;
        let mut conn = self.pool.get()?;
        Ok(dsl::component_config
            .select((dsl::field_id, dsl::value))
            .filter(dsl::component_id.eq(component_id))
            .load::<(String, String)>(&mut conn)?)
    }

    fn set_config_value(&self, component_id: &str, field_id: &str, value: &str) -> Result<()> {
        use crate::schema::component_config::dsl;

        let mut conn = self.pool.get()?;
        diesel::insert_into(dsl::component_config)
            .values(NewComponentConfigRow {
                component_id,
                field_id,
                value,
            })
            .on_conflict((dsl::component_id, dsl::field_id))
            .do_update()
            .set(dsl::value.eq(excluded(dsl::value)))
            .execute(&mut conn)?;

        Ok(())
    }

    fn set_config_values(&self, component_id: &str, fields: &[(String, String)]) -> Result<()> {
        use crate::schema::component_config::dsl;
        let mut conn = self.pool.get()?;
        conn.transaction(|conn| {
            for (field_id, value) in fields {
                diesel::insert_into(dsl::component_config)
                    .values(NewComponentConfigRow {
                        component_id,
                        field_id,
                        value,
                    })
                    .on_conflict((dsl::component_id, dsl::field_id))
                    .do_update()
                    .set(dsl::value.eq(excluded(dsl::value)))
                    .execute(conn)?;
            }
            Ok(())
        })
    }
}
