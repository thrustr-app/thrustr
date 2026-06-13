use crate::{SqliteStorage, models::NewArtworkRow};
use anyhow::Result;
use diesel::{ExpressionMethods, RunQueryDsl};
use domain::{
    artwork::{Artwork, ArtworkRepository},
    game::GameId,
};

impl ArtworkRepository for SqliteStorage {
    fn insert(&self, game_id: GameId, artwork: &Artwork) -> Result<()> {
        use crate::schema::artwork::dsl;

        let row = NewArtworkRow {
            game_id: u64::from(game_id) as i64,
            hash: &artwork.hash,
            accent_color: artwork.accent_color.map(|c| c.to_hex() as i32),
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
                dsl::accent_color.eq(row.accent_color),
            ))
            .execute(&mut conn)?;

        Ok(())
    }
}
