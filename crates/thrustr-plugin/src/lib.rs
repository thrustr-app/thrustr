use wit_bindgen::generate;

generate!({
    world: "storefront-plugin",
    pub_export_macro: true,
});

#[macro_export]
macro_rules! register_storefront {
    ($plugin_type:ty) => {
        struct Guest;

        impl $crate::exports::thrustr::storefront::storefront::Guest for Guest {
            fn init() -> Result<(), String> {
                <$plugin_type as $crate::Storefront>::init()
            }
        }

        $crate::export!(Guest with_types_in $crate);
    };
}

pub use exports::thrustr::storefront::storefront::Guest as Storefront;
