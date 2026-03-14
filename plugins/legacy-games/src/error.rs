use crate::api::error::Error as ApiError;
use thrustr_plugin::Error;

impl From<ApiError> for Error {
    fn from(e: ApiError) -> Self {
        Error::Auth(e.to_string())
    }
}
