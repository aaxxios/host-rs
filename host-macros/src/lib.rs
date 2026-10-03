//! Procedural macros for the host crates.

#![warn(missing_docs)]

mod abstraction;
mod expand;
mod parse;

use proc_macro::TokenStream;

/// Derives `Service` for a struct.
///
/// By default every field is injected and must be a dependency such as
/// `Arc<T>`. Options:
///
/// - `#[service(constructor = name)]`: build the struct with an associated
///   function instead. A single name refers to `Self::name`; use a path such as
///   `self::connect` for a free function. Its parameters are injected, and it
///   returns `Self` or `Result<Self, E>`.
/// - `#[service(crate = path)]`: path to the `host` crate when it is renamed or
///   re-exported.
#[proc_macro_derive(Service, attributes(service))]
pub fn derive_service(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    expand::expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Lets implementations of a trait be registered as `dyn Trait`.
///
/// Apply once to the trait; any implementation can then be registered with
/// `add_singleton_as::<dyn Trait, Impl>()` and friends. The trait must be
/// `Send + Sync`, since services are shared across threads, and cannot have
/// lifetime parameters. `#[abstraction(crate = path)]` names the `host` crate
/// when it is renamed or re-exported.
#[proc_macro_attribute]
pub fn abstraction(attr: TokenStream, item: TokenStream) -> TokenStream {
    abstraction::expand(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
