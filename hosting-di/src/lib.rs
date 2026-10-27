//! Dependency injection container: register services in a
//! [`ServiceCollection`], build it into a [`ServiceProvider`], then resolve.

#![warn(missing_docs, missing_debug_implementations)]

mod abstraction;
mod collection;
mod container;
mod error;
mod graph;
mod key;
mod resolver;
mod scope;
mod service;
mod sync;

pub use abstraction::CastFrom;
pub use collection::ServiceCollection;
pub use container::ServiceProvider;
pub use error::{BoxError, BuildError, BuildIssue, ResolveError};
pub use key::Dependency;
pub use resolver::Resolver;
pub use scope::Scope;
pub use service::Service;

#[doc(hidden)]
pub mod __private {
    pub use crate::service::{Bound, Constructor, ConstructorOutput, Inject, constructor};
    pub use crate::{CastFrom, Dependency, ResolveError, Resolver, Service};
}

/// How long a resolved instance is reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifetime {
    /// One instance for the life of the provider.
    Singleton,
    /// One instance per scope.
    Scoped,
    /// A new instance on every resolution.
    Transient,
}
