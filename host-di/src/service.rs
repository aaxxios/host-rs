use crate::{BoxError, Dependency, ResolveError, Resolver};
use std::sync::Arc;

/// A type the container can construct.
///
/// Usually derived with `#[derive(Service)]`. Constructors must not block on
/// other threads that resolve services from the same container: construction
/// of cached instances is serialised per container and per scope.
pub trait Service: Sized + Send + Sync + 'static {
    /// The services `construct` resolves, used to validate the graph at build.
    fn dependencies() -> Vec<Dependency>;

    /// Builds the service, resolving its dependencies through `resolver`.
    fn construct(resolver: &Resolver<'_>) -> Result<Self, ResolveError>;
}

/// A value the container can supply as a field or constructor argument.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be injected",
    label = "not a dependency",
    note = "fields must be `Arc<T>`; use `#[service(constructor = ...)]` for other fields"
)]
pub trait Inject: Sized {
    fn dependency() -> Dependency;

    fn inject(resolver: &Resolver<'_>) -> Result<Self, ResolveError>;
}

impl<T: ?Sized + Send + Sync + 'static> Inject for Arc<T> {
    fn dependency() -> Dependency {
        Dependency::on::<T>()
    }

    fn inject(resolver: &Resolver<'_>) -> Result<Self, ResolveError> {
        resolver.resolve()
    }
}

/// What a named constructor may return: `S` or `Result<S, E>`.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "service constructor must return `{S}` or `Result<{S}, E>`",
    note = "constructors are synchronous; move async setup into a hosted service"
)]
pub trait ConstructorOutput<S> {
    fn into_result(self) -> Result<S, BoxError>;
}

impl<S> ConstructorOutput<S> for S {
    fn into_result(self) -> Result<S, BoxError> {
        Ok(self)
    }
}

impl<S, E: Into<BoxError>> ConstructorOutput<S> for Result<S, E> {
    fn into_result(self) -> Result<S, BoxError> {
        self.map_err(Into::into)
    }
}

/// A function whose arguments can all be injected, building `S`.
#[doc(hidden)]
pub trait Constructor<S, Args> {
    fn dependencies() -> Vec<Dependency>;

    fn call(&self, resolver: &Resolver<'_>) -> Result<S, ResolveError>;
}

macro_rules! constructor {
    ($($arg:ident),*) => {
        impl<F, R, S, $($arg: Inject),*> Constructor<S, ($($arg,)*)> for F
        where
            F: Fn($($arg),*) -> R,
            R: ConstructorOutput<S>,
        {
            fn dependencies() -> Vec<Dependency> {
                vec![$($arg::dependency()),*]
            }

            #[allow(non_snake_case)]
            fn call(&self, resolver: &Resolver<'_>) -> Result<S, ResolveError> {
                $(let $arg = $arg::inject(resolver)?;)*
                self($($arg),*)
                    .into_result()
                    .map_err(|error| resolver.failure(error))
            }
        }
    };
}

constructor!();
constructor!(A1);
constructor!(A1, A2);
constructor!(A1, A2, A3);
constructor!(A1, A2, A3, A4);
constructor!(A1, A2, A3, A4, A5);
constructor!(A1, A2, A3, A4, A5, A6);
constructor!(A1, A2, A3, A4, A5, A6, A7);
constructor!(A1, A2, A3, A4, A5, A6, A7, A8);
constructor!(A1, A2, A3, A4, A5, A6, A7, A8, A9);
constructor!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10);
constructor!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11);
constructor!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12);
constructor!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13);
constructor!(A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14);
constructor!(
    A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15
);
constructor!(
    A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16
);

#[doc(hidden)]
pub fn constructor_dependencies<S, Args, F: Constructor<S, Args>>(_: &F) -> Vec<Dependency> {
    F::dependencies()
}

#[doc(hidden)]
pub fn call_constructor<S, Args, F: Constructor<S, Args>>(
    constructor: &F,
    resolver: &Resolver<'_>,
) -> Result<S, ResolveError> {
    constructor.call(resolver)
}
