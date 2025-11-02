use crate::service_collection::ServiceCollection;
use crate::{ConfigHost, ErasedService, Lifetime, ResolveError, DependencyResolver, Service, downcast};
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

        let registration = self
            .services
            .metadata()
            .get(&type_id)
            .ok_or(ResolveError::NotRegistered(type_id))?;

        match registration.lifetime {
            Lifetime::Singleton => self.host.resolve::<S>(),

            Lifetime::Scoped => {
                if let Some(value) = self
                    .scoped
                    .read()
                    .unwrap_or_else(|e| {
                        //TODO: log
                        e.into_inner()
                    })
                    .get(&type_id)
                {
                    return downcast::<S>(Arc::clone(value));
                }

                let value = S::construct(self)?;
                let value: ErasedService = Arc::new(value);

                let mut cache = self.scoped.write().unwrap();

                if let Some(existing) = cache.get(&type_id) {
                    return downcast::<S>(Arc::clone(existing));
                }

                cache.insert(type_id, Arc::clone(&value));

                downcast::<S>(value)
            }

            Lifetime::Transient => {
                let value = S::construct(self)?;
                downcast::<S>(Arc::new(value))
            }
        }
    }
}




