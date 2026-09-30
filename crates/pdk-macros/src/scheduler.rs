//! Rewrites `impl pdk::Scheduler for T { fn task() … }` into:
//!
//! - An inherent impl of `T` containing the task functions.
//! - `T::schedule_<task>`, `T::schedule_<task>_every`, and
//!   `T::cancel_<task>` methods for tasks without `#[every(...)]`.
//! - A private task enum and the `pdk::Scheduler` implementation used by the
//!   host to identify, schedule and run tasks.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use std::collections::HashMap;
use syn::{
    Attribute, Error, FnArg, Ident, ImplItem, ImplItemFn, ItemImpl, LitInt, LitStr, Pat, Type,
    ext::IdentExt, spanned::Spanned,
};

struct Task {
    ident: Ident,
    is_async: bool,
    params: Vec<Param>,
    name: LitStr,
    timing: Timing,
}

enum Timing {
    /// `#[every(…)]`: the host runs the task every this many seconds.
    Every(u64),
    Dynamic {
        schedule: Ident,
        schedule_every: Ident,
        cancel: Ident,
    },
}

const UNITS: [(&str, u64); 3] = [("hours", 3600), ("mins", 60), ("secs", 1)];

struct Param {
    name: Ident,
    /// The type of the parameter on the generated methods.
    ty: TokenStream,
    /// The owned type used when decoding the argument.
    owned: TokenStream,
    /// Whether the task borrows the decoded value instead of taking ownership.
    by_ref: bool,
    span: Span,
}

