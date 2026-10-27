use crate::parse::Options;
use proc_macro2::{Ident, TokenStream, TokenTree};
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, Fields, parse_quote};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let options = Options::from_attrs(&input.attrs)?;
    let root = options.krate.unwrap_or_else(|| parse_quote!(::hosting));
    let private = quote!(#root::__private);

    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "`#[derive(Service)]` supports structs only",
        ));
    };
    let (dependencies, construct) = match &options.constructor {
        Some(path) => {
            // A bare name refers to an associated function of the struct.
            let function = match path.get_ident() {
                Some(name) => quote!(Self::#name),
                None => quote!(#path),
            };
            let bound =
                quote_spanned!(path.span()=> #private::constructor::<Self, _, _>(&#function));
            (quote!(#bound.dependencies()), quote!(#bound.call(resolver)))
        }
        None => fields_constructor(&data.fields, &private),
    };

    let ident = &input.ident;
    let mut generics = input.generics.clone();
    let params: Vec<_> = generics
        .type_params()
        .map(|param| param.ident.clone())
        .collect();
    let predicates = &mut generics.make_where_clause().predicates;
    predicates.push(parse_quote!(Self: ::core::marker::Send + ::core::marker::Sync + 'static));
    // Bound generic field types on `Inject`, as serde does for `Serialize`, so
    // users don't have to repeat `Send + Sync + 'static` on type parameters.
    if options.constructor.is_none() {
        for field in &data.fields {
            let ty = &field.ty;
            if mentions_any(quote!(#ty), &params) {
                predicates.push(parse_quote!(#ty: #private::Inject));
            }
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics #private::Service for #ident #ty_generics #where_clause {
            fn dependencies() -> ::std::vec::Vec<#private::Dependency> {
                #dependencies
            }

            fn construct(
                resolver: &#private::Resolver<'_>,
            ) -> ::core::result::Result<Self, #private::ResolveError> {
                #construct
            }
        }
    })
}

fn mentions_any(tokens: TokenStream, idents: &[Ident]) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => idents.contains(&ident),
        TokenTree::Group(group) => mentions_any(group.stream(), idents),
        _ => false,
    })
}

fn fields_constructor(fields: &Fields, private: &TokenStream) -> (TokenStream, TokenStream) {
    let types: Vec<_> = fields.iter().map(|field| &field.ty).collect();
    let inject = quote!(#private::Inject>::inject(resolver)?);
    let value = match fields {
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
    (
        quote!(::std::vec![#(<#types as #private::Inject>::dependency()),*]),
        quote!(::core::result::Result::Ok(#value)),
    )
}
