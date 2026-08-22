use crate::Host;
use host_di::{BuildError, ServiceCollection};

/// Configures and builds a [`Host`].
#[derive(Debug, Default)]
pub struct HostBuilder {
    services: ServiceCollection,
}

impl HostBuilder {
    /// Creates a builder with no services registered.
    pub fn new() -> Self {
        Self::default()
    }

    /// The services the host will provide.
    pub fn services(&mut self) -> &mut ServiceCollection {
        &mut self.services
    }

    /// Builds the host, rejecting an invalid service collection.
    pub fn build(self) -> Result<Host, BuildError> {
        Ok(Host {
            services: self.services.build()?,
        })
    }
}
