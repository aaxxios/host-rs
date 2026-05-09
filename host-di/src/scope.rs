use crate::container::{Container, CreationLog, Erased, release};
use crate::sync::ConstructionLock;
use crate::{ResolveError, Resolver};
use std::sync::{Arc, OnceLock};

/// A unit of work with its own scoped instances, such as one request.
///
/// Owns a handle to the container, so it can be moved into other threads or
/// tasks and keeps working after the provider that created it is dropped.
pub struct Scope {
    container: Arc<Container>,
    state: ScopeState,
}

#[derive(Default)]
pub(crate) struct ScopeState {
    slots: OnceLock<Box<[OnceLock<Erased>]>>,
    pub(crate) construction: ConstructionLock,
    pub(crate) created: CreationLog,
}

impl Scope {
    pub(crate) fn new(container: Arc<Container>) -> Self {
        Self {
            container,
            state: ScopeState::default(),
        }
    }

    /// Resolves a service of any lifetime.
    pub fn resolve<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Arc<T>, ResolveError> {
        Resolver::scoped(&self.container, &self.state).resolve()
    }
}

impl ScopeState {
    /// Storage is allocated on the first scoped resolution, so scopes that only
    /// touch singletons and transients never allocate.
    pub(crate) fn slot(&self, container: &Container, index: usize) -> Option<&OnceLock<Erased>> {
        self.slots
            .get_or_init(|| {
                (0..container.scoped_count)
                    .map(|_| OnceLock::new())
                    .collect()
            })
            .get(index)
    }
}

impl Drop for ScopeState {
    fn drop(&mut self) {
        if let Some(slots) = self.slots.get_mut() {
            release(slots, &mut self.created);
        }
    }
}
