pub mod scope;
pub mod service_collection;

use crate::service_collection::ServiceCollection;
pub use host_macros::service;
use std::{
    any::{Any, TypeId},
    sync::Arc,
};

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
    CachedPoisoned,
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

#[cfg(feature = "logging")]
pub(crate) fn log(message: &str) {
    tracing::info!("{message}");
}

#[cfg(not(feature = "logging"))]
pub(crate) fn log(_message: &str) {}

pub fn downcast<T>(value: ErasedService) -> Result<Arc<T>, ResolveError>
where
    T: Send + Sync + 'static,
{
    value.downcast::<T>().map_err(|_| {
        log("invalid service");
        ResolveError::InvalidService
    })
}
