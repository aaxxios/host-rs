pub mod scope;
pub mod service_collection;

use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use crate::service_collection::ServiceCollection;
pub use host_macros::service;

pub type ErasedService = Arc<dyn Any + Send + Sync>;

/// the lifetime of a dependency
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifetime {
    /// service will be created once and reused for the lifetime of the host
    Singleton,

    /// service will be created once and reused for the lifetime of the scope
    Scoped,

    /// the service will always be created anew
    Transient,
}

#[derive(Debug)]
pub enum ResolveError {
    NotRegistered(TypeId),
    InvalidService,
    Construction(String),
}

/// Host where services are registered and resolved
pub trait ConfigHost {
    fn service_collection(&self) -> Arc<ServiceCollection>;

    fn resolve<T: Service>(&self) -> Result<Arc<T>, ResolveError>;
}

/// A type that can be constructed by a dependency resolver
pub trait Service: Sized + Send + Sync + 'static {
    fn construct<R: DependencyResolver>(resolver: &R) -> Result<Self, ResolveError>;
}

pub trait DependencyResolver {
    fn resolve<T>(&self) -> Result<Arc<T>, ResolveError>
    where
        T: Service;
}

pub fn downcast<T>(value: ErasedService) -> Result<Arc<T>, ResolveError>
where
    T: Send + Sync + 'static,
{
    value
        .downcast::<T>()
        .map_err(|_| ResolveError::InvalidService)
}

#[cfg(test)]
mod tests {
    use crate::*;
    use crate::{Lifetime, ServiceCollection, service};
    use std::sync::Arc;

    struct Database {
        name: String,
    }

    #[service]
    impl Database {
        fn new() -> Self {
            println!("Database::new");

            Self {
                name: "Postgres".into(),
            }
        }
    }

    struct Repository {
        db: Arc<Database>,
    }

    #[service]
    impl Repository {
        fn new(db: Arc<Database>) -> Self {
            println!("Repository::new");

            Self { db }
        }
    }

    struct UserService {
        repository: Arc<Repository>,
    }

    #[service]
    impl UserService {
        fn new(repository: Arc<Repository>) -> Self {
            println!("UserService::new");

            Self { repository }
        }
    }

    #[test]
    fn test_resolve() {
        let mut container = ServiceCollection::new();

        container.register::<UserService>(Lifetime::Transient);
        container.register::<Database>(Lifetime::Singleton);
        container.register::<Repository>(Lifetime::Scoped);

        // let host = container.build().unwrap();
        //
        // let scope = host.scope();
        //
        // let users = scope.resolve::<UserService>().unwrap();
        //
        // println!("database = {}", users.repository.db.name);
    }
}
