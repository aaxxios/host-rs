use crate::collection::Registration;
use crate::key::{Key, KeyMap};
use crate::scope::Scope;
use crate::sync::ConstructionLock;
use crate::{Lifetime, ResolveError, Resolver};
use std::any::Any;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

pub(crate) type Erased = Arc<dyn Any + Send + Sync>;

pub(crate) type ConstructFn =
    Arc<dyn Fn(&Resolver<'_>) -> Result<Erased, ResolveError> + Send + Sync>;

/// Registrations frozen at build, plus singleton storage.
pub(crate) struct Container {
    registrations: Box<[Registration]>,
    lookup: KeyMap<usize>,
    pub(crate) scoped_count: usize,
    pub(crate) singletons: Box<[OnceLock<Erased>]>,
    pub(crate) construction: ConstructionLock,
    pub(crate) created: CreationLog,
}

impl Container {
    pub(crate) fn new(mut registrations: Vec<Registration>) -> Self {
        let mut lookup = KeyMap::default();
        let (mut singletons, mut scoped) = (0, 0);
        for (index, registration) in registrations.iter_mut().enumerate() {
            match registration.lifetime {
                Lifetime::Singleton => {
                    registration.slot = singletons;
                    singletons += 1;
                }
                Lifetime::Scoped => {
                    registration.slot = scoped;
                    scoped += 1;
                }
                Lifetime::Transient => {}
            }
            lookup.insert(registration.key, index);
        }

        Self {
            registrations: registrations.into_boxed_slice(),
            lookup,
            scoped_count: scoped,
            singletons: (0..singletons).map(|_| OnceLock::new()).collect(),
            construction: ConstructionLock::default(),
            created: CreationLog::default(),
        }
    }

    pub(crate) fn registration(&self, key: Key) -> Option<&Registration> {
        self.lookup
            .get(&key)
            .and_then(|&index| self.registrations.get(index))
    }
}

/// Slot indices in the order their instances were created.
#[derive(Default)]
pub(crate) struct CreationLog(Mutex<Vec<usize>>);

impl CreationLog {
    pub(crate) fn record(&self, slot: usize) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(slot);
    }
}

/// The root of a built container. Cheap to clone.
#[derive(Clone)]
pub struct ServiceProvider {
    container: Arc<Container>,
}

impl ServiceProvider {
    pub(crate) fn new(container: Container) -> Self {
        Self {
            container: Arc::new(container),
        }
    }

    /// Resolves a singleton or transient service.
    pub fn resolve<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Arc<T>, ResolveError> {
        Resolver::root(&self.container).resolve()
    }

    /// Creates a scope for resolving scoped services.
    pub fn create_scope(&self) -> Scope {
        Scope::new(Arc::clone(&self.container))
    }
}
