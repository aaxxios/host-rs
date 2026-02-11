mod builder;

pub use builder::HostBuilder;
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
                {
                    let cache = self.services.singleton().read().map_err(|_| {
                        log("cached poisoned");
                        ResolveError::CachedPoisoned
                    })?;
                    if let Some(value) = cache.get(&type_id) {
                        return downcast::<T>(Arc::clone(value));
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use host_di::DependencyResolver;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    struct Database;

    impl Service for Database {
        fn construct<R: DependencyResolver>(_: &R) -> Result<Self, ResolveError> {
            Ok(Database)
        }
    }

    fn within_timeout(body: impl FnOnce() + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            body();
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("resolution deadlocked");
    }

    #[test]
    fn singleton_resolves_without_deadlock() {
        within_timeout(|| {
            let mut builder = HostBuilder::new();
            builder.services().register::<Database>(Lifetime::Singleton);
            let host = builder.build();
            let first = host.resolve::<Database>().unwrap();
            let second = host.resolve::<Database>().unwrap();
            assert!(Arc::ptr_eq(&first, &second));
        });
    }

    #[test]
    fn scoped_resolves_without_deadlock() {
        within_timeout(|| {
            let mut builder = HostBuilder::new();
            builder.services().register::<Database>(Lifetime::Scoped);
            let host = builder.build();
            let scope = host.scope();
            let first = scope.resolve::<Database>().unwrap();
            let second = scope.resolve::<Database>().unwrap();
            assert!(Arc::ptr_eq(&first, &second));
        });
    }
}
