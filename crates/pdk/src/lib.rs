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
pub use wit::thrustr::plugin::types::{Error, Game};

pub trait Plugin {
    fn init() -> Result<(), Error>;
    fn get_login_flow() -> Result<Option<AuthFlow>, Error>;
    fn get_logout_flow() -> Result<Option<AuthFlow>, Error>;
    fn login(
        url: Option<String>,
        body: Option<String>,
        fields: Option<HashMap<String, String>>,
    ) -> Result<(), Error>;
    fn logout() -> Result<(), Error>;
    fn validate_config(fields: HashMap<String, String>) -> Result<(), Error>;
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
            fn list_games() -> Result<Vec<$crate::Game>, $crate::Error> {
                <$plugin_type as $crate::Storefront>::list_games()
            }
        }

        $crate::wit::export!(Guest with_types_in $crate::wit);
    };
}
