use proc_macro::TokenStream;
use quote::quote;
use syn::{
    FnArg,
    ImplItem,
    ItemImpl,
    PathArguments,
    ReturnType,
    Type,
    TypePath,
    parse_macro_input,
};

#[proc_macro_attribute]
pub fn service(
    _attr: TokenStream,
    input: TokenStream,
) -> TokenStream {
    let item = parse_macro_input!(input as ItemImpl);

    match expand_service(item) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_service(item: ItemImpl) -> syn::Result<proc_macro2::TokenStream> {
    let self_ty = item.self_ty.as_ref().clone();

    let constructor = item
        .items
        .iter()
        .find_map(|item| match item {
            ImplItem::Fn(method) if method.sig.ident == "new" => {
                Some(method)
            }
            _ => None,
        })
        .ok_or_else(|| {
            syn::Error::new_spanned(
                &item.self_ty,
                "a #[service] impl must contain fn new(...)",
            )
        })?;

    if constructor.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            &constructor.sig,
            "async constructors are not supported yet",
        ));
    }

    if constructor.sig.receiver().is_some() {
        return Err(syn::Error::new_spanned(
            &constructor.sig,
            "service constructor must be an associated function",
        ));
    }

    match &constructor.sig.output {
        ReturnType::Default => {}

        ReturnType::Type(_, ty) => {
            if !matches!(&**ty, Type::Path(path) if path.path.is_ident("Self")) {
                return Err(syn::Error::new_spanned(
                    ty,
                    "service constructor must return Self",
                ));
            }
        }
    }

    let mut dependencies = Vec::new();

    for argument in &constructor.sig.inputs {
        let FnArg::Typed(argument) = argument else {
            return Err(syn::Error::new_spanned(
                argument,
                "invalid constructor parameter",
            ));
        };

        let dependency = extract_arc_inner_type(&argument.ty)?;

        dependencies.push(dependency);
    }

    let resolved_dependencies = dependencies.iter().map(|dependency| {
        quote! {
            resolver.resolve::<#dependency>()?
        }
    });

    let constructor_name = &constructor.sig.ident;

    let generated = quote! {
        #item

        impl Service for #self_ty {
            fn construct<R: DependencyResolver>(
                resolver: &R,
            ) -> Result<Self, ResolveError> {
                Ok(Self::#constructor_name(
                    #(#resolved_dependencies),*
                ))
            }
        }
    };

    Ok(generated)
}

fn extract_arc_inner_type(ty: &Type) -> syn::Result<Type> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "service constructor dependencies must be Arc<T>",
        ));
    };

    let segment = path
        .segments
        .last()
        .ok_or_else(|| {
            syn::Error::new_spanned(
                ty,
                "invalid dependency type",
            )
        })?;

    if segment.ident != "Arc" {
        return Err(syn::Error::new_spanned(
            ty,
            "service constructor dependencies must be Arc<T>",
        ));
    }

    let PathArguments::AngleBracketed(arguments) =
        &segment.arguments
    else {
        return Err(syn::Error::new_spanned(
            ty,
            "Arc<T> requires a type parameter",
        ));
    };

    let Some(syn::GenericArgument::Type(inner)) =
        arguments.args.first()
    else {
        return Err(syn::Error::new_spanned(
            ty,
            "Arc<T> requires a type parameter",
        ));
    };

    Ok(inner.clone())
}