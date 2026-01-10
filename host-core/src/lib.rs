use host_di::scope::Scope;
use host_di::service_collection::ServiceCollection;
use host_di::{
    ConfigHost, DependencyResolver, ErasedService, Lifetime, ResolveError, Service, downcast,
};
use std::any::TypeId;
use std::sync::Arc;
pub struct Host {
    services: Arc<ServiceCollection>,
}

#[cfg(feature = "logging")]
pub(crate) fn log(message: &str) {
    tracing::info!("{message}");
}

#[cfg(not(feature = "logging"))]
pub(crate) fn log(_message: &str) {}

impl Host {
    /// create a new service scope
    pub fn scope(&self) -> Scope<'_, Host> {
        Scope::new(self)
    }

    pub fn resolve<T>(&self) -> Result<Arc<T>, ResolveError>
    where
        T: Service,
    {
        self.resolve_inner()
    }

    fn resolve_inner<T: Service>(&self) -> Result<Arc<T>, ResolveError> {
        let type_id = TypeId::of::<T>();

        let registration = self.services.get_meta(type_id)?;

        match registration.lifetime {
            Lifetime::Singleton => {
                let cache = self.services.singleton().read().map_err(|_| {
                    log("cached poisoned");
                    ResolveError::CachedPoisoned
                })?;

                if let Some(value) = cache.get(&type_id) {
                    return downcast::<T>(Arc::clone(value));
                }

                let value = T::construct(self)?;
                let value: ErasedService = Arc::new(value);

                let mut cache = self.services.singleton().write().map_err(|_| {
                    log("cached poisoned");
                    ResolveError::CachedPoisoned
                })?;

                if let Some(existing) = cache.get(&type_id) {
                    return downcast::<T>(Arc::clone(existing));
                }

                cache.insert(type_id, Arc::clone(&value));

                downcast::<T>(value)
            }

            Lifetime::Transient => {
                let value = T::construct(self)?;
                downcast::<T>(Arc::new(value))
            }

            Lifetime::Scoped => {
                log("attempting to resolve scoped service outside a scope");
                Err(ResolveError::Construction(
                    "a scoped service must be resolved from a Scope".into(),
                ))
            }
        }
    }
}

impl ConfigHost for Host {
    fn service_collection(&self) -> Arc<ServiceCollection> {
        Arc::clone(&self.services)
    }

    fn resolve<T: Service>(&self) -> Result<Arc<T>, ResolveError> {
        self.resolve()
    }
}
impl DependencyResolver for Host {
    fn resolve<T>(&self) -> Result<Arc<T>, ResolveError>
    where
        T: Service,
    {
        self.resolve::<T>()
    }
}