/// Invalid tasks are reported and omitted so the rest of the expansion
/// still type-checks instead of cascading errors.
pub(crate) fn expand(item: ItemImpl) -> TokenStream {
    let (_, path, _) = item.trait_.expect("export should pass a trait impl");
    let ty = item.self_ty;

    let mut errors = Vec::new();
    let mut functions = Vec::with_capacity(item.items.len());
    for item in item.items {
        match item {
            ImplItem::Fn(function) => functions.push(function),
            item => errors.push(Error::new_spanned(
                item,
                "`impl Scheduler` may only contain task functions",
            )),
        }
    }

    // Every function name the expansion defines on `T`, and the task that
    // defines it.
    let mut defined: HashMap<String, String> = functions
        .iter()
        .map(|function| {
            let name = function.sig.ident.unraw().to_string();
            (name.clone(), name)
        })
        .collect();
    let mut tasks = Vec::with_capacity(functions.len());
    for function in &mut functions {
        match task(function, &mut defined) {
            Ok(task) => tasks.push(task),
            Err(error) => errors.push(error),
        }
    }

    let errors = errors.into_iter().map(Error::into_compile_error);
    let variants = tasks.iter().map(|task| &task.ident);
    let to_name = tasks.iter().map(|Task { ident, name, .. }| {
        quote! { Self::#ident => #name }
    });
    let from_name = tasks.iter().map(|Task { ident, name, .. }| {
        quote! { #name => ::std::option::Option::Some(Self::#ident) }
    });
    let methods = tasks.iter().map(methods);
    let periodic = tasks.iter().filter_map(|task| match task.timing {
        Timing::Every(secs) => {
            let variant = &task.ident;
            Some(quote! { (Task::#variant, ::std::time::Duration::from_secs(#secs)) })
        }
        Timing::Dynamic { .. } => None,
    });
    let dispatch = tasks.iter().map(dispatch);

    quote! {
        #(#errors)*

        impl #ty {
            #(#functions)*
        }

        // Scoped so `Task` doesn't conflict with the plugin's own items.
        const _: () = {
            #[allow(non_camel_case_types)]
            #[derive(Clone, Copy)]
            pub enum Task {
                #(#variants,)*
            }

            impl ::pdk::__private::Task for Task {
                fn name(&self) -> &str {
                    match *self {
                        #(#to_name,)*
                    }
                }

                fn from_name(name: &str) -> ::std::option::Option<Self> {
                    match name {
                        #(#from_name,)*
                        _ => ::std::option::Option::None,
                    }
                }
            }

            impl #ty {
                #(#methods)*
            }

            impl #path for #ty {
                type Task = Task;

                fn periodic() -> ::std::vec::Vec<(Task, ::std::time::Duration)> {
                    ::std::vec![#(#periodic),*]
                }

                async fn run(
                    task: Task,
                    args: ::std::vec::Vec<u8>,
                ) -> ::std::result::Result<(), ::pdk::Error> {
                    match task {
                        #(#dispatch,)*
                    }
                }
            }
        };
    }
}

/// Removes `#[every(…)]` from `function`, since the function is emitted even
/// when it isn't a valid task.
fn task(function: &mut ImplItemFn, defined: &mut HashMap<String, String>) -> syn::Result<Task> {
    let every: Vec<_> = function
        .attrs
        .extract_if(.., |attr| attr.path().is_ident("every"))
        .collect();

    let sig = &function.sig;
    if !sig.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &sig.generics,
            "task functions cannot be generic",
        ));
    }

    let params: Vec<_> = sig
        .inputs
        .iter()
        .enumerate()
        .map(|(index, input)| param(index, input))
        .collect::<syn::Result<_>>()?;

    let name = sig.ident.unraw().to_string();
    let span = sig.ident.span();
    let timing = match every.as_slice() {
        [] => {
            let schedule = format_ident!("schedule_{name}", span = span);
            let schedule_every = format_ident!("schedule_{name}_every", span = span);
            let cancel = format_ident!("cancel_{name}", span = span);

            for generated in [&schedule, &schedule_every, &cancel] {
                if let Some(owner) = defined.insert(generated.to_string(), name.clone()) {
                    return Err(Error::new_spanned(
                        &sig.ident,
                        format!(
                            "task `{name}` would define `{generated}`, which `{owner}` already defines"
                        ),
                    ));
                }
            }
            Timing::Dynamic {
                schedule,
                schedule_every,
                cancel,
            }
        }
        [attr] if !params.is_empty() => {
            return Err(Error::new_spanned(
                attr,
                format!(
                    "`#[every]` tasks can't take parameters. To run `{name}` periodically \
                     with arguments, remove `#[every]` and call \
                     `Self::schedule_{name}_every(interval, …)` instead"
                ),
            ));
        }
        [attr] => Timing::Every(interval(attr)?),
        [_, duplicate, ..] => {
            return Err(Error::new_spanned(
                duplicate,
                "a task can only have one `#[every]`",
            ));
        }
    };

    Ok(Task {
        ident: sig.ident.clone(),
        is_async: sig.asyncness.is_some(),
        params,
        name: LitStr::new(&name, span),
        timing,
    })
}

fn interval(attr: &Attribute) -> syn::Result<u64> {
    let mut seen = [false; UNITS.len()];
    let mut secs = 0u64;
    attr.parse_nested_meta(|meta| {
        let Some(index) = UNITS.iter().position(|(unit, _)| meta.path.is_ident(unit)) else {
            return Err(meta.error("expected `hours`, `mins` or `secs`"));
        };
        let (unit, scale) = UNITS[index];
        if std::mem::replace(&mut seen[index], true) {
            return Err(meta.error(format!("`{unit}` is already set")));
        }
        let value: LitInt = meta.value()?.parse()?;
        secs = value
            .base10_parse::<u64>()?
            .checked_mul(scale)
            .and_then(|value| secs.checked_add(value))
            .ok_or_else(|| Error::new_spanned(&value, "`#[every]` interval is too long"))?;
        Ok(())
    })?;

    if secs == 0 {
        return Err(Error::new_spanned(
            attr,
            "`#[every]` needs a non-zero interval, e.g. `#[every(mins = 30)]`",
        ));
    }
    Ok(secs)
}

