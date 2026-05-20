use crate::api::error::Error as ApiError;
use pdk::Error;

impl From<ApiError> for Error {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::InvalidCredentials => Error::auth("Invalid credentials."),
            ApiError::UserNotFound => Error::auth("User not found."),
            ApiError::Request(e) => Error::other(e.to_string()),
            ApiError::Other(e) => Error::other(e),
        }
    }
}
