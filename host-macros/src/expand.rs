use crate::parse::Options;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, parse_quote};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let options = Options::from_attrs(&input.attrs)?;
    let root = options.krate.unwrap_or_else(|| parse_quote!(::host));
    let private = quote!(#root::__private);

    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "`#[derive(Service)]` supports structs only",
        ));
    };
    if let Some(constructor) = &options.constructor {
        return Err(Error::new_spanned(
            constructor,
            "named constructors are not supported yet",
        ));
    }

    let types: Vec<_> = data.fields.iter().map(|field| &field.ty).collect();
    let inject = quote!(#private::Inject>::inject(resolver)?);
    let value = match &data.fields {
        Fields::Named(fields) => {
            let names = fields.named.iter().map(|field| &field.ident);
            quote!(Self { #(#names: <#types as #inject),* })
        }
        Fields::Unnamed(_) => quote!(Self(#(<#types as #inject),*)),
        Fields::Unit => quote!({
            let _ = resolver;
            Self
        }),
    };

    let ident = &input.ident;
    let mut generics = input.generics.clone();
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(Self: ::core::marker::Send + ::core::marker::Sync + 'static));
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics #private::Service for #ident #ty_generics #where_clause {
            fn dependencies() -> ::std::vec::Vec<#private::Dependency> {
                ::std::vec![#(<#types as #private::Inject>::dependency()),*]
            }

            fn construct(
                resolver: &#private::Resolver<'_>,
            ) -> ::core::result::Result<Self, #private::ResolveError> {
                ::core::result::Result::Ok(#value)
            }
        }
    })
}
