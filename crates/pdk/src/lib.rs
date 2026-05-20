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

pub use wit::exports::thrustr::plugin::base::AuthFlow;
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
    fn init() -> Result<(), Error>;

    fn get_login_flow() -> Result<Option<AuthFlow>, Error> {
        Ok(None)
    }

    fn get_logout_flow() -> Result<Option<AuthFlow>, Error> {
        Ok(None)
    }

    fn login(
        url: Option<String>,
        body: Option<String>,
        fields: Option<HashMap<String, String>>,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn logout() -> Result<(), Error> {
        Ok(())
    }

    fn validate_config(fields: HashMap<String, String>) -> Result<(), Error> {
        Ok(())
    }
}

#[macro_export]
macro_rules! register_storefront {
    ($plugin_type:ty) => {
        struct Guest;
        impl $crate::wit::exports::thrustr::plugin::base::Guest for Guest {
            fn init() -> Result<(), $crate::Error> {
                <$plugin_type as $crate::Plugin>::init()
            }
            fn get_login_flow() -> Result<Option<$crate::AuthFlow>, $crate::Error> {
                <$plugin_type as $crate::Plugin>::get_login_flow()
            }
            fn get_logout_flow() -> Result<Option<$crate::AuthFlow>, $crate::Error> {
                <$plugin_type as $crate::Plugin>::get_logout_flow()
            }
            fn login(
                url: Option<String>,
                body: Option<String>,
                fields: Option<Vec<(String, String)>>,
            ) -> Result<(), $crate::Error> {
                let fields = fields.map(|v| v.into_iter().collect::<std::collections::HashMap<_, _>>());
                <$plugin_type as $crate::Plugin>::login(url, body, fields)
            }
            fn logout() -> Result<(), $crate::Error> {
                <$plugin_type as $crate::Plugin>::logout()
            }
            fn validate_config(fields: Vec<(String, String)>) -> Result<(), $crate::Error> {
                let fields = fields.into_iter().collect::<std::collections::HashMap<_, _>>();
                <$plugin_type as $crate::Plugin>::validate_config(fields)
            }
        }

        impl $crate::wit::exports::thrustr::plugin::storefront::Guest for Guest {
            fn get_games() -> Result<Vec<$crate::Game>, $crate::Error> {
                <$plugin_type as $crate::Storefront>::get_games()
            }

            fn get_game_versions(game: $crate::Game) -> Result<Vec<$crate::GameVersion>, $crate::Error> {
                <$plugin_type as $crate::Storefront>::get_game_versions(game)
            }
        }

        $crate::wit::export!(Guest with_types_in $crate::wit);
    };
}
