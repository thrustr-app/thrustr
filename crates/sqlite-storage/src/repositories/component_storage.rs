use crate::{
    SqliteStorage,
    models::{ComponentConfig, ComponentData},
};
use anyhow::Result;
use diesel::{
    ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, TextExpressionMethods,
    upsert::excluded,
};
use ports::storage::ComponentStorage;

impl ComponentStorage for SqliteStorage {
    fn get_data(&self, component_id: &str, key: &str) -> Result<Option<Vec<u8>>> {
        use crate::schema::component_data::dsl;

        let mut conn = self.pool.get()?;
        let result: Option<ComponentData> = dsl::component_data
            .filter(dsl::component_id.eq(component_id))
            .filter(dsl::key.eq(key))
            .first::<ComponentData>(&mut conn)
            .optional()?;

        Ok(result.map(|pd| pd.value))
    }

    fn set_data(&self, component_id: &str, key: &str, data: Vec<u8>) -> Result<()> {
        use crate::schema::component_data::dsl;

        let value = ComponentData {
            component_id: component_id.to_owned(),
            key: key.to_owned(),
            value: data,
        };

        let mut conn = self.pool.get()?;
        diesel::insert_into(dsl::component_data)
            .values(&value)
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

    fn get_config(&self, component_id: &str, field_id: &str) -> Result<Option<String>> {
        use crate::schema::component_config::dsl;

        let mut conn = self.pool.get()?;
        let result = dsl::component_config
            .filter(dsl::component_id.eq(component_id))
            .filter(dsl::field_id.eq(field_id))
            .first::<ComponentConfig>(&mut conn)
            .optional()?;

        Ok(result.map(|pd| pd.value))
    }
}
