use crate::{Dependency, ResolveError, Resolver};
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
