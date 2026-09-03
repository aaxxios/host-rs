//! Procedural macros for the host crates.

#![warn(missing_docs)]

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
