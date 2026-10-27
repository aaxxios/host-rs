//! Application hosting and dependency injection.
//!
//! ```
//! use hosting::{Service, ServiceCollection, abstraction};
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
//! #[abstraction]
//! trait Notifier: Send + Sync {
//!     fn notify(&self, message: &str);
//! }
//!
//! #[derive(Service)]
//! struct Email;
//!
//! impl Notifier for Email {
//!     fn notify(&self, _message: &str) {}
//! }
//!
//! #[derive(Service)]
//! struct Signup {
//!     repository: Arc<Repository>,
//!     notifiers: Vec<Arc<dyn Notifier>>,
//! }
//!
//! let mut services = ServiceCollection::new();
//! services
//!     .add_singleton::<Database>()
//!     .add_scoped::<Repository>()
//!     .add_singleton_as::<dyn Notifier, Email>()
//!     .add_transient::<Signup>();
//! let provider = services.build()?;
//!
//! let scope = provider.create_scope();
//! let signup = scope.resolve::<Signup>()?;
//! for notifier in &signup.notifiers {
//!     notifier.notify("welcome");
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![warn(missing_docs, missing_debug_implementations)]

pub use hosting_core::{Host, HostBuilder};
pub use hosting_di::{
    BoxError, BuildError, BuildIssue, CastFrom, Dependency, Lifetime, ResolveError, Resolver,
    Scope, Service, ServiceCollection, ServiceProvider,
};
pub use hosting_macros::{Service, abstraction};

#[doc(hidden)]
pub use hosting_di::__private;
