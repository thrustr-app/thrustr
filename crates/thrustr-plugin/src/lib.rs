mod kv_store;

#[doc(hidden)]
pub mod wit {
    use wit_bindgen::generate;

    generate!({
        world: "storefront",
        pub_export_macro: true,
    });
}

pub use kv_store::KvStore;
pub use wit::exports::thrustr::storefront::storefront_provider::Guest as Storefront;

#[macro_export]
macro_rules! register_storefront {
    ($plugin_type:ty) => {
        struct Guest;

        impl $crate::wit::exports::thrustr::storefront::storefront_provider::Guest for Guest {
            fn init() -> Result<(), String> {
                <$plugin_type as $crate::Storefront>::init()
            }
        }

        $crate::wit::export!(Guest with_types_in $crate::wit);
    };
}
