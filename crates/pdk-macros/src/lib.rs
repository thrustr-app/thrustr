use proc_macro::TokenStream;
use quote::quote;
use syn::{Error, ItemImpl, parse_macro_input};

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
    expand(&item)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand(item: &ItemImpl) -> syn::Result<proc_macro2::TokenStream> {
    let Some((None, path, _)) = &item.trait_ else {
        return Err(Error::new_spanned(
            item.impl_token,
            "`#[pdk::export]` must be placed on `impl pdk::<Trait> for <Type>`",
        ));
    };
    if !item.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &item.generics,
            "generic plugin types are not supported",
        ));
    }

    let capability = &path
        .segments
        .last()
        .expect("trait path should have a segment")
        .ident;
    let ty = &item.self_ty;

    Ok(quote! {
        #item
        ::pdk::__export!(#capability, #ty);
    })
}
