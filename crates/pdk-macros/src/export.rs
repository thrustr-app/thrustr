use crate::scheduler;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, ItemImpl};

pub(crate) fn expand(item: ItemImpl) -> syn::Result<TokenStream> {
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

    let capability = path
        .segments
        .last()
        .expect("trait path should have a segment")
        .ident
        .clone();
    let ty = item.self_ty.clone();

    let item = match capability == "Scheduler" {
        true => scheduler::expand(item),
        false => quote! { #item },
    };

    Ok(quote! {
        #item
        ::pdk::__export!(#capability, #ty);
    })
}
