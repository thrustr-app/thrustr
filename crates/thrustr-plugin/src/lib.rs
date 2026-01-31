pub mod config;
pub mod kv_store;

#[doc(hidden)]
pub mod wit {
    use wit_bindgen::generate;

    generate!({
        world: "storefront",
        pub_export_macro: true,
    });
}

pub use wit::exports::thrustr::storefront::storefront_provider::Error as StorefrontProviderError;
pub use wit::exports::thrustr::storefront::storefront_provider::Guest as Storefront;

#[macro_export]
macro_rules! register_storefront {
    ($plugin_type:ty) => {
        struct Guest;

        impl $crate::wit::exports::thrustr::storefront::storefront_provider::Guest for Guest {
            fn init() -> Result<(), $crate::StorefrontProviderError> {
                <$plugin_type as $crate::Storefront>::init()
            }

            fn auth(url: String, body: Vec<u8>) -> Result<(), $crate::StorefrontProviderError> {
                <$plugin_type as $crate::Storefront>::auth(url, body)
            }
        }

        $crate::wit::export!(Guest with_types_in $crate::wit);
    };
}
