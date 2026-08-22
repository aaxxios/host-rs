use crate::{BoxError, Dependency, ResolveError, Resolver};
use std::fmt;
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
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot construct `{S}`",
    label = "not a service constructor",
    note = "a service constructor takes injectable parameters such as `Arc<T>` \
            and returns `{S}` or `Result<{S}, E>`",
    note = "constructors are synchronous; move async setup into a hosted service"
)]
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

/// Checks `f` once so derive output reports a bad constructor in one error.
#[doc(hidden)]
pub fn constructor<S, Args, F: Constructor<S, Args>>(f: &F) -> Bound<'_, F, S> {
    Bound {
        f,
        dependencies: F::dependencies,
        call: F::call,
    }
}

/// A constructor whose bound was checked by [`constructor`]; its methods carry
/// no bounds so a bad constructor produces no follow-on errors.
#[doc(hidden)]
pub struct Bound<'a, F, S> {
    f: &'a F,
    dependencies: fn() -> Vec<Dependency>,
    call: fn(&F, &Resolver<'_>) -> Result<S, ResolveError>,
}

impl<F, S> fmt::Debug for Bound<'_, F, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bound").finish_non_exhaustive()
    }
}

impl<F, S> Bound<'_, F, S> {
    pub fn dependencies(&self) -> Vec<Dependency> {
        (self.dependencies)()
    }

    pub fn call(&self, resolver: &Resolver<'_>) -> Result<S, ResolveError> {
        (self.call)(self.f, resolver)
    }
}
