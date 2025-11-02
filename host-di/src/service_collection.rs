use crate::{ErasedService, Lifetime, Service};
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::RwLock;

pub struct Registration {
    pub lifetime: Lifetime,
}

/// collection of registered services
pub struct ServiceCollection {
    registrations: HashMap<TypeId, Registration>,

    singletons: RwLock<HashMap<TypeId, ErasedService>>,
}

impl ServiceCollection {
    pub fn new() -> Self {
        Self {
            registrations: HashMap::new(),
            singletons: RwLock::new(HashMap::new()),
        }
    }

    /// get registered services metadata
    pub fn metadata(&self) -> &HashMap<TypeId, Registration> {
        &self.registrations
    }

    pub fn singleton(&self) -> &RwLock<HashMap<TypeId, ErasedService>> {
        &self.singletons
    }

    pub fn register<T>(&mut self, lifetime: Lifetime)
    where
        T: Service,
    {
        self.registrations
            .insert(TypeId::of::<T>(), Registration { lifetime });
    }
}
