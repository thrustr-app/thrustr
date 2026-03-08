pub mod config;
pub mod kv_store;

#[doc(hidden)]
pub mod wit {
    wit_bindgen::generate!({
        world: "storefront-plugin",
        pub_export_macro: true,
    });
}

pub use wit::exports::thrustr::plugin::base::Error as PluginError;
pub use wit::exports::thrustr::plugin::base::Guest as Plugin;

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

            fn get_auth_url() -> Result<Option<String>, $crate::PluginError> {
                <$plugin_type as $crate::Plugin>::get_auth_url()
            }

            fn validate_config(fields: Vec<(String, String)>) -> Result<(), $crate::PluginError> {
                <$plugin_type as $crate::Plugin>::validate_config(fields)
            }
        }

        $crate::wit::export!(Guest with_types_in $crate::wit);
    };
}
