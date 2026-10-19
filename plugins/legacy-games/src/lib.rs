use crate::api::Product;
use base64::{Engine, engine::general_purpose::STANDARD};
use pdk::{
    Auth, Error, FieldValue, Game, GameVersion, GameVersionDetails, LoginRequest, Platform, Plugin,
    Storefront, kv_store::KvStore,
};
use std::collections::{BTreeMap, HashMap};

mod api;

type Result<T> = std::result::Result<T, Error>;

const GAME_ID: &str = "game_id";
const PRODUCT_ID: &str = "product_id";

pub struct LegacyGames;

#[pdk::export]
impl Plugin for LegacyGames {
    async fn init() -> Result<()> {
        let session = Session::load()?;

        match &session.account {
            Some(account) => {
                api::login(&account.token).await?;
            }
            None => api::giveaway_login(&session.email).await?,
        }

        Ok(())
    }
}

#[pdk::export]
impl Auth for LegacyGames {
    async fn login(request: LoginRequest) -> Result<()> {
        let LoginRequest::Form(form) = request else {
            return Err(Error::auth("expected a login form"));
        };

        let email = form
            .fields
            .get("email")
            .and_then(FieldValue::as_text)
            .ok_or_else(|| Error::auth("email is mandatory"))?
            .to_owned();

        let password = form
            .fields
            .get("password")
            .and_then(FieldValue::as_text)
            .filter(|p| !p.is_empty());

        let account = match password {
            Some(password) => {
                let token = STANDARD.encode(format!("{email}:{password}"));
                let user_id = api::login(&token).await?;
                Some(Account { token, user_id })
            }
            None => {
                api::giveaway_login(&email).await?;
                None
            }
        };

        Session { email, account }.save()
    }

    async fn logout() -> Result<()> {
        Session::clear()
    }
}

#[pdk::export]
impl Storefront for LegacyGames {
    async fn list_games() -> Result<Vec<Game>> {
        let session = Session::load()?;
        let products = owned_products(&session).await?;

        Ok(products.into_iter().flat_map(games).collect())
    }

    async fn list_game_versions(game: Game) -> Result<Vec<GameVersion>> {
        Ok(vec![GameVersion {
            id: game.lookup_id,
            pretty_name: Some(game.name),
            platform: Platform::Windows,
        }])
    }

    async fn game_version_details(game: Game, _version_id: String) -> Result<GameVersionDetails> {
        let external_id = |key: &str| {
            game.external_ids
                .get(key)
                .ok_or_else(|| Error::other(format!("missing `{key}` for {}", game.name)))
        };

        let download_size =
            api::download_size(external_id(PRODUCT_ID)?, external_id(GAME_ID)?).await?;

        Ok(GameVersionDetails {
            install_size: None,
            download_size,
        })
    }
}

struct Session {
    email: String,
    account: Option<Account>,
}

struct Account {
    token: String,
    user_id: u64,
}

impl Session {
    const EMAIL: &str = "email";
    const TOKEN: &str = "token";
    const USER_ID: &str = "user_id";

    fn load() -> Result<Self> {
        let email = KvStore::get(Self::EMAIL)?.ok_or(Error::auth("not logged in"))?;
        let token = KvStore::get(Self::TOKEN)?;
        let user_id = KvStore::get(Self::USER_ID)?;

        Ok(Self {
            email,
            account: token
                .zip(user_id)
                .map(|(token, user_id)| Account { token, user_id }),
        })
    }

    fn save(&self) -> Result<()> {
        KvStore::set(Self::EMAIL, &self.email)?;

        match &self.account {
            Some(account) => {
                KvStore::set(Self::TOKEN, &account.token)?;
                KvStore::set(Self::USER_ID, &account.user_id)?;
            }
            None => {
                KvStore::delete(Self::TOKEN)?;
                KvStore::delete(Self::USER_ID)?;
            }
        }

        Ok(())
    }

    fn clear() -> Result<()> {
        for key in [Self::EMAIL, Self::TOKEN, Self::USER_ID] {
            KvStore::delete(key)?;
        }

        Ok(())
    }
}

async fn owned_products(session: &Session) -> Result<Vec<Product>> {
    let (mut products, catalog, entitlements) = futures::try_join!(
        api::giveaway_products(&session.email),
        api::catalog(),
        async {
            match &session.account {
                Some(account) => api::entitlements(&account.token, account.user_id).await,
                None => Ok(Vec::new()),
            }
        },
    )?;

    let mut catalog: HashMap<u64, Product> = catalog.into_iter().map(|p| (p.id, p)).collect();

    for product in &mut products {
        if let Some(listing) = catalog.get(&product.id) {
            fill_missing_details(product, listing);
        }
    }

    products.extend(
        entitlements
            .iter()
            .filter_map(|entitlement| catalog.remove(&entitlement.product_id)),
    );

    Ok(products)
}

fn fill_missing_details(product: &mut Product, listing: &Product) {
    for game in &mut product.games {
        let Some(listed) = listing
            .games
            .iter()
            .find(|listed| listed.installer_uuid == game.installer_uuid)
        else {
            continue;
        };

        if game.game_name.is_empty() {
            game.game_name.clone_from(&listed.game_name);
        }
        if game.game_description.is_empty() {
            game.game_description.clone_from(&listed.game_description);
        }
    }
}

fn games(product: Product) -> impl Iterator<Item = Game> {
    let product_id = product.id.to_string();

    product.games.into_iter().map(move |game| Game {
        name: game.game_name,
        lookup_id: game.installer_uuid,
        external_ids: BTreeMap::from([
            (GAME_ID.into(), game.game_id),
            (PRODUCT_ID.into(), product_id.clone()),
        ]),
        cover_url: non_empty(game.game_coverart),
        summary: non_empty(game.game_description),
        description: None,
    })
}

fn non_empty(value: String) -> Option<String> {
    Some(value).filter(|value| !value.is_empty())
}
