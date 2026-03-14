use crate::{Dependency, ResolveError, Resolver};

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
