//! Rewrites `impl pdk::Config for T { #[options("id")] fn f() … }` into:
//!
//! - An inherent impl of `T` containing the option functions.
//! - The `pdk::Config` implementation.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use std::{collections::HashMap, mem};
use syn::{Attribute, Error, Ident, ImplItem, ImplItemFn, ItemImpl, LitStr, parse_quote};

struct Provider {
    field: LitStr,
    ident: Ident,
    is_async: bool,
}

/// Invalid option functions are reported and omitted so the rest of the
/// expansion still type-checks instead of cascading errors.
pub(crate) fn expand(mut item: ItemImpl) -> TokenStream {
    let ty = item.self_ty.clone();

    let mut errors = Vec::new();
    let mut functions = Vec::new();
    let mut providers = Vec::new();
    let mut provided = HashMap::new();
    item.items = mem::take(&mut item.items)
        .into_iter()
        .filter_map(|item| {
            let ImplItem::Fn(mut function) = item else {
                return Some(item);
            };
            let attrs: Vec<_> = function
                .attrs
                .extract_if(.., |attr| attr.path().is_ident("options"))
                .collect();
            if attrs.is_empty() {
                return Some(ImplItem::Fn(function));
            }
            match provider(&function, &attrs, &mut provided) {
                Ok(provider) => providers.push(provider),
                Err(error) => errors.push(error),
            }
            functions.push(function);
            None
        })
        .collect();

    let arms = providers.iter().map(|provider| {
        let Provider {
            field,
            ident,
            is_async,
        } = provider;
        let await_ = is_async.then(|| quote! { .await });
        let call = quote_spanned! { ident.span() => Self::#ident() #await_ };
        quote! {
            #field => {
                let options: ::std::result::Result<
                    ::std::vec::Vec<::pdk::ComboboxOption>,
                    ::pdk::Error,
                > = #call;
                options
            }
        }
    });
    item.items.push(parse_quote! {
        async fn options(
            field_id: ::std::string::String,
        ) -> ::std::result::Result<::std::vec::Vec<::pdk::ComboboxOption>, ::pdk::Error> {
            match field_id.as_str() {
                #(#arms,)*
                _ => ::std::result::Result::Err(::pdk::__private::no_options(&field_id)),
            }
        }
    });

    let errors = errors.into_iter().map(Error::into_compile_error);
    quote! {
        #(#errors)*

        impl #ty {
            #(#functions)*
        }

        #item
    }
}

fn provider(
    function: &ImplItemFn,
    attrs: &[Attribute],
    provided: &mut HashMap<String, Ident>,
) -> syn::Result<Provider> {
    let attr = match attrs {
        [attr] => attr,
        [_, duplicate, ..] => {
            return Err(Error::new_spanned(
                duplicate,
                "a function can only have one `#[options]`",
            ));
        }
        [] => unreachable!("providers should have an `#[options]`"),
    };
    let field: LitStr = attr.parse_args().map_err(|e| {
        Error::new(
            e.span(),
            "expected a field id, e.g. `#[options(\"collections\")]`",
        )
    })?;
    if field.value().is_empty() {
        return Err(Error::new_spanned(&field, "the field id cannot be empty"));
    }

    let sig = &function.sig;
    if !sig.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &sig.generics,
            "option functions cannot be generic",
        ));
    }
    if !sig.inputs.is_empty() {
        return Err(Error::new_spanned(
            &sig.inputs,
            "option functions cannot take parameters",
        ));
    }

    if let Some(owner) = provided.insert(field.value(), sig.ident.clone()) {
        return Err(Error::new_spanned(
            &field,
            format!("`{}` already has options from `{owner}`", field.value()),
        ));
    }

    Ok(Provider {
        field,
        ident: sig.ident.clone(),
        is_async: sig.asyncness.is_some(),
    })
}
