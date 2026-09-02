use crate::container::{ConstructFn, Container, Erased};
use crate::graph::find_cycles;
use crate::key::Key;
use crate::{
    BoxError, BuildError, BuildIssue, Dependency, Lifetime, Resolver, Service, ServiceProvider,
};
use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// Services registered for an application, frozen into a [`ServiceProvider`]
/// by [`build`](Self::build).
///
/// Registering a type again replaces the earlier registration. Singletons are
/// constructed outside any scope, so a singleton that depends on a scoped
/// service fails to resolve rather than holding on to one scope's instance.
/// Transients are constructed in the scope that requested them.
#[derive(Debug, Default)]
pub struct ServiceCollection {
    registrations: Vec<Registration>,
}

pub(crate) struct Registration {
    pub(crate) key: Key,
    pub(crate) lifetime: Lifetime,
    pub(crate) source: Source,
    /// Declared dependencies; `None` for factories and instances.
    pub(crate) dependencies: Option<fn() -> Vec<Dependency>>,
    /// A `fn(Erased) -> Option<Arc<I>>` for the registered service type `I`.
    pub(crate) cast: Box<dyn Any + Send + Sync>,
    /// Index into the storage of this registration's lifetime, set by the container.
    pub(crate) slot: usize,
}

impl fmt::Debug for Registration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registration")
            .field("service", &self.key.name)
            .field("lifetime", &self.lifetime)
            .finish_non_exhaustive()
    }
}

pub(crate) enum Source {
    Construct(ConstructFn),
    Instance(Erased),
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

    /// Registers a factory for `T` with one instance shared for the life of the
    /// provider.
    ///
    /// Errors from resolving dependencies inside the factory keep their original
    /// path; any other error is reported as a construction failure of `T`.
    /// Like constructors, factories must not block on other threads that
    /// resolve from the same container.
    pub fn add_singleton_with<T, F>(&mut self, factory: F) -> &mut Self
    where
        T: Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<T, BoxError> + Send + Sync + 'static,
    {
        self.add_factory(Lifetime::Singleton, factory)
    }

    /// Registers a factory for `T` with one instance per scope.
    pub fn add_scoped_with<T, F>(&mut self, factory: F) -> &mut Self
    where
        T: Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<T, BoxError> + Send + Sync + 'static,
    {
        self.add_factory(Lifetime::Scoped, factory)
    }

    /// Registers a factory for `T` that runs on every resolution.
    pub fn add_transient_with<T, F>(&mut self, factory: F) -> &mut Self
    where
        T: Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<T, BoxError> + Send + Sync + 'static,
    {
        self.add_factory(Lifetime::Transient, factory)
    }

    /// Registers an already constructed singleton.
    pub fn add_singleton_instance<T: Send + Sync + 'static>(&mut self, instance: T) -> &mut Self {
        self.push::<T>(
            Lifetime::Singleton,
            Source::Instance(Arc::new(instance)),
            None,
        )
    }

    /// Freezes the registrations into a provider.
    ///
    /// Fails if registered services depend on each other in a cycle.
    pub fn build(self) -> Result<ServiceProvider, BuildError> {
        let cycles = find_cycles(&self.registrations);
        if !cycles.is_empty() {
            return Err(BuildError::new(
                cycles
                    .into_iter()
                    .map(|path| BuildIssue::Cycle { path })
                    .collect(),
            ));
        }
        Ok(ServiceProvider::new(Container::new(self.registrations)))
    }

    fn add<T: Service>(&mut self, lifetime: Lifetime) -> &mut Self {
        let construct: ConstructFn =
            Arc::new(|resolver| T::construct(resolver).map(|service| Arc::new(service) as Erased));
        self.push::<T>(
            lifetime,
            Source::Construct(construct),
            Some(T::dependencies),
        )
    }

    fn add_factory<T, F>(&mut self, lifetime: Lifetime, factory: F) -> &mut Self
    where
        T: Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<T, BoxError> + Send + Sync + 'static,
    {
        let construct: ConstructFn = Arc::new(move |resolver| match factory(resolver) {
            Ok(service) => Ok(Arc::new(service) as Erased),
            Err(error) => Err(resolver.failure(error)),
        });
        self.push::<T>(lifetime, Source::Construct(construct), None)
    }

    fn push<T: Send + Sync + 'static>(
        &mut self,
        lifetime: Lifetime,
        source: Source,
        dependencies: Option<fn() -> Vec<Dependency>>,
    ) -> &mut Self {
        self.registrations.push(Registration {
            key: Key::of::<T>(),
            lifetime,
            source,
            dependencies,
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
