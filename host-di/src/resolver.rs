use crate::collection::{Registration, Source};
use crate::container::{Container, CreationLog, Erased};
use crate::key::Key;
use crate::scope::ScopeState;
use crate::sync::ConstructionLock;
use crate::{BoxError, Lifetime, ResolveError};
use std::sync::{Arc, OnceLock};

/// Resolves dependencies while a service is being constructed.
///
/// Each construction gets its own frame linked to the frame that requested it,
/// so the chain of services under construction lives on the stack.
pub struct Resolver<'a> {
    container: &'a Container,
    scope: Option<&'a ScopeState>,
    parent: Option<&'a Resolver<'a>>,
    service: Option<Key>,
}

impl<'a> Resolver<'a> {
    pub(crate) fn root(container: &'a Container) -> Self {
        Self {
            container,
            scope: None,
            parent: None,
            service: None,
        }
    }

    pub(crate) fn scoped(container: &'a Container, scope: &'a ScopeState) -> Self {
        Self {
            container,
            scope: Some(scope),
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
            Lifetime::Scoped => self.scoped_instance(registration)?,
            Lifetime::Transient => self.construct(registration, self.scope)?,
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
        let container = self.container;
        let slot = container.singletons.get(registration.slot);
        self.cached(
            registration,
            slot,
            &container.construction,
            &container.created,
            None,
        )
    }

    fn scoped_instance(&self, registration: &Registration) -> Result<Erased, ResolveError> {
        let Some(scope) = self.scope else {
            return Err(ResolveError::ScopedFromRoot {
                path: self.path_to(registration.key),
            });
        };
        let slot = scope.slot(self.container, registration.slot);
        self.cached(
            registration,
            slot,
            &scope.construction,
            &scope.created,
            Some(scope),
        )
    }

    /// Returns the instance in `slot`, constructing it under `lock` on first use.
    /// Failed constructions leave the slot empty so the next resolution retries.
    fn cached(
        &self,
        registration: &Registration,
        slot: Option<&OnceLock<Erased>>,
        lock: &ConstructionLock,
        created: &CreationLog,
        scope: Option<&ScopeState>,
    ) -> Result<Erased, ResolveError> {
        let Some(slot) = slot else {
            return Err(ResolveError::NotRegistered {
                path: self.path_to(registration.key),
            });
        };
        if let Some(instance) = slot.get() {
            return Ok(Arc::clone(instance));
        }

        let _guard = lock.lock();
        if let Some(instance) = slot.get() {
            return Ok(Arc::clone(instance));
        }
        let instance = self.construct(registration, scope)?;
        if slot.set(Arc::clone(&instance)).is_ok() {
            created.record(registration.slot);
        }
        Ok(instance)
    }

    fn construct(
        &self,
        registration: &Registration,
        scope: Option<&ScopeState>,
    ) -> Result<Erased, ResolveError> {
        if let Some(path) = self.cycle_to(registration.key) {
            return Err(ResolveError::Cycle { path });
        }
        let frame = Resolver {
            container: self.container,
            scope,
            parent: Some(self),
            service: Some(registration.key),
        };
        let result = match &registration.source {
            Source::Construct(construct) => construct(&frame),
            Source::Instance(instance) => Ok(Arc::clone(instance)),
        };

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

    /// The cycle closed by constructing `key` again, if it is already under
    /// construction in this chain.
    fn cycle_to(&self, key: Key) -> Option<Vec<&'static str>> {
        let mut frame = Some(self);
        while let Some(current) = frame {
            if current.service == Some(key) {
                let path = self.path_to(key);
                let start = path.iter().position(|&name| name == key.name)?;
                return Some(path[start..].to_vec());
            }
            frame = current.parent;
        }
        None
    }

    fn path_to(&self, key: Key) -> Vec<&'static str> {
        let mut path = self.path();
        path.push(key.name);
        path
    }
}
