use crate::api::error::Error as ApiError;
use pdk::Error;

impl From<ApiError> for Error {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::InvalidCredentials => Error::Auth("Invalid credentials.".into()),
            ApiError::UserNotFound => Error::Auth("User not found.".into()),
            ApiError::Request(e) => Error::Other(e.to_string()),
            ApiError::Other(e) => Error::Other(e.to_string()),
        }
    }
}
