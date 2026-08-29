use crate::collection::{Registration, Source};
use crate::key::{Key, KeyMap};
use crate::scope::Scope;
use crate::sync::ConstructionLock;
use crate::{Lifetime, ResolveError, Resolver};
use std::any::Any;
use std::cmp::Reverse;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

pub(crate) type Erased = Arc<dyn Any + Send + Sync>;

pub(crate) type ConstructFn =
    Arc<dyn Fn(&Resolver<'_>) -> Result<Erased, ResolveError> + Send + Sync>;

/// Registrations frozen at build, plus singleton storage.
pub(crate) struct Container {
    registrations: Box<[Registration]>,
    lookup: KeyMap<usize>,
    pub(crate) scoped_count: usize,
    pub(crate) singletons: Slots,
    pub(crate) construction: ConstructionLock,
}

impl Container {
    pub(crate) fn new(mut registrations: Vec<Registration>) -> Self {
        let mut lookup = KeyMap::default();
        for (index, registration) in registrations.iter().enumerate() {
            lookup.insert(registration.key, index);
        }

        // Only the last registration of each service is reachable, so only
        // those get storage.
        let (mut singletons, mut scoped) = (0, 0);
        let mut effective: Vec<_> = lookup.values().copied().collect();
        effective.sort_unstable();
        for &index in &effective {
            let registration = &mut registrations[index];
            let count = match registration.lifetime {
                Lifetime::Singleton => &mut singletons,
                Lifetime::Scoped => &mut scoped,
                Lifetime::Transient => continue,
            };
            registration.slot = *count;
            *count += 1;
        }

        let container = Self {
            registrations: registrations.into_boxed_slice(),
            lookup,
            scoped_count: scoped,
            singletons: Slots::new(singletons),
            construction: ConstructionLock::default(),
        };
        for &index in &effective {
            let registration = &container.registrations[index];
            if let Source::Instance(instance) = &registration.source {
                container
                    .singletons
                    .fill(registration.slot, Arc::clone(instance));
            }
        }
        container
    }

    pub(crate) fn registration(&self, key: Key) -> Option<&Registration> {
        self.lookup
            .get(&key)
            .and_then(|&index| self.registrations.get(index))
    }
}

/// Cached instances owned by the container or by a scope.
///
/// Creation order is stamped into each slot so the whole store stays one
/// allocation; instances are released in reverse of that order.
pub(crate) struct Slots {
    slots: Box<[Slot]>,
    created: AtomicUsize,
}

#[derive(Default)]
struct Slot {
    value: OnceLock<Erased>,
    order: AtomicUsize,
}

impl Slots {
    pub(crate) fn new(len: usize) -> Self {
        Self {
            slots: (0..len).map(|_| Slot::default()).collect(),
            created: AtomicUsize::new(0),
        }
    }

    pub(crate) fn get(&self, index: usize) -> Option<Erased> {
        self.slots
            .get(index)
            .and_then(|slot| slot.value.get())
            .map(Arc::clone)
    }

    /// Stores a new instance. Callers hold the owner's construction lock, so
    /// stamps follow creation order.
    pub(crate) fn fill(&self, index: usize, instance: Erased) {
        if let Some(slot) = self.slots.get(index) {
            if slot.value.set(instance).is_ok() {
                let order = self.created.fetch_add(1, Ordering::Relaxed);
                slot.order.store(order, Ordering::Relaxed);
            }
        }
    }

    fn release(&mut self) {
        let count = self.created.load(Ordering::Relaxed);
        let mut filled = Vec::with_capacity(count);
        filled.extend(
            self.slots
                .iter_mut()
                .filter(|slot| slot.value.get().is_some()),
        );
        filled.sort_unstable_by_key(|slot| Reverse(slot.order.load(Ordering::Relaxed)));
        for slot in filled {
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
    fn overridden_registrations_get_no_storage() {
        let mut services = ServiceCollection::new();
        services
            .add_scoped::<Leaf>()
            .add_scoped::<Leaf>()
            .add_singleton_instance(Config)
            .add_singleton_with(|_| Ok(Config));
        let provider = services.build().unwrap();
        let container = &provider.container;

        assert_eq!(container.scoped_count, 1);
        assert_eq!(container.singletons.slots.len(), 1);
        assert_eq!(container.singletons.created.load(Ordering::Relaxed), 0);
    }
}
