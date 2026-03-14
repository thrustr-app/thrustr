use crate::api::BASE_URL;

pub fn is_exists_by_email(email: &str) -> String {
    format!("{BASE_URL}/users/isexistsbyemail?email={email}")
}

pub fn login() -> String {
    format!("{BASE_URL}/users/login")
}
