use crate::{SqliteStorage, models::NewAppStateRow};
use anyhow::Result;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, upsert::excluded};
use domain::state::AppStateRepository;
use serde_json::Value;

impl AppStateRepository for SqliteStorage {
    fn get(&self, key: &str) -> Result<Option<Value>> {
        use crate::schema::app_state::dsl;

        self.read(|conn| {
            Ok(dsl::app_state
                .find(key)
                .select(dsl::value)
                .first::<Value>(conn)
                .optional()?)
        })
    }

    fn set(&self, key: &str, value: &Value) -> Result<()> {
        use crate::schema::app_state::dsl;

        self.write(|conn| {
            diesel::insert_into(dsl::app_state)
                .values(NewAppStateRow { key, value })
                .on_conflict(dsl::key)
                .do_update()
                .set(dsl::value.eq(excluded(dsl::value)))
                .execute(conn)?;
            Ok(())
        })
    }

    fn delete(&self, key: &str) -> Result<()> {
        use crate::schema::app_state::dsl;

        self.write(|conn| {
            diesel::delete(dsl::app_state.find(key)).execute(conn)?;
            Ok(())
        })
    }
}
