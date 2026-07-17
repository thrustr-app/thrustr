use std::collections::HashMap;

pub mod config;
pub mod kv_store;

#[doc(hidden)]
pub mod wit {
    wit_bindgen::generate!({
        world: "storefront-plugin",
        pub_export_macro: true,
    });
}

pub use wit::exports::thrustr::plugin::base::{AuthFlow, LoginRequest};
pub use wit::exports::thrustr::plugin::storefront::Guest as Storefront;
pub use wit::thrustr::plugin::types::{Error, Game, GameVersion, Platform};

impl Error {
    pub fn auth(message: impl Into<String>) -> Self {
        Error::Auth(message.into())
    }

    pub fn config(message: impl Into<String>) -> Self {
        Error::Config(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Error::Other(message.into())
    }
}

#[allow(unused_variables)]
pub trait Plugin {
    fn init() -> impl Future<Output = Result<(), Error>>;

    fn get_login_flow() -> impl Future<Output = Result<Option<AuthFlow>, Error>> {
        async { Ok(None) }
    }

    fn get_logout_flow() -> impl Future<Output = Result<Option<AuthFlow>, Error>> {
        async { Ok(None) }
    }

    fn login(request: LoginRequest) -> impl Future<Output = Result<(), Error>> {
        async { Ok(()) }
    }

    fn logout() -> impl Future<Output = Result<(), Error>> {
        async { Ok(()) }
    }

    fn validate_config(fields: HashMap<String, String>) -> impl Future<Output = Result<(), Error>> {
        async { Ok(()) }
    }
}

#[macro_export]
macro_rules! register_storefront {
    ($plugin_type:ty) => {
        struct Guest;
        impl $crate::wit::exports::thrustr::plugin::base::Guest for Guest {
            async fn init() -> Result<(), $crate::Error> {
                <$plugin_type as $crate::Plugin>::init().await
            }
            async fn get_login_flow() -> Result<Option<$crate::AuthFlow>, $crate::Error> {
                <$plugin_type as $crate::Plugin>::get_login_flow().await
            }
            async fn get_logout_flow() -> Result<Option<$crate::AuthFlow>, $crate::Error> {
                <$plugin_type as $crate::Plugin>::get_logout_flow().await
            }
            async fn login(
                request: $crate::LoginRequest,
            ) -> Result<(), $crate::Error> {
                <$plugin_type as $crate::Plugin>::login(request).await
            }
            async fn logout() -> Result<(), $crate::Error> {
                <$plugin_type as $crate::Plugin>::logout().await
            }
            async fn validate_config(fields: Vec<(String, String)>) -> Result<(), $crate::Error> {
                let fields = fields.into_iter().collect::<std::collections::HashMap<_, _>>();
                <$plugin_type as $crate::Plugin>::validate_config(fields).await
            }
        }

        impl $crate::wit::exports::thrustr::plugin::storefront::Guest for Guest {
            async fn get_games() -> Result<Vec<$crate::Game>, $crate::Error> {
                <$plugin_type as $crate::Storefront>::get_games().await
            }

            async fn get_game_versions(game: $crate::Game) -> Result<Vec<$crate::GameVersion>, $crate::Error> {
                <$plugin_type as $crate::Storefront>::get_game_versions(game).await
            }
        }

        $crate::wit::export!(Guest with_types_in $crate::wit);
    };
}
