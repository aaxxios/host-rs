//! Application host built around a dependency injection container.

#![warn(missing_docs, missing_debug_implementations)]

mod builder;

pub use builder::HostBuilder;

use hosting_di::ServiceProvider;

/// A built application host.
#[derive(Debug)]
pub struct Host {
    services: ServiceProvider,
}

impl Host {
    /// Starts configuring a host.
    pub fn builder() -> HostBuilder {
        HostBuilder::new()
    }

    /// The root service provider.
    pub fn services(&self) -> &ServiceProvider {
        &self.services
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hosting_di::{Dependency, ResolveError, Resolver, Service};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::Duration;

    struct Database;

    impl Service for Database {
        fn dependencies() -> Vec<Dependency> {
            Vec::new()
        }

        fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
            Ok(Database)
        }
    }

    #[test]
    fn singleton_resolves_without_deadlock() {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut builder = Host::builder();
            builder.services().add_singleton::<Database>();
            let host = builder.build().unwrap();
            let first = host.services().resolve::<Database>().unwrap();
            let second = host.services().resolve::<Database>().unwrap();
            let _ = tx.send(Arc::ptr_eq(&first, &second));
        });
        let shared = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("resolution deadlocked");
        assert!(shared);
    }
}
