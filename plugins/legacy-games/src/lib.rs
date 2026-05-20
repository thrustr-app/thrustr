use crate::api::{get_products, giveaway_login, login};
use base64::{Engine, engine::general_purpose::STANDARD};
use pdk::{
    Error, Game, GameVersion, Platform, Plugin, Storefront, kv_store::KvStore, register_storefront,
};
use std::collections::HashMap;
use wstd::runtime::block_on;

mod api;
mod error;
mod mapper;

pub struct LegacyGames;

impl Plugin for LegacyGames {
    fn init() -> Result<(), Error> {
        block_on(async move {
            let email: String = KvStore::get("email")?.ok_or(Error::auth("not logged in"))?;
            let token = KvStore::get::<String>("token")?;

            match token {
                Some(token) => {
                    login(&token).await?.into_result()?;
                }
                None => giveaway_login(&email).await?.into_result()?,
            }

            Ok(())
        })
    }

    fn login(
        _url: Option<String>,
        _body: Option<String>,
        fields: Option<HashMap<String, String>>,
    ) -> Result<(), Error> {
        block_on(async {
            let fields = fields.ok_or(Error::auth("fields should not be None"))?;
            let email = fields
                .get("email")
                .ok_or(Error::auth("email is mandatory"))?;
            let password = fields.get("password");

            if let Some(password) = password {
                let token = STANDARD.encode(format!("{email}:{password}"));
                let user_id = login(&token).await?.into_result()?.user_id;

                KvStore::set("user_id", &user_id)?;
                KvStore::set("token", &token)?;
            } else {
                giveaway_login(email).await?.into_result()?;
            }

            KvStore::set("email", email)?;

            Ok(())
        })
    }

    fn logout() -> Result<(), Error> {
        KvStore::delete("email")?;
        KvStore::delete("token")?;
        Ok(())
    }
}

impl Storefront for LegacyGames {
    fn get_games() -> Result<Vec<Game>, Error> {
        block_on(async {
            let email: String = KvStore::get("email")?.ok_or(Error::auth("not logged in"))?;
            let token = KvStore::get::<String>("token")?;
            let user_id = KvStore::get("user_id")?;

            let games = get_products(&email, token.as_deref(), user_id)
                .await?
                .into_iter()
                .flat_map(Vec::<Game>::from)
                .collect();

            Ok(games)
        })
    }

    fn get_game_versions(game: Game) -> Result<Vec<GameVersion>, Error> {
        Ok(vec![GameVersion {
            id: game.lookup_id,
            pretty_name: Some(game.name),
            platform: Platform::Windows,
        }])
    }
}

register_storefront!(LegacyGames);
