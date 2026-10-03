use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::meta::parser;
use syn::parse::Parser;
use syn::{Error, Item, Path, parse_quote};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let mut krate: Option<Path> = None;
    parser(|meta| {
        if !meta.path.is_ident("crate") {
            return Err(meta.error("unknown `abstraction` option, expected `crate`"));
        }
        if krate.is_some() {
            return Err(meta.error("duplicate `abstraction` option"));
        }
        krate = Some(meta.value()?.parse()?);
        Ok(())
    })
    .parse2(attr)?;
    let root = krate.unwrap_or_else(|| parse_quote!(::host));
    let private = quote!(#root::__private);

    let item: Item = syn::parse2(item)?;
    let Item::Trait(item) = item else {
        return Err(Error::new_spanned(
            item,
            "`#[abstraction]` applies to traits only",
        ));
    };
    if let Some(lifetime) = item.generics.lifetimes().next() {
        return Err(Error::new_spanned(
            lifetime,
            "`#[abstraction]` traits cannot have lifetime parameters",
        ));
    }

    let ident = &item.ident;
    let (trait_impl_generics, trait_generics, trait_where) = item.generics.split_for_impl();

    let mut generics = item.generics.clone();
    generics.params.push(parse_quote!(__Impl));
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(__Impl: #ident #trait_generics + 'static));
    let (impl_generics, _, where_clause) = generics.split_for_impl();

    let assert_send_sync = quote_spanned! {ident.span()=>
        assert_send_sync::<dyn #ident #trait_generics>();
    };

    Ok(quote! {
        #item

        #[automatically_derived]
        impl #impl_generics #private::CastFrom<__Impl> for dyn #ident #trait_generics #where_clause {
            fn cast(from: ::std::sync::Arc<__Impl>) -> ::std::sync::Arc<Self> {
                from
            }
        }

        const _: () = {
            fn assert_send_sync<
                T: ?::core::marker::Sized + ::core::marker::Send + ::core::marker::Sync,
            >() {
            }

            #[allow(dead_code)]
            fn check #trait_impl_generics () #trait_where {
                #assert_send_sync
            }
        };
    })
}
