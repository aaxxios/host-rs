use syn::{Attribute, Path};

/// Options from `#[service(...)]`.
#[derive(Default)]
pub(crate) struct Options {
    pub(crate) constructor: Option<Path>,
    pub(crate) krate: Option<Path>,
}

impl Options {
    pub(crate) fn from_attrs(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut options = Self::default();
        for attr in attrs.iter().filter(|attr| attr.path().is_ident("service")) {
            attr.parse_nested_meta(|meta| {
                let slot = if meta.path.is_ident("constructor") {
                    &mut options.constructor
                } else if meta.path.is_ident("crate") {
                    &mut options.krate
                } else {
                    return Err(
                        meta.error("unknown `service` option, expected `constructor` or `crate`")
                    );
                };
                if slot.is_some() {
                    return Err(meta.error("duplicate `service` option"));
                }
                *slot = Some(meta.value()?.parse()?);
                Ok(())
            })?;
        }
        Ok(options)
    }
}
