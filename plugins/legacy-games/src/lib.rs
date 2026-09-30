use crate::api::{fetch_products, giveaway_login, login};
use base64::{Engine, engine::general_purpose::STANDARD};
use pdk::{
    Auth, Error, Game, GameVersion, LoginRequest, Platform, Plugin, Storefront, kv_store::KvStore,
};

mod api;
mod error;
mod mapper;

pub struct LegacyGames;

type Result<T> = std::result::Result<T, Error>;

#[pdk::export]
impl Plugin for LegacyGames {
    async fn init() -> Result<()> {
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
}

#[pdk::export]
impl Auth for LegacyGames {
    async fn login(request: LoginRequest) -> Result<()> {
        if let LoginRequest::Form(form) = request {
            let email = form
                .fields
                .get("email")
                .ok_or(Error::auth("email is mandatory"))?;

            let password = form.fields.get("password");

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

    async fn logout() -> Result<()> {
        KvStore::delete("email")?;
        KvStore::delete("token")?;
        Ok(())
    }
}

#[pdk::export]
impl Storefront for LegacyGames {
    async fn list_games() -> Result<Vec<Game>> {
        let email: String = KvStore::get("email")?.ok_or(Error::auth("not logged in"))?;
        let token = KvStore::get::<String>("token")?;
        let user_id = KvStore::get("user_id")?;

        let games = fetch_products(&email, token.as_deref(), user_id)
            .await?
            .into_iter()
            .flat_map(Vec::<Game>::from)
            .collect();

        Ok(games)
    }

    async fn list_game_versions(game: Game) -> Result<Vec<GameVersion>> {
        Ok(vec![GameVersion {
            id: game.lookup_id,
            pretty_name: Some(game.name),
            platform: Platform::Windows,
        }])
    }
}
