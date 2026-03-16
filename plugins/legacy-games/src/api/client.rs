use crate::api::{
    endpoints,
    error::Error,
    models::{
        EntitlementsResponse, IsExistsByEmailResponse, LoginResponse, Product, ProductsResponse,
    },
};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use wstd::http::{Client, Request, request::Builder};

pub async fn giveaway_login(email: &str) -> Result<IsExistsByEmailResponse, Error> {
    send(base_request(&endpoints::is_exists_by_email(email))).await
}

pub async fn login(token: &str) -> Result<LoginResponse, Error> {
    send(authenticated_request(&endpoints::login(), token)).await
}

pub async fn get_products(
    email: &str,
    token: Option<&str>,
    user_id: Option<u64>,
) -> Result<Vec<Product>, Error> {
    let (giveaway_response, catalog, entitlements) =
        futures::try_join!(get_giveaway_catalog(email), get_catalog(), async {
            match token.zip(user_id) {
                Some((token, user_id)) => get_entitlements(token, user_id).await.map(Some),
                None => Ok(None),
            }
        })?;

    let mut products: Vec<Product> = giveaway_response.into_result()?;
    let mut catalog: HashMap<u64, Product> = catalog.into_iter().map(|p| (p.id, p)).collect();

    for product in &mut products {
        if let Some(catalog_product) = catalog.get(&product.id) {
            for game in &mut product.games {
                if game.game_name.is_empty() || game.game_description.is_empty() {
                    if let Some(cg) = catalog_product
                        .games
                        .iter()
                        .find(|cg| cg.installer_uuid == game.installer_uuid)
                    {
                        if game.game_name.is_empty() {
                            game.game_name = cg.game_name.clone();
                        }
                        if game.game_description.is_empty() {
                            game.game_description = cg.game_description.clone();
                        }
                    }
                }
            }
        }
    }

    if let Some(entitlements) = entitlements {
        products.extend(
            entitlements
                .into_result()?
                .into_iter()
                .filter_map(|d| catalog.remove(&d.product_id)),
        );
    }

    Ok(products)
}

async fn get_catalog() -> Result<Vec<Product>, Error> {
    send(base_request(&endpoints::catalog())).await
}

async fn get_entitlements(token: &str, user_id: u64) -> Result<EntitlementsResponse, Error> {
    send(authenticated_request(
        &endpoints::entitlements(user_id),
        token,
    ))
    .await
}

async fn get_giveaway_catalog(email: &str) -> Result<ProductsResponse, Error> {
    send(base_request(&endpoints::get_giveaway_catalog_by_email(
        email,
    )))
    .await
}

async fn send<T: DeserializeOwned>(builder: Builder) -> Result<T, Error> {
    Ok(Client::new()
        .send(builder.body(()).unwrap())
        .await?
        .into_body()
        .json()
        .await?)
}

fn base_request(url: &str) -> Builder {
    Request::builder()
        .uri(url)
        .header("Authorization", "?token?")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Cache-Control", "no-cache")
}

fn authenticated_request(url: &str, token: &str) -> Builder {
    base_request(url).header("UserToken", format!("Basic {token}"))
}
