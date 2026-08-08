use crate::collection::{Registration, Source};
use crate::container::{Container, Erased, Slots};
use crate::key::Key;
use crate::scope::ScopeState;
use crate::sync::ConstructionLock;
use crate::{BoxError, Lifetime, ResolveError};
use std::cell::RefCell;
use std::ptr;
use std::sync::Arc;

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

    /// Like [`construction_error`](Self::construction_error), but passes a
    /// dependency's `ResolveError` through so its path is kept.
    pub(crate) fn failure(&self, error: BoxError) -> ResolveError {
        match error.downcast::<ResolveError>() {
            Ok(error) => *error,
            Err(error) => self.construction_error(error),
        }
    }

    fn singleton(&self, registration: &Registration) -> Result<Erased, ResolveError> {
        let container = self.container;
        self.cached(
            registration,
            &container.singletons,
            &container.construction,
            None,
        )
    }

    fn scoped_instance(&self, registration: &Registration) -> Result<Erased, ResolveError> {
        let Some(scope) = self.scope else {
            return Err(ResolveError::ScopedFromRoot {
                path: self.path_to(registration.key),
            });
        };
        self.cached(
            registration,
            scope.slots(self.container),
            &scope.construction,
            Some(scope),
        )
    }

    /// Returns the cached instance, constructing it under `lock` on first use.
    /// Failed constructions store nothing, so the next resolution retries.
    fn cached(
        &self,
        registration: &Registration,
        slots: &Slots,
        lock: &ConstructionLock,
        scope: Option<&ScopeState>,
    ) -> Result<Erased, ResolveError> {
        if let Some(instance) = slots.get(registration.slot) {
            return Ok(instance);
        }

        let _guard = lock.lock();
        if let Some(instance) = slots.get(registration.slot) {
            return Ok(instance);
        }
        let instance = self.construct(registration, scope)?;
        slots.fill(registration.slot, Arc::clone(&instance));
        Ok(instance)
    }

    fn construct(
        &self,
        registration: &Registration,
        scope: Option<&ScopeState>,
    ) -> Result<Erased, ResolveError> {
        let _constructing = Constructing::enter(self.container, registration.key)
            .map_err(|path| ResolveError::Cycle { path })?;
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

    fn path_to(&self, key: Key) -> Vec<&'static str> {
        let mut path = self.path();
        path.push(key.name);
        path
    }
}

thread_local! {
    /// Services under construction on this thread, per container. Kept outside
    /// the frame chain so a resolution that starts a new chain on the same
    /// thread, such as a factory resolving through a captured provider, still
    /// sees the services it is nested in.
    static CONSTRUCTING: RefCell<Vec<(*const Container, Key)>> = const { RefCell::new(Vec::new()) };
}

/// Marks a service as under construction on this thread until dropped.
struct Constructing;

impl Constructing {
    /// Fails with the cycle path if `key` is already under construction in
    /// `container` on this thread.
    fn enter(container: &Container, key: Key) -> Result<Self, Vec<&'static str>> {
        let container: *const Container = container;
        CONSTRUCTING.with_borrow_mut(|stack| {
            let ours = stack
                .iter()
                .filter(|(owner, _)| ptr::eq(*owner, container))
                .map(|&(_, service)| service);
            if let Some(start) = ours.clone().position(|service| service == key) {
                let mut path: Vec<_> = ours.skip(start).map(|service| service.name).collect();
                path.push(key.name);
                return Err(path);
            }
            stack.push((container, key));
            Ok(Constructing)
        })
    }
}

impl Drop for Constructing {
    fn drop(&mut self) {
        CONSTRUCTING.with_borrow_mut(|stack| {
            stack.pop();
        });
    }
}
