use crate::api::{get_products, giveaway_login, login};
use base64::{Engine, engine::general_purpose::STANDARD};
use pdk::{
    Error, Game, GameVersion, LoginRequest, Platform, Plugin, Storefront, kv_store::KvStore,
    register_storefront,
};

mod api;
mod error;
mod mapper;

pub struct LegacyGames;

impl Plugin for LegacyGames {
    async fn init() -> Result<(), Error> {
        let email: String = KvStore::get("email")?.ok_or(Error::auth("not logged in"))?;
        let token = KvStore::get::<String>("token")?;

        match token {
            Some(token) => {
                login(&token).await?.into_result()?;
            }
            None => giveaway_login(&email).await?.into_result()?,
        }

        Ok(())
    }

    async fn login(request: LoginRequest) -> Result<(), Error> {
        if let LoginRequest::Form(form) = request {
            let email = form
                .fields
                .iter()
                .find(|(key, _)| key == "email")
                .map(|(_, value)| value)
                .ok_or(Error::auth("email is mandatory"))?;

            let password = form
                .fields
                .iter()
                .find(|(key, _)| key == "password")
                .map(|(_, value)| value);

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
        } else {
            Err(Error::auth("fields should not be None"))
        }
    }

    async fn logout() -> Result<(), Error> {
        KvStore::delete("email")?;
        KvStore::delete("token")?;
        Ok(())
    }
}

impl Storefront for LegacyGames {
    async fn get_games() -> Result<Vec<Game>, Error> {
        let email: String = KvStore::get("email")?.ok_or(Error::auth("not logged in"))?;
        let token = KvStore::get::<String>("token")?;
        let user_id = KvStore::get("user_id")?;

        let games = get_products(&email, token.as_deref(), user_id)
            .await?
            .into_iter()
            .flat_map(Vec::<Game>::from)
            .collect();

        Ok(games)
    }

    async fn get_game_versions(game: Game) -> Result<Vec<GameVersion>, Error> {
        Ok(vec![GameVersion {
            id: game.lookup_id,
            pretty_name: Some(game.name),
            platform: Platform::Windows,
        }])
    }
}

register_storefront!(LegacyGames);
