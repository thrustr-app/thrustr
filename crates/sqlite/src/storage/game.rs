use crate::SqliteStorage;
use crate::models::{ArtworkRow, GameRow, NewGameRow};
use anyhow::Result;
use diesel::{
    BoolExpressionMethods, Connection, ExpressionMethods, JoinOnDsl, OptionalExtension, QueryDsl,
    RunQueryDsl, SelectableHelper,
};
use domain::artwork::{Artwork, ArtworkKind};
use domain::game::{Game, GameId, GameListItem, GameRepository, NewGame};

impl GameRepository for SqliteStorage {
    fn insert(&self, game: &NewGame) -> Result<Option<Game>> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        let row = diesel::insert_or_ignore_into(dsl::games)
            .values(NewGameRow::from(game))
            .returning(GameRow::as_returning())
            .get_result::<GameRow>(&mut conn)
            .optional()?;

        Ok(row.map(Game::from))
    }

    fn insert_many(&self, games: &[NewGame]) -> Result<Vec<Game>> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        conn.transaction(|conn| {
            let mut inserted = Vec::new();
            for game in games {
                let row = diesel::insert_or_ignore_into(dsl::games)
                    .values(NewGameRow::from(game))
                    .returning(GameRow::as_returning())
                    .get_result::<GameRow>(conn)
                    .optional()?;

                if let Some(row) = row {
                    inserted.push(Game::from(row));
                }
            }
            Ok(inserted)
        })
    }

    fn count(&self) -> Result<usize> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        let count = dsl::games.count().get_result::<i64>(&mut conn)?;
        Ok(count as usize)
    }

    fn get(&self, id: GameId) -> Result<Option<Game>> {
        use crate::schema::games::dsl;

        let id = u64::from(id) as i64;
        let mut conn = self.pool.get()?;
        let row = dsl::games
            .find(id)
            .select(GameRow::as_select())
            .first::<GameRow>(&mut conn)
            .optional()?;

        Ok(row.map(Game::from))
    }

    fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListItem>> {
        use crate::schema::artwork;
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        let rows: Vec<(GameRow, Option<ArtworkRow>)> = dsl::games
            .left_join(artwork::table.on(artwork::game_id.eq(dsl::id)))
            .order(dsl::name.asc())
            .limit(limit as i64)
            .offset(offset as i64)
            .select((GameRow::as_select(), Option::<ArtworkRow>::as_select()))
            .load(&mut conn)?;
        let items = rows
            .into_iter()
            .map(|(game, artwork)| GameListItem {
                id: (game.id as u64).into(),
                name: game.name,
                source_id: game.source_id,
                cover_url: game.cover_url,
                artwork: artwork.map(Artwork::from),
            })
            .collect();
        Ok(items)
    }

    fn games_missing_artwork(
        &self,
        kind: ArtworkKind,
        after: GameId,
        limit: usize,
    ) -> Result<Vec<(GameId, String)>> {
        use crate::schema::artwork;
        use crate::schema::games::dsl;

        let after = u64::from(after) as i64;
        let mut conn = self.pool.get()?;
        let rows: Vec<(i64, Option<String>)> = dsl::games
            .left_join(
                artwork::table.on(artwork::game_id
                    .eq(dsl::id)
                    .and(artwork::kind.eq(kind.as_ref()))),
            )
            // TODO: when per-kind source URLs exist, filter on that
            .filter(dsl::cover_url.is_not_null())
            .filter(artwork::game_id.is_null())
            .filter(dsl::id.gt(after))
            .order(dsl::id.asc())
            .limit(limit as i64)
            .select((dsl::id, dsl::cover_url))
            .load(&mut conn)?;

        Ok(rows
            .into_iter()
            .filter_map(|(id, url)| url.map(|url| ((id as u64).into(), url)))
            .collect())
    }
}
