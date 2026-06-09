use crate::{SqliteStorage, models::NewArtworkRow};
use anyhow::Result;
use diesel::{ExpressionMethods, RunQueryDsl};
use domain::{
    artwork::{Artwork, ArtworkRepository},
    game::Game,
    id::Id,
};

impl ArtworkRepository for SqliteStorage {
    fn insert(&self, game_id: Id<Game>, artwork: &Artwork) -> Result<()> {
        use crate::schema::artwork::dsl;

        let row = NewArtworkRow {
            game_id: u64::from(game_id) as i64,
            hash: &artwork.hash,
            vibrant_color: artwork.vibrant_color.map(|c| c.to_hex() as i32),
            kind: artwork.kind.as_ref(),
            position: artwork.position as i32,
        };

        let mut conn = self.pool.get()?;
        diesel::insert_into(dsl::artwork)
            .values(&row)
            .on_conflict((dsl::game_id, dsl::kind, dsl::position))
            .do_update()
            .set((
                dsl::hash.eq(&row.hash),
                dsl::vibrant_color.eq(row.vibrant_color),
            ))
            .execute(&mut conn)?;

        Ok(())
    }
}
