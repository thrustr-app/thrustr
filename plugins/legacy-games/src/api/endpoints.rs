use crate::api::BASE_URL;

pub fn login() -> String {
    format!("{BASE_URL}/users/login")
}

pub fn is_exists_by_email(email: &str) -> String {
    format!("{BASE_URL}/users/isexistsbyemail?email={email}")
}

pub fn entitlements(user_id: u64) -> String {
    format!("{BASE_URL}/users/downloads?userId={user_id}")
}

pub fn get_giveaway_catalog_by_email(email: &str) -> String {
    format!("{BASE_URL}/users/getgiveawaycatalogbyemail?email={email}")
}

pub fn catalog() -> String {
    format!("{BASE_URL}/products/catalog")
}
