use crate::Host;
use host_di::service_collection::ServiceCollection;
use std::sync::Arc;

#[derive(Default)]
pub struct HostBuilder {
    services: ServiceCollection,
}

impl HostBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn services(&mut self) -> &mut ServiceCollection {
        &mut self.services
    }

    pub fn build(self) -> Host {
        Host {
            services: Arc::new(self.services),
        }
    }
}
