pub mod config;
pub mod kv_store;

#[doc(hidden)]
pub mod wit {
    wit_bindgen::generate!({
        world: "storefront-provider-plugin",
        pub_export_macro: true,
    });
}

pub use wit::exports::thrustr::plugin::base::Error as PluginError;
pub use wit::exports::thrustr::plugin::base::Guest as Plugin;

pub use wit::exports::thrustr::plugin::storefront_provider::Guest as StorefrontProvider;

#[macro_export]
macro_rules! register_storefront_provider {
    ($plugin_type:ty) => {
        struct Guest;

        impl $crate::wit::exports::thrustr::plugin::storefront_provider::Guest for Guest {
            fn test() -> Result<(), $crate::PluginError> {
                <$plugin_type as $crate::StorefrontProvider>::test()
            }
        }

        impl $crate::wit::exports::thrustr::plugin::base::Guest for Guest {
            fn init() -> Result<(), $crate::PluginError> {
                <$plugin_type as $crate::Plugin>::init()
            }
        }

        $crate::wit::export!(Guest with_types_in $crate::wit);
    };
}
