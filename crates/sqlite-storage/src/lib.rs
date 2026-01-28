use crate::{
    models::{PluginConfig, PluginData},
    schema::{plugin_config, plugin_data},
};
use anyhow::Result;
use diesel::{
    ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SqliteConnection,
    TextExpressionMethods,
    r2d2::{ConnectionManager, Pool},
    upsert::excluded,
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use domain::{PluginStorage, Storage};
use std::path::Path;

mod models;
mod schema;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

pub struct SqliteStorage {
    pool: Pool<ConnectionManager<SqliteConnection>>,
}

impl SqliteStorage {
    pub fn new(sqlite_file_path: impl AsRef<Path>) -> Result<Self> {
        let url = format!(
            "sqlite://{}?mode=rwc",
            sqlite_file_path.as_ref().to_str().unwrap()
        );

        let manager = ConnectionManager::<SqliteConnection>::new(url);
        let pool = Pool::builder().max_size(5).build(manager)?;

        let mut connection = pool.get()?;
        connection
            .run_pending_migrations(MIGRATIONS)
            .expect("Failed to run migrations");

        Ok(Self { pool })
    }
}

impl Storage for SqliteStorage {}

impl PluginStorage for SqliteStorage {
    fn get_data(&self, plugin_id: &str, key: &str) -> Result<Option<Vec<u8>>> {
        use crate::plugin_data::dsl;

        let mut conn = self.pool.get()?;
        let result: Option<PluginData> = dsl::plugin_data
            .filter(dsl::plugin_id.eq(plugin_id))
            .filter(dsl::key.eq(key))
            .first::<PluginData>(&mut conn)
            .optional()?;

        Ok(result.map(|pd| pd.value))
    }

    fn set_data(&self, plugin_id: &str, key: &str, data: Vec<u8>) -> Result<()> {
        use crate::plugin_data::dsl;

        let value = PluginData {
            plugin_id: plugin_id.to_owned(),
            key: key.to_owned(),
            value: data,
        };

        let mut conn = self.pool.get()?;
        diesel::insert_into(dsl::plugin_data)
            .values(&value)
            .on_conflict((dsl::plugin_id, dsl::key))
            .do_update()
            .set(dsl::value.eq(excluded(dsl::value)))
            .execute(&mut conn)?;

        Ok(())
    }

    fn delete_data(&self, plugin_id: &str, key: &str) -> Result<()> {
        use crate::plugin_data::dsl;

        let mut conn = self.pool.get()?;
        diesel::delete(
            dsl::plugin_data
                .filter(dsl::plugin_id.eq(plugin_id))
                .filter(dsl::key.eq(key)),
        )
        .execute(&mut conn)?;

        Ok(())
    }

    fn list_data(&self, plugin_id: &str, prefix: Option<&str>) -> Result<Vec<String>> {
        use crate::plugin_data::dsl;

        let mut conn = self.pool.get()?;
        let mut query = dsl::plugin_data
            .filter(dsl::plugin_id.eq(plugin_id))
            .select(dsl::key)
            .into_boxed();

        if let Some(p) = prefix {
            query = query.filter(dsl::key.like(format!("{p}%")));
        }

        Ok(query.load::<String>(&mut conn)?)
    }

    fn get_config(&self, plugin_id: &str, field_id: &str) -> Result<Option<String>> {
        use crate::plugin_config::dsl;

        let mut conn = self.pool.get()?;
        let result = dsl::plugin_config
            .filter(dsl::plugin_id.eq(plugin_id))
            .filter(dsl::field_id.eq(field_id))
            .first::<PluginConfig>(&mut conn)
            .optional()?;

        Ok(result.map(|pd| pd.value))
    }
}
