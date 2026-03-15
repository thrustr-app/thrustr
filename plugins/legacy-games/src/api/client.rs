use crate::api::{
    USER_TOKEN_HEADER, endpoints,
    error::Error,
    models::{
        EntitlementsResponse, IsExistsByEmailResponse, LoginResponse, Product, ProductsResponse,
    },
};
use golem_wasi_http::{
    Client, RequestBuilder,
    header::{ACCEPT, AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE},
};
use std::collections::HashMap;

pub fn giveaway_login(email: &str) -> Result<IsExistsByEmailResponse, Error> {
    Ok(base_request(endpoints::is_exists_by_email(email))
        .send()?
        .json()?)
}

pub fn login(token: &str) -> Result<LoginResponse, Error> {
    Ok(authenticated_request(endpoints::login(), token)
        .send()?
        .json()?)
}

pub fn get_products(
    email: &str,
    token: Option<&str>,
    user_id: Option<u64>,
) -> Result<Vec<Product>, Error> {
    let mut products = get_giveaway_catalog(email)?.into_result()?;

    let mut catalog: HashMap<u64, Product> =
        get_catalog()?.into_iter().map(|p| (p.id, p)).collect();

    for product in &mut products {
        if let Some(catalog_product) = catalog.get(&product.id) {
            for game in &mut product.games {
                if game.game_name.is_empty() || game.game_description.is_empty() {
                    if let Some(catalog_game) = catalog_product
                        .games
                        .iter()
                        .find(|cg| cg.installer_uuid == game.installer_uuid)
                    {
                        if game.game_name.is_empty() {
                            game.game_name = catalog_game.game_name.clone();
                        }
                        if game.game_description.is_empty() {
                            game.game_description = catalog_game.game_description.clone();
                        }
                    }
                }
            }
        }
    }

    if let Some(token) = token
        && let Some(user_id) = user_id
    {
        products.extend(
            get_entitlements(token, user_id)?
                .into_result()?
                .into_iter()
                .filter_map(|d| catalog.remove(&d.product_id)),
        );
    }

    Ok(products)
}

fn get_catalog() -> Result<Vec<Product>, Error> {
    Ok(base_request(endpoints::catalog()).send()?.json()?)
}

fn get_entitlements(token: &str, user_id: u64) -> Result<EntitlementsResponse, Error> {
    Ok(
        authenticated_request(endpoints::entitlements(user_id), token)
            .send()?
            .json()?,
    )
}

fn get_giveaway_catalog(email: &str) -> Result<ProductsResponse, Error> {
    Ok(
        base_request(endpoints::get_giveaway_catalog_by_email(email))
            .send()?
            .json()?,
    )
}

fn base_request(url: String) -> RequestBuilder {
    Client::new()
        .get(url)
        .header(AUTHORIZATION, "?token?")
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .header(CACHE_CONTROL, "no-cache")
}

fn authenticated_request(url: String, token: &str) -> RequestBuilder {
    base_request(url).header(USER_TOKEN_HEADER, format!("Basic {token}"))
}
