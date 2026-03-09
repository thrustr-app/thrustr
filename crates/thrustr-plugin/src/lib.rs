pub mod config;
pub mod kv_store;

#[doc(hidden)]
pub mod wit {
    wit_bindgen::generate!({
        world: "storefront-plugin",
        pub_export_macro: true,
    });
}

pub use wit::exports::thrustr::plugin::base::{AuthFlow, Error as PluginError, Guest as Plugin};

pub use wit::exports::thrustr::plugin::storefront::Guest as Storefront;

#[macro_export]
macro_rules! register_storefront {
    ($plugin_type:ty) => {
        struct Guest;

        impl $crate::wit::exports::thrustr::plugin::storefront::Guest for Guest {
            fn test() -> Result<(), $crate::PluginError> {
                <$plugin_type as $crate::Storefront>::test()
            }
        }

        impl $crate::wit::exports::thrustr::plugin::base::Guest for Guest {
            fn init() -> Result<(), $crate::PluginError> {
                <$plugin_type as $crate::Plugin>::init()
            }

            fn get_login_flow() -> Result<Option<$crate::AuthFlow>, $crate::PluginError> {
                <$plugin_type as $crate::Plugin>::get_login_flow()
            }

            fn get_logout_flow() -> Result<Option<$crate::AuthFlow>, $crate::PluginError> {
                <$plugin_type as $crate::Plugin>::get_logout_flow()
            }

            fn login(url: String, body: String) -> Result<(), $crate::PluginError> {
                <$plugin_type as $crate::Plugin>::login(url, body)
            }

            fn logout(url: String, body: String) -> Result<(), $crate::PluginError> {
                <$plugin_type as $crate::Plugin>::logout(url, body)
            }

            fn validate_config(fields: Vec<(String, String)>) -> Result<(), $crate::PluginError> {
                <$plugin_type as $crate::Plugin>::validate_config(fields)
            }
        }

        $crate::wit::export!(Guest with_types_in $crate::wit);
    };
}
