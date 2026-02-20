use crate::{
    SqliteStorage,
    models::{ExtensionConfig, ExtensionData},
};
use anyhow::Result;
use diesel::{
    ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, TextExpressionMethods,
    upsert::excluded,
};
use ports::storage::ExtensionStorage;

impl ExtensionStorage for SqliteStorage {
    fn get_data(&self, extension_id: &str, key: &str) -> Result<Option<Vec<u8>>> {
        use crate::extension_data::dsl;

        let mut conn = self.pool.get()?;
        let result: Option<ExtensionData> = dsl::extension_data
            .filter(dsl::extension_id.eq(extension_id))
            .filter(dsl::key.eq(key))
            .first::<ExtensionData>(&mut conn)
            .optional()?;

        Ok(result.map(|pd| pd.value))
    }

    fn set_data(&self, extension_id: &str, key: &str, data: Vec<u8>) -> Result<()> {
        use crate::extension_data::dsl;

        let value = ExtensionData {
            extension_id: extension_id.to_owned(),
            key: key.to_owned(),
            value: data,
        };

        let mut conn = self.pool.get()?;
        diesel::insert_into(dsl::extension_data)
            .values(&value)
            .on_conflict((dsl::extension_id, dsl::key))
            .do_update()
            .set(dsl::value.eq(excluded(dsl::value)))
            .execute(&mut conn)?;

        Ok(())
    }

    fn delete_data(&self, extension_id: &str, key: &str) -> Result<()> {
        use crate::extension_data::dsl;

        let mut conn = self.pool.get()?;
        diesel::delete(
            dsl::extension_data
                .filter(dsl::extension_id.eq(extension_id))
                .filter(dsl::key.eq(key)),
        )
        .execute(&mut conn)?;

        Ok(())
    }

    fn list_data(&self, extension_id: &str, prefix: Option<&str>) -> Result<Vec<String>> {
        use crate::extension_data::dsl;

        let mut conn = self.pool.get()?;
        let mut query = dsl::extension_data
            .filter(dsl::extension_id.eq(extension_id))
            .select(dsl::key)
            .into_boxed();

        if let Some(p) = prefix {
            query = query.filter(dsl::key.like(format!("{p}%")));
        }

        Ok(query.load::<String>(&mut conn)?)
    }

    fn get_config(&self, extension_id: &str, field_id: &str) -> Result<Option<String>> {
        use crate::extension_config::dsl;

        let mut conn = self.pool.get()?;
        let result = dsl::extension_config
            .filter(dsl::extension_id.eq(extension_id))
            .filter(dsl::field_id.eq(field_id))
            .first::<ExtensionConfig>(&mut conn)
            .optional()?;

        Ok(result.map(|pd| pd.value))
    }
}
