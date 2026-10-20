use proc_macro2::{Group, TokenStream, TokenTree};
use quote::{ToTokens, quote, quote_spanned};
use syn::meta::parser;
use syn::parse::Parser;
use syn::{Error, GenericParam, Item, Path, TraitItem, parse_quote};

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

    // An associated type is fixed by the trait object (`dyn Trait<Out = X>`),
    // so each one becomes a parameter of the generated impl.
    let mut generics = item.generics.clone();
    let mut args: Vec<TokenStream> = item
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Type(param) => Some(param.ident.to_token_stream()),
            GenericParam::Const(param) => Some(param.ident.to_token_stream()),
            GenericParam::Lifetime(_) => None,
        })
        .collect();
    for trait_item in &item.items {
        let TraitItem::Type(assoc) = trait_item else {
            continue;
        };
        if !assoc.generics.params.is_empty() {
            return Err(Error::new_spanned(
                &assoc.generics,
                "`#[abstraction]` does not support generic associated types",
            ));
        }
        let name = &assoc.ident;
        let bounds = &assoc.bounds;
        generics.params.push(parse_quote!(#name: #bounds));
        args.push(quote!(#name = #name));
    }
    let trait_path = if args.is_empty() {
        quote!(#ident)
    } else {
        quote!(#ident<#(#args),*>)
    };
    let object = quote!(dyn #trait_path);
    // Generated code sits outside the trait, where `Self` meant the object.
    let as_self = quote!((#object));

    let (check_generics, _, check_where) = generics.split_for_impl();
    let check_generics = replace_self(check_generics.to_token_stream(), &as_self);
    let check_where = replace_self(check_where.to_token_stream(), &as_self);

    generics.params.push(parse_quote!(__Impl));
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(__Impl: #trait_path + 'static));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let impl_generics = replace_self(impl_generics.to_token_stream(), &as_self);
    let where_clause = replace_self(where_clause.to_token_stream(), &as_self);

    // Spanned at the trait name so a missing bound is reported there.
    let assert_send_sync = quote_spanned! {ident.span()=>
        assert_send_sync::<dyn #trait_path>();
    };

    Ok(quote! {
        #item

        #[automatically_derived]
        impl #impl_generics #private::CastFrom<__Impl> for #object #where_clause {
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
            fn check #check_generics () #check_where {
                #assert_send_sync
            }
        };
    })
}

fn replace_self(tokens: TokenStream, with: &TokenStream) -> TokenStream {
    tokens
        .into_iter()
        .flat_map(|token| match token {
            TokenTree::Ident(ident) if ident == "Self" => with.clone(),
            TokenTree::Group(group) => {
                let mut replaced =
                    Group::new(group.delimiter(), replace_self(group.stream(), with));
                replaced.set_span(group.span());
                TokenTree::Group(replaced).into()
            }
            other => other.into(),
        })
        .collect()
}
