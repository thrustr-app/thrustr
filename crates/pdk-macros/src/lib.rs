use proc_macro::TokenStream;
use syn::{Error, ItemImpl, parse_macro_input};

mod export;
mod scheduler;

/// Exports a pdk trait implementation to the host.
///
/// ```ignore
/// #[pdk::export]
/// impl pdk::Plugin for MyPlugin { … }
///
/// #[pdk::export]
/// impl pdk::Storefront for MyPlugin { … }
/// ```
#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        let attr = proc_macro2::TokenStream::from(attr);
        return Error::new_spanned(attr, "`#[pdk::export]` takes no arguments")
            .to_compile_error()
            .into();
    }

    let item = parse_macro_input!(item as ItemImpl);
    export::expand(item)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}
