use crate::{SqliteStorage, id::to_row_id, models::NewArtworkRow};
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
            game_id: to_row_id(game_id),
            hash: &artwork.hash,
            accent: artwork.accent.map(|c| c.to_argb_hex() as i32),
            kind: artwork.kind.as_ref(),
            position: artwork.position as i32,
        };

        let mut conn = self.conn()?;
        diesel::insert_into(dsl::artwork)
            .values(&row)
            .on_conflict((dsl::game_id, dsl::kind, dsl::position))
            .do_update()
            .set((dsl::hash.eq(&row.hash), dsl::accent.eq(row.accent)))
            .execute(&mut conn)?;

        Ok(())
    }
}
