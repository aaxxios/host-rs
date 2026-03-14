use crate::container::{ConstructFn, Container, Erased};
use crate::key::Key;
use crate::{BuildError, Lifetime, Service, ServiceProvider};
use std::any::Any;
use std::sync::Arc;

/// Services registered for an application, frozen into a [`ServiceProvider`]
/// by [`build`](Self::build).
///
/// Registering a type again replaces the earlier registration. Singletons are
/// constructed outside any scope, so a singleton that depends on a scoped
/// service fails to resolve rather than holding on to one scope's instance.
/// Transients are constructed in the scope that requested them.
#[derive(Default)]
pub struct ServiceCollection {
    registrations: Vec<Registration>,
}

pub(crate) struct Registration {
    pub(crate) key: Key,
    pub(crate) lifetime: Lifetime,
    pub(crate) construct: ConstructFn,
    /// A `fn(Erased) -> Option<Arc<I>>` for the registered service type `I`.
    pub(crate) cast: Box<dyn Any + Send + Sync>,
    /// Index into the storage of this registration's lifetime, set by the container.
    pub(crate) slot: usize,
}

impl ServiceCollection {
    /// Creates an empty collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `T` with one instance shared for the life of the provider.
    pub fn add_singleton<T: Service>(&mut self) -> &mut Self {
        self.add::<T>(Lifetime::Singleton)
    }

    /// Registers `T` with one instance per scope.
    pub fn add_scoped<T: Service>(&mut self) -> &mut Self {
        self.add::<T>(Lifetime::Scoped)
    }

    /// Registers `T` with a new instance on every resolution.
    pub fn add_transient<T: Service>(&mut self) -> &mut Self {
        self.add::<T>(Lifetime::Transient)
    }

    /// Freezes the registrations into a provider.
    pub fn build(self) -> Result<ServiceProvider, BuildError> {
        Ok(ServiceProvider::new(Container::new(self.registrations)))
    }

    fn add<T: Service>(&mut self, lifetime: Lifetime) -> &mut Self {
        self.registrations.push(Registration {
            key: Key::of::<T>(),
            lifetime,
            construct: Arc::new(|resolver| {
                T::construct(resolver).map(|service| Arc::new(service) as Erased)
            }),
            cast: concrete_cast::<T>(),
            slot: 0,
        });
        self
    }
}

fn concrete_cast<T: Send + Sync + 'static>() -> Box<dyn Any + Send + Sync> {
    let cast: fn(Erased) -> Option<Arc<T>> = |instance| instance.downcast().ok();
    Box::new(cast)
}
