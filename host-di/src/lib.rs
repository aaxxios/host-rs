//! Dependency injection container: register services in a
//! [`ServiceCollection`], build it into a [`ServiceProvider`], then resolve.

#![warn(missing_docs)]

mod collection;
mod container;
mod error;
mod key;
mod resolver;
mod scope;
mod service;
mod sync;

pub use collection::ServiceCollection;
pub use container::ServiceProvider;
pub use error::{BoxError, BuildError, BuildIssue, ResolveError};
pub use key::Dependency;
pub use resolver::Resolver;
pub use scope::Scope;
pub use service::Service;

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
