use crate::collection::Registration;
use crate::key::{Key, KeyMap};
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
    pub(crate) singletons: Box<[OnceLock<Erased>]>,
    pub(crate) construction: ConstructionLock,
    created: Mutex<Vec<usize>>,
}

impl Container {
    pub(crate) fn new(mut registrations: Vec<Registration>) -> Self {
        let mut lookup = KeyMap::default();
        let mut singletons = 0;
        for (index, registration) in registrations.iter_mut().enumerate() {
            if registration.lifetime == Lifetime::Singleton {
                registration.slot = singletons;
                singletons += 1;
            }
            lookup.insert(registration.key, index);
        }

        Self {
            registrations: registrations.into_boxed_slice(),
            lookup,
            singletons: (0..singletons).map(|_| OnceLock::new()).collect(),
            construction: ConstructionLock::default(),
            created: Mutex::default(),
        }
    }

    pub(crate) fn registration(&self, key: Key) -> Option<&Registration> {
        self.lookup
            .get(&key)
            .and_then(|&index| self.registrations.get(index))
    }

    pub(crate) fn record_singleton(&self, slot: usize) {
        self.created
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
}
