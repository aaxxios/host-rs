use crate::service_collection::ServiceCollection;
use crate::{
    ConfigHost, DependencyResolver, ErasedService, Lifetime, ResolveError, Service, downcast, log,
};
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Represents a scope of services. Scoped services are unique within the scope they were created.
pub struct Scope<'a, T: ConfigHost> {
    host: &'a T,
    services: Arc<ServiceCollection>,
    scoped: RwLock<HashMap<TypeId, ErasedService>>,
}

impl<'a, T: ConfigHost> DependencyResolver for Scope<'a, T> {
    fn resolve<S>(&self) -> Result<Arc<S>, ResolveError>
    where
        S: Service,
    {
        self.resolve::<S>()
    }
}

impl<'a, T: ConfigHost> Scope<'a, T> {
    pub fn new(host: &'a T) -> Self {
        Scope {
            services: host.service_collection(),
            scoped: RwLock::new(HashMap::new()),
            host,
        }
    }
    pub fn resolve<S>(&self) -> Result<Arc<S>, ResolveError>
    where
        S: Service,
    {
        self.resolve_inner()
    }

    fn resolve_inner<S>(&self) -> Result<Arc<S>, ResolveError>
    where
        S: Service,
    {
        let type_id = TypeId::of::<S>();

        let registration = self.services.get_meta(type_id)?;

        match registration.lifetime {
            Lifetime::Singleton => self.host.resolve(),
            Lifetime::Transient => self.host.resolve(),
            Lifetime::Scoped => {
                {
                    let cache = self.scoped.read().map_err(|_| {
                        log("cached poisoned");
                        ResolveError::CachedPoisoned
                    })?;
                    if let Some(existing) = cache.get(&type_id) {
                        return downcast::<S>(Arc::clone(existing));
                    }
                }
                let value = S::construct(self)?;
                let value: ErasedService = Arc::new(value);

                let mut cache = self.scoped.write().map_err(|_| {
                    log("cached poisoned");
                    ResolveError::CachedPoisoned
                })?;

                if let Some(existing) = cache.get(&type_id) {
                    return downcast::<S>(Arc::clone(existing));
                }

                cache.insert(type_id, Arc::clone(&value));

                downcast::<S>(value)
            }
        }
    }
}
