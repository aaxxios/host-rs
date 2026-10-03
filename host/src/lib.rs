//! Application hosting and dependency injection.
//!
//! ```
//! use host::{Service, ServiceCollection};
//! use std::sync::Arc;
//!
//! #[derive(Service)]
//! struct Database;
//!
//! #[derive(Service)]
//! struct Repository {
//!     db: Arc<Database>,
//! }
//!
//! let mut services = ServiceCollection::new();
//! services.add_singleton::<Database>().add_scoped::<Repository>();
//! let provider = services.build()?;
//!
//! let scope = provider.create_scope();
//! let repository = scope.resolve::<Repository>()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![warn(missing_docs, missing_debug_implementations)]

pub use host_core::{Host, HostBuilder};
pub use host_di::{
    BoxError, BuildError, BuildIssue, CastFrom, Dependency, Lifetime, ResolveError, Resolver,
    Scope, Service, ServiceCollection, ServiceProvider,
};
pub use host_macros::{Service, abstraction};

#[doc(hidden)]
pub use host_di::__private;
