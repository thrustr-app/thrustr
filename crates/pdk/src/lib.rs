use std::collections::BTreeMap;

pub mod config;
pub mod kv_store;

#[doc(hidden)]
pub mod wit {
    pub mod plugin {
        wit_bindgen::generate!({
            world: "plugin",
            pub_export_macro: true,
            export_macro_name: "export_plugin",
        });
    }

    pub mod auth {
        wit_bindgen::generate!({
            world: "auth-capability",
            pub_export_macro: true,
            export_macro_name: "export_auth",
            with: {
                "thrustr:plugin/types@0.1.0": crate::wit::plugin::thrustr::plugin::types,
            },
        });
    }

    pub mod config {
        wit_bindgen::generate!({
            world: "config-capability",
            pub_export_macro: true,
            export_macro_name: "export_config",
            with: {
                "thrustr:plugin/types@0.1.0": crate::wit::plugin::thrustr::plugin::types,
            },
        });
    }

    pub mod storefront {
        wit_bindgen::generate!({
            world: "storefront-capability",
            pub_export_macro: true,
            export_macro_name: "export_storefront",
            with: {
                "thrustr:plugin/types@0.1.0": crate::wit::plugin::thrustr::plugin::types,
            },
        });
    }
}

pub use pdk_macros::export;
pub use wit::auth::exports::thrustr::plugin::auth::{AuthFlow, LoginFlow, LoginForm, LoginRequest};
pub use wit::plugin::thrustr::plugin::types::{Error, Game, GameVersion, Platform};

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

#[doc(hidden)]
pub mod __private {
    pub struct Plugin;
    pub struct Auth;
    pub struct Config;
    pub struct Storefront;

    #[diagnostic::on_unimplemented(
        message = "`{Self}` implements a pdk trait that is not exported",
        label = "missing `#[pdk::export]`",
        note = "add `#[pdk::export]` above this `impl` block"
    )]
    pub trait Exported<Capability> {}
}

pub trait Plugin: __private::Exported<__private::Plugin> {
    fn init() -> impl Future<Output = Result<(), Error>>;
}

pub trait Auth: Plugin + __private::Exported<__private::Auth> {
    fn login_flow() -> impl Future<Output = Result<Option<AuthFlow>, Error>> {
        async { Ok(None) }
    }

    fn logout_flow() -> impl Future<Output = Result<Option<AuthFlow>, Error>> {
        async { Ok(None) }
    }

    fn login(request: LoginRequest) -> impl Future<Output = Result<(), Error>>;

    fn logout() -> impl Future<Output = Result<(), Error>> {
        async { Ok(()) }
    }
}

#[allow(unused_variables)]
pub trait Config: Plugin + __private::Exported<__private::Config> {
    fn validate(fields: BTreeMap<String, String>) -> impl Future<Output = Result<(), Error>> {
        async { Ok(()) }
    }
}

pub trait Storefront: Plugin + __private::Exported<__private::Storefront> {
    fn list_games() -> impl Future<Output = Result<Vec<Game>, Error>>;

    fn list_game_versions(game: Game) -> impl Future<Output = Result<Vec<GameVersion>, Error>>;
}

#[doc(hidden)]
#[macro_export]
macro_rules! __export {
    (Plugin, $ty:ty) => {
        const _: () = {
            impl $crate::__private::Exported<$crate::__private::Plugin> for $ty {}

            struct Guest;

            impl $crate::wit::plugin::exports::thrustr::plugin::base::Guest for Guest {
                async fn init() -> Result<(), $crate::Error> {
                    <$ty as $crate::Plugin>::init().await
                }
            }

            $crate::wit::plugin::export_plugin! { Guest with_types_in $crate::wit::plugin }
        };
    };
    (Auth, $ty:ty) => {
        const _: () = {
            impl $crate::__private::Exported<$crate::__private::Auth> for $ty {}

            struct Guest;

            impl $crate::wit::auth::exports::thrustr::plugin::auth::Guest for Guest {
                async fn get_login_flow() -> Result<Option<$crate::AuthFlow>, $crate::Error> {
                    <$ty as $crate::Auth>::login_flow().await
                }
                async fn get_logout_flow() -> Result<Option<$crate::AuthFlow>, $crate::Error> {
                    <$ty as $crate::Auth>::logout_flow().await
                }
                async fn login(request: $crate::LoginRequest) -> Result<(), $crate::Error> {
                    <$ty as $crate::Auth>::login(request).await
                }
                async fn logout() -> Result<(), $crate::Error> {
                    <$ty as $crate::Auth>::logout().await
                }
            }

            $crate::wit::auth::export_auth! { Guest with_types_in $crate::wit::auth }
        };
    };
    (Config, $ty:ty) => {
        const _: () = {
            impl $crate::__private::Exported<$crate::__private::Config> for $ty {}

            struct Guest;

            impl $crate::wit::config::exports::thrustr::plugin::config::Guest for Guest {
                async fn validate(
                    fields: ::std::collections::BTreeMap<String, String>,
                ) -> Result<(), $crate::Error> {
                    <$ty as $crate::Config>::validate(fields).await
                }
            }

            $crate::wit::config::export_config! { Guest with_types_in $crate::wit::config }
        };
    };
    (Storefront, $ty:ty) => {
        const _: () = {
            impl $crate::__private::Exported<$crate::__private::Storefront> for $ty {}

            struct Guest;

            impl $crate::wit::storefront::exports::thrustr::plugin::storefront::Guest for Guest {
                async fn get_games() -> Result<Vec<$crate::Game>, $crate::Error> {
                    <$ty as $crate::Storefront>::list_games().await
                }
                async fn get_game_versions(
                    game: $crate::Game,
                ) -> Result<Vec<$crate::GameVersion>, $crate::Error> {
                    <$ty as $crate::Storefront>::list_game_versions(game).await
                }
            }

            $crate::wit::storefront::export_storefront! { Guest with_types_in $crate::wit::storefront }
        };
    };
    ($other:ident, $ty:ty) => {
        compile_error!(concat!(
            "`#[pdk::export]` does not support trait `",
            stringify!($other),
            "`"
        ));
    };
}