fn param(index: usize, input: &FnArg) -> syn::Result<Param> {
    let FnArg::Typed(input) = input else {
        return Err(Error::new_spanned(
            input,
            "task functions cannot take `self`",
        ));
    };

    let name = match &*input.pat {
        Pat::Ident(pat) if pat.subpat.is_none() => pat.ident.clone(),
        _ => Ident::new(&format!("arg{index}"), Span::mixed_site()),
    };
    let span = input.ty.span();
    let (ty, owned, by_ref) = match &*input.ty {
        Type::Reference(reference) if reference.mutability.is_some() => {
            return Err(Error::new_spanned(
                reference,
                "task parameters cannot be `&mut`",
            ));
        }
        Type::Reference(reference) => {
            let elem = &reference.elem;
            (
                quote! { &#elem },
                quote! { <#elem as ::std::borrow::ToOwned>::Owned },
                true,
            )
        }
        Type::ImplTrait(ty) => {
            return Err(Error::new_spanned(
                ty,
                "task parameters cannot be `impl Trait`",
            ));
        }
        ty => (quote! { #ty }, quote! { #ty }, false),
    };

    Ok(Param {
        name,
        ty,
        owned,
        by_ref,
        span,
    })
}

fn dispatch(task: &Task) -> TokenStream {
    let function = &task.ident;
    let locals: Vec<_> = (0..task.params.len())
        .map(|index| Ident::new(&format!("arg{index}"), Span::mixed_site()))
        .collect();
    let owned = task.params.iter().map(|param| &param.owned);
    let checks = task.params.iter().map(|param| {
        let owned = &param.owned;
        // Spanned so an unsupported type is reported on the parameter.
        quote_spanned! { param.span => ::pdk::__private::task_arg::<#owned>(); }
    });
    let args = task.params.iter().zip(&locals).map(|(param, local)| {
        if param.by_ref {
            quote! { &#local }
        } else {
            quote! { #local }
        }
    });
    let await_ = task.is_async.then(|| quote! { .await });
    // Spanned so a wrong return type is reported on the task function.
    let call = quote_spanned! { function.span() => Self::#function(#(#args),*) #await_ };

    quote! {
        Task::#function => {
            #(#checks)*
            let (#(#locals,)*): (#(#owned,)*) = ::pdk::__private::decode(&task, &args)?;
            #call
        }
    }
}

fn methods(task: &Task) -> TokenStream {
    let Task {
        ident: variant,
        params,
        timing:
            Timing::Dynamic {
                schedule,
                schedule_every,
                cancel,
            },
        ..
    } = task
    else {
        return TokenStream::new();
    };
    let names: Vec<_> = params.iter().map(|param| &param.name).collect();
    let params: Vec<_> = params
        .iter()
        .map(|Param { name, ty, .. }| quote! { #name: #ty })
        .collect();
    let delay = Ident::new("delay", Span::mixed_site());
    let interval = Ident::new("interval", Span::mixed_site());
    let doc_schedule = format!(
        " Runs [`Self::{variant}`] once after `delay`. Replaces any pending schedule of the task. \
         When called during `init`, the delay starts once initialization succeeds."
    );
    let doc_every = format!(
        " Runs [`Self::{variant}`] every `interval`. The interval is measured from \
         the end of each run, so runs never overlap. Replaces any \
         pending schedule of the task. When called during `init`, the first \
         interval starts once initialization succeeds."
    );
    let doc_cancel = format!(" Cancels any pending run of [`Self::{variant}`].");

    quote! {
        #[doc = #doc_schedule]
        pub fn #schedule(
            #delay: ::std::time::Duration,
            #(#params),*
        ) -> ::std::result::Result<(), ::pdk::Error> {
            ::pdk::__private::schedule(
                &Task::#variant,
                &(#(#names,)*),
                #delay,
                ::std::option::Option::None,
            )
        }

        #[doc = #doc_every]
        pub fn #schedule_every(
            #interval: ::std::time::Duration,
            #(#params),*
        ) -> ::std::result::Result<(), ::pdk::Error> {
            ::pdk::__private::schedule(
                &Task::#variant,
                &(#(#names,)*),
                #interval,
                ::std::option::Option::Some(#interval),
            )
        }

        #[doc = #doc_cancel]
        pub fn #cancel() {
            ::pdk::__private::cancel(&Task::#variant)
        }
    }
}
