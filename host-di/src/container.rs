use crate::collection::{Registration, Source};
use crate::key::{Key, KeyMap};
use crate::scope::Scope;
use crate::sync::ConstructionLock;
use crate::{Lifetime, ResolveError, Resolver};
use std::any::Any;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

pub(crate) type Erased = Arc<dyn Any + Send + Sync>;

pub(crate) type ConstructFn =
    Arc<dyn Fn(&Resolver<'_>) -> Result<Erased, ResolveError> + Send + Sync>;

/// Registrations frozen at build, plus singleton storage.
pub(crate) struct Container {
    registrations: Box<[Registration]>,
    /// Every registration of a service, in registration order.
    lookup: KeyMap<Box<[usize]>>,
    pub(crate) scoped_count: usize,
    pub(crate) singletons: Slots,
    pub(crate) construction: ConstructionLock,
}

impl Container {
    pub(crate) fn new(mut registrations: Vec<Registration>) -> Self {
        // Every registration is reachable through `resolve_all`, so each gets
        // its own storage, including ones a later registration overrides.
        let (mut singletons, mut scoped) = (0, 0);
        for registration in &mut registrations {
            let count = match registration.lifetime {
                Lifetime::Singleton => &mut singletons,
                Lifetime::Scoped => &mut scoped,
                Lifetime::Transient => continue,
            };
            registration.slot = *count;
            *count += 1;
        }

        let container = Self {
            lookup: index_by_key(&registrations),
            registrations: registrations.into_boxed_slice(),
            scoped_count: scoped,
            singletons: Slots::new(singletons),
            construction: ConstructionLock::default(),
        };
        for registration in &container.registrations {
            if let Source::Instance(instance) = &registration.source {
                container
                    .singletons
                    .fill(registration.slot, Arc::clone(instance));
            }
        }
        container
    }

    /// The registration a single resolution uses: the last one.
    pub(crate) fn registration(&self, key: Key) -> Option<&Registration> {
        self.registrations_of(key).last()
    }

    pub(crate) fn registrations_of(
        &self,
        key: Key,
    ) -> impl DoubleEndedIterator<Item = &Registration> + ExactSizeIterator {
        self.lookup
            .get(&key)
            .map_or(&[][..], |indices| &indices[..])
            .iter()
            .map(|&index| &self.registrations[index])
    }
}

/// Registration indices grouped by service, each group in registration order.
pub(crate) fn index_by_key(registrations: &[Registration]) -> KeyMap<Box<[usize]>> {
    let mut grouped: KeyMap<Vec<usize>> = KeyMap::default();
    for (index, registration) in registrations.iter().enumerate() {
        grouped.entry(registration.key).or_default().push(index);
    }
    grouped
        .into_iter()
        .map(|(key, indices)| (key, indices.into_boxed_slice()))
        .collect()
}

/// Cached instances owned by the container or by a scope.
///
/// Filled slots form a list from the newest instance back to the oldest,
/// threaded through the slots themselves, so the store stays one allocation
/// and is released in reverse creation order without sorting.
pub(crate) struct Slots {
    slots: Box<[Slot]>,
    newest: AtomicUsize,
}

struct Slot {
    value: OnceLock<Erased>,
    previous: AtomicUsize,
}

const NONE: usize = usize::MAX;

impl Slots {
    pub(crate) fn new(len: usize) -> Self {
        Self {
            slots: (0..len)
                .map(|_| Slot {
                    value: OnceLock::new(),
                    previous: AtomicUsize::new(NONE),
                })
                .collect(),
            newest: AtomicUsize::new(NONE),
        }
    }

    pub(crate) fn get(&self, index: usize) -> Option<Erased> {
        self.slots
            .get(index)
            .and_then(|slot| slot.value.get())
            .map(Arc::clone)
    }

    /// Stores a new instance. Callers hold the owner's construction lock, so
    /// the list is only ever extended by one thread at a time.
    pub(crate) fn fill(&self, index: usize, instance: Erased) {
        if let Some(slot) = self.slots.get(index) {
            if slot.value.set(instance).is_ok() {
                let previous = self.newest.swap(index, Ordering::Relaxed);
                slot.previous.store(previous, Ordering::Relaxed);
            }
        }
    }

    fn release(&mut self) {
        let mut next = self.newest.swap(NONE, Ordering::Relaxed);
        while let Some(slot) = self.slots.get_mut(next) {
            next = *slot.previous.get_mut();
            drop(slot.value.take());
        }
    }
}

impl Drop for Slots {
    fn drop(&mut self) {
        self.release();
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        self.singletons.release();
        // Registered instances were created at build, before anything else, so
        // they are released last, latest registration first.
        let registrations = std::mem::take(&mut self.registrations).into_vec();
        registrations.into_iter().rev().for_each(drop);
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

    /// Resolves every registration of `T`, in registration order.
    pub fn resolve_all<T: ?Sized + Send + Sync + 'static>(
        &self,
    ) -> Result<Vec<Arc<T>>, ResolveError> {
        Resolver::root(&self.container).resolve_all()
    }

    /// Creates a scope for resolving scoped services.
    pub fn create_scope(&self) -> Scope {
        Scope::new(Arc::clone(&self.container))
    }
}

impl fmt::Debug for ServiceProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceProvider")
            .field("registrations", &self.container.registrations)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dependency, Service, ServiceCollection};

    struct Leaf;

    impl Service for Leaf {
        fn dependencies() -> Vec<Dependency> {
            Vec::new()
        }

        fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
            Ok(Leaf)
        }
    }

    struct Config;

    #[test]
    fn every_registration_gets_storage() {
        let mut services = ServiceCollection::new();
        services
            .add_scoped::<Leaf>()
            .add_scoped::<Leaf>()
            .add_singleton_instance(Config)
            .add_singleton_with(|_| Ok(Config));
        let provider = services.build().unwrap();
        let container = &provider.container;

        assert_eq!(container.scoped_count, 2);
        assert_eq!(container.singletons.slots.len(), 2);
        assert_eq!(container.singletons.newest.load(Ordering::Relaxed), 0);
    }
}
