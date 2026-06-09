use crate::SqliteStorage;
use crate::models::{ArtworkRow, GameRow, NewGameRow};
use anyhow::Result;
use diesel::{
    Connection, ExpressionMethods, JoinOnDsl, OptionalExtension, QueryDsl, RunQueryDsl,
    SelectableHelper,
};
use domain::artwork::Artwork;
use domain::game::{Game, GameRepository, NewGame};
use game::{GameListItem, GameQuery};

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
}

impl GameQuery for SqliteStorage {
    fn count(&self) -> Result<usize> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        let count = dsl::games.count().get_result::<i64>(&mut conn)?;
        Ok(count as usize)
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
                artwork: artwork.map(Artwork::from),
            })
            .collect();
        Ok(items)
    }
}
