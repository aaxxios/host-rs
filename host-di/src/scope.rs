use crate::container::{Container, Slots};
use crate::sync::ConstructionLock;
use crate::{ResolveError, Resolver};
use std::fmt;
use std::sync::{Arc, OnceLock};

/// A unit of work with its own scoped instances, such as one request.
///
/// Owns a handle to the container, so it can be moved into other threads or
/// tasks and keeps working after the provider that created it is dropped.
pub struct Scope {
    // Declared first: scoped instances must be released before the container,
    // which may be dropped with this scope.
    state: ScopeState,
    container: Arc<Container>,
}

#[derive(Default)]
pub(crate) struct ScopeState {
    slots: OnceLock<Slots>,
    pub(crate) construction: ConstructionLock,
}

impl Scope {
    pub(crate) fn new(container: Arc<Container>) -> Self {
        Self {
            state: ScopeState::default(),
            container,
        }
    }

    /// Resolves a service of any lifetime.
    pub fn resolve<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Arc<T>, ResolveError> {
        Resolver::scoped(&self.container, &self.state).resolve()
    }

    /// Resolves every registration of `T`, in registration order.
    pub fn resolve_all<T: ?Sized + Send + Sync + 'static>(
        &self,
    ) -> Result<Vec<Arc<T>>, ResolveError> {
        Resolver::scoped(&self.container, &self.state).resolve_all()
    }
}

impl fmt::Debug for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scope").finish_non_exhaustive()
    }
}

impl ScopeState {
    /// Storage is allocated on the first scoped resolution, so scopes that only
    /// touch singletons and transients never allocate.
    pub(crate) fn slots(&self, container: &Container) -> &Slots {
        self.slots
            .get_or_init(|| Slots::new(container.scoped_count))
    }
}
