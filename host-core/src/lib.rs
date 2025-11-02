use host_di::scope::Scope;
use host_di::service_collection::ServiceCollection;
use host_di::{ConfigHost, ErasedService, Lifetime, ResolveError, DependencyResolver, Service, downcast};
use std::any::TypeId;
use std::sync::Arc;
pub struct Host {
    services: Arc<ServiceCollection>,
}

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

        let registration = self
            .services
            .metadata()
            .get(&type_id)
            .ok_or(ResolveError::NotRegistered(type_id))?;

        match registration.lifetime {
            Lifetime::Singleton => {
                if let Some(value) = self.services.singleton().read().unwrap().get(&type_id) {
                    return downcast::<T>(Arc::clone(value));
                }

                let value = T::construct(self)?;
                let value: ErasedService = Arc::new(value);

                let mut cache = self.services.singleton().write().unwrap();

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

            Lifetime::Scoped => Err(ResolveError::Construction(
                "a scoped service must be resolved from a Scope".into(),
            )),
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
