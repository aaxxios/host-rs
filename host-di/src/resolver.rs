use crate::collection::Registration;
use crate::container::{Container, Erased};
use crate::key::Key;
use crate::{BoxError, Lifetime, ResolveError};
use std::sync::Arc;

/// Resolves dependencies while a service is being constructed.
///
/// Each construction gets its own frame linked to the frame that requested it,
/// so the chain of services under construction lives on the stack.
pub struct Resolver<'a> {
    container: &'a Container,
    parent: Option<&'a Resolver<'a>>,
    service: Option<Key>,
}

impl<'a> Resolver<'a> {
    pub(crate) fn root(container: &'a Container) -> Self {
        Self {
            container,
            parent: None,
            service: None,
        }
    }

    /// Resolves a dependency of the service under construction.
    pub fn resolve<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Arc<T>, ResolveError> {
        let key = Key::of::<T>();
        let Some(registration) = self.container.registration(key) else {
            return Err(ResolveError::NotRegistered {
                path: self.path_to(key),
            });
        };

        let instance = match registration.lifetime {
            Lifetime::Singleton => self.singleton(registration)?,
            Lifetime::Scoped => {
                return Err(ResolveError::ScopedFromRoot {
                    path: self.path_to(key),
                });
            }
            Lifetime::Transient => self.construct(registration)?,
        };

        registration
            .cast
            .downcast_ref::<fn(Erased) -> Option<Arc<T>>>()
            .and_then(|cast| cast(instance))
            .ok_or_else(|| ResolveError::NotRegistered {
                path: self.path_to(key),
            })
    }

    /// Wraps a constructor failure with the current dependency path.
    pub fn construction_error(&self, source: impl Into<BoxError>) -> ResolveError {
        ResolveError::Construction {
            path: self.path(),
            source: source.into(),
        }
    }

    fn singleton(&self, registration: &Registration) -> Result<Erased, ResolveError> {
        let Some(slot) = self.container.singletons.get(registration.slot) else {
            return Err(ResolveError::NotRegistered {
                path: self.path_to(registration.key),
            });
        };
        if let Some(instance) = slot.get() {
            return Ok(Arc::clone(instance));
        }

        let _guard = self.container.construction.lock();
        if let Some(instance) = slot.get() {
            return Ok(Arc::clone(instance));
        }
        let instance = self.construct(registration)?;
        if slot.set(Arc::clone(&instance)).is_ok() {
            self.container.record_singleton(registration.slot);
        }
        Ok(instance)
    }

    fn construct(&self, registration: &Registration) -> Result<Erased, ResolveError> {
        let frame = Resolver {
            container: self.container,
            parent: Some(self),
            service: Some(registration.key),
        };
        let result = (registration.construct)(&frame);

        #[cfg(feature = "logging")]
        match &result {
            Ok(_) => tracing::debug!(
                service = registration.key.name,
                lifetime = ?registration.lifetime,
                "constructed service"
            ),
            Err(error) => tracing::debug!(
                service = registration.key.name,
                lifetime = ?registration.lifetime,
                %error,
                "service construction failed"
            ),
        }

        result
    }

    /// Services under construction, outermost first.
    fn path(&self) -> Vec<&'static str> {
        let mut path = Vec::new();
        let mut frame = Some(self);
        while let Some(current) = frame {
            if let Some(service) = current.service {
                path.push(service.name);
            }
            frame = current.parent;
        }
        path.reverse();
        path
    }

    fn path_to(&self, key: Key) -> Vec<&'static str> {
        let mut path = self.path();
        path.push(key.name);
        path
    }
}
