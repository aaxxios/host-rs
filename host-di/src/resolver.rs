use crate::collection::{Registration, Source};
use crate::container::{Container, Erased, Slots};
use crate::key::Key;
use crate::scope::ScopeState;
use crate::sync::ConstructionLock;
use crate::{BoxError, Lifetime, ResolveError};
use std::cell::RefCell;
use std::fmt;
use std::ptr;
use std::sync::Arc;

/// Resolves dependencies while a service is being constructed.
///
/// Each construction gets its own frame linked to the frame that requested it,
/// so the chain of services under construction lives on the stack.
///
/// Use it only on the constructing thread. Construction of cached instances is
/// serialised per container and per scope, so handing a resolver to another
/// thread and waiting for it deadlocks.
pub struct Resolver<'a> {
    container: &'a Container,
    scope: Option<&'a ScopeState>,
    parent: Option<&'a Resolver<'a>>,
    service: Option<&'a Registration>,
}

impl fmt::Debug for Resolver<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Resolver")
            .field("constructing", &self.path())
            .finish_non_exhaustive()
    }
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

        let instance = self.instance(registration)?;
        self.cast(registration, instance)
    }

    /// Resolves every registration of `T`, in registration order. The first
    /// failure fails the whole call; nothing is cached for the failed member.
    pub fn resolve_all<T: ?Sized + Send + Sync + 'static>(
        &self,
    ) -> Result<Vec<Arc<T>>, ResolveError> {
        let registrations = self.container.registrations_of(Key::of::<T>());
        let mut all = Vec::with_capacity(registrations.len());
        for registration in registrations {
            let instance = self.instance(registration)?;
            all.push(self.cast(registration, instance)?);
        }
        Ok(all)
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

    fn instance(&self, registration: &Registration) -> Result<Erased, ResolveError> {
        match registration.lifetime {
            Lifetime::Singleton => self.singleton(registration),
            Lifetime::Scoped => self.scoped_instance(registration),
            Lifetime::Transient => self.construct(registration, self.scope),
        }
    }

    fn cast<T: ?Sized + 'static>(
        &self,
        registration: &Registration,
        instance: Erased,
    ) -> Result<Arc<T>, ResolveError> {
        registration
            .cast
            .downcast_ref::<fn(Erased) -> Option<Arc<T>>>()
            .and_then(|cast| cast(instance))
            .ok_or_else(|| ResolveError::NotRegistered {
                path: self.path_to(registration.key),
            })
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
            // Name the implementation: when resolving a set, the service type
            // alone doesn't say which member is scoped.
            let mut path = self.path();
            path.push(registration.implementation);
            return Err(ResolveError::ScopedFromRoot { path });
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
        registration: &'a Registration,
        scope: Option<&'a ScopeState>,
    ) -> Result<Erased, ResolveError> {
        let _constructing =
            Constructing::enter(registration).map_err(|path| ResolveError::Cycle { path })?;
        let frame = Resolver {
            container: self.container,
            scope,
            parent: Some(self),
            service: Some(registration),
        };
        let result = match &registration.source {
            Source::Construct(construct) => construct(&frame),
            Source::Instance(instance) => Ok(Arc::clone(instance)),
        };

        #[cfg(feature = "logging")]
        match &result {
            Ok(_) => tracing::debug!(
                service = registration.implementation,
                lifetime = ?registration.lifetime,
                "constructed service"
            ),
            Err(error) => tracing::debug!(
                service = registration.implementation,
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
                path.push(service.implementation);
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
    /// Registrations under construction on this thread. Kept outside the frame
    /// chain so a resolution that starts a new chain on the same thread, such
    /// as a factory resolving through a captured provider, still sees what it
    /// is nested in. A registration belongs to exactly one container, so its
    /// address identifies it.
    static CONSTRUCTING: RefCell<Vec<(*const Registration, &'static str)>> =
        const { RefCell::new(Vec::new()) };
}

/// Marks a registration as under construction on this thread until dropped.
struct Constructing;

impl Constructing {
    /// Fails with the cycle path if `registration` is already under
    /// construction on this thread.
    fn enter(registration: &Registration) -> Result<Self, Vec<&'static str>> {
        let this: *const Registration = registration;
        CONSTRUCTING.with_borrow_mut(|stack| {
            if let Some(start) = stack.iter().position(|&(entry, _)| ptr::eq(entry, this)) {
                let mut path: Vec<_> = stack[start..].iter().map(|&(_, name)| name).collect();
                path.push(registration.implementation);
                return Err(path);
            }
            stack.push((this, registration.implementation));
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
