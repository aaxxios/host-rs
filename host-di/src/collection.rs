use crate::container::{ConstructFn, Container, Erased};
use crate::graph::{find_captive, find_cycles, find_missing};
use crate::key::Key;
use crate::{
    BoxError, BuildError, BuildIssue, CastFrom, Dependency, Lifetime, Resolver, Service,
    ServiceProvider,
};
use std::any::{Any, type_name};
use std::fmt;
use std::sync::Arc;

/// Services registered for an application, frozen into a [`ServiceProvider`]
/// by [`build`](Self::build).
///
/// Registering a type again replaces the earlier registration. Singletons are
/// constructed outside any scope, so a singleton that depends on a scoped
/// service fails to resolve rather than holding on to one scope's instance.
/// Transients are constructed in the scope that requested them.
#[derive(Debug, Default)]
pub struct ServiceCollection {
    registrations: Vec<Registration>,
    validation: Validation,
}

/// Opt-in checks run by `build`.
#[derive(Debug, Default)]
struct Validation {
    dependencies: bool,
    scopes: bool,
}

pub(crate) struct Registration {
    pub(crate) key: Key,
    /// Type name of what is constructed; differs from the key's name when a
    /// service is registered as a trait.
    pub(crate) implementation: &'static str,
    pub(crate) lifetime: Lifetime,
    pub(crate) source: Source,
    /// Declared dependencies; `None` for factories and instances.
    pub(crate) dependencies: Option<fn() -> Vec<Dependency>>,
    /// A `fn(Erased) -> Option<Arc<I>>` for the registered service type `I`.
    pub(crate) cast: Box<dyn Any + Send + Sync>,
    /// Index into the storage of this registration's lifetime, set by the container.
    pub(crate) slot: usize,
}

impl fmt::Debug for Registration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registration")
            .field("service", &self.key.name)
            .field("implementation", &self.implementation)
            .field("lifetime", &self.lifetime)
            .finish_non_exhaustive()
    }
}

pub(crate) enum Source {
    Construct(ConstructFn),
    Instance(Erased),
}

impl ServiceCollection {
    /// Creates an empty collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `T` with one instance shared for the life of the provider.
    pub fn add_singleton<T: Service>(&mut self) -> &mut Self {
        self.add::<T>(Lifetime::Singleton)
    }

    /// Registers `T` with one instance per scope.
    pub fn add_scoped<T: Service>(&mut self) -> &mut Self {
        self.add::<T>(Lifetime::Scoped)
    }

    /// Registers `T` with a new instance on every resolution.
    pub fn add_transient<T: Service>(&mut self) -> &mut Self {
        self.add::<T>(Lifetime::Transient)
    }

    /// Registers `T` as an implementation of the trait object `I`, with one
    /// instance shared for the life of the provider.
    ///
    /// `T` is reachable only as `I`; register it separately to resolve it by
    /// its own type.
    pub fn add_singleton_as<I, T>(&mut self) -> &mut Self
    where
        I: ?Sized + CastFrom<T> + Send + Sync + 'static,
        T: Service,
    {
        self.add_service::<I, T>(Lifetime::Singleton, trait_cast::<I, T>())
    }

    /// Registers `T` as an implementation of `I`, with one instance per scope.
    pub fn add_scoped_as<I, T>(&mut self) -> &mut Self
    where
        I: ?Sized + CastFrom<T> + Send + Sync + 'static,
        T: Service,
    {
        self.add_service::<I, T>(Lifetime::Scoped, trait_cast::<I, T>())
    }

    /// Registers `T` as an implementation of `I`, constructed on every
    /// resolution.
    pub fn add_transient_as<I, T>(&mut self) -> &mut Self
    where
        I: ?Sized + CastFrom<T> + Send + Sync + 'static,
        T: Service,
    {
        self.add_service::<I, T>(Lifetime::Transient, trait_cast::<I, T>())
    }

    /// Whether anything is registered as `T`.
    ///
    /// Covers every form of registration, so it can guard any of them, for
    /// example to add a default only when the application hasn't.
    pub fn contains<T: ?Sized + 'static>(&self) -> bool {
        let key = Key::of::<T>();
        self.registrations
            .iter()
            .any(|registration| registration.key == key)
    }

    /// Like [`add_singleton`](Self::add_singleton), but only if nothing is
    /// registered as `T` yet.
    pub fn try_add_singleton<T: Service>(&mut self) -> &mut Self {
        if self.contains::<T>() {
            return self;
        }
        self.add_singleton::<T>()
    }

    /// Like [`add_scoped`](Self::add_scoped), but only if nothing is
    /// registered as `T` yet.
    pub fn try_add_scoped<T: Service>(&mut self) -> &mut Self {
        if self.contains::<T>() {
            return self;
        }
        self.add_scoped::<T>()
    }

    /// Like [`add_transient`](Self::add_transient), but only if nothing is
    /// registered as `T` yet.
    pub fn try_add_transient<T: Service>(&mut self) -> &mut Self {
        if self.contains::<T>() {
            return self;
        }
        self.add_transient::<T>()
    }

    /// Like [`add_singleton_as`](Self::add_singleton_as), but only if nothing
    /// is registered as `I` yet, whatever its implementation.
    pub fn try_add_singleton_as<I, T>(&mut self) -> &mut Self
    where
        I: ?Sized + CastFrom<T> + Send + Sync + 'static,
        T: Service,
    {
        if self.contains::<I>() {
            return self;
        }
        self.add_singleton_as::<I, T>()
    }

    /// Like [`add_scoped_as`](Self::add_scoped_as), but only if nothing is
    /// registered as `I` yet.
    pub fn try_add_scoped_as<I, T>(&mut self) -> &mut Self
    where
        I: ?Sized + CastFrom<T> + Send + Sync + 'static,
        T: Service,
    {
        if self.contains::<I>() {
            return self;
        }
        self.add_scoped_as::<I, T>()
    }

    /// Like [`add_transient_as`](Self::add_transient_as), but only if nothing
    /// is registered as `I` yet.
    pub fn try_add_transient_as<I, T>(&mut self) -> &mut Self
    where
        I: ?Sized + CastFrom<T> + Send + Sync + 'static,
        T: Service,
    {
        if self.contains::<I>() {
            return self;
        }
        self.add_transient_as::<I, T>()
    }

    /// Registers a factory for `T` with one instance shared for the life of the
    /// provider.
    ///
    /// Errors from resolving dependencies inside the factory keep their original
    /// path; any other error is reported as a construction failure of `T`.
    /// Like constructors, factories must not block on other threads that
    /// resolve from the same container.
    pub fn add_singleton_with<T, F>(&mut self, factory: F) -> &mut Self
    where
        T: Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<T, BoxError> + Send + Sync + 'static,
    {
        self.add_factory(Lifetime::Singleton, factory)
    }

    /// Registers a factory for `T` with one instance per scope.
    pub fn add_scoped_with<T, F>(&mut self, factory: F) -> &mut Self
    where
        T: Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<T, BoxError> + Send + Sync + 'static,
    {
        self.add_factory(Lifetime::Scoped, factory)
    }

    /// Registers a factory for `T` that runs on every resolution.
    pub fn add_transient_with<T, F>(&mut self, factory: F) -> &mut Self
    where
        T: Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<T, BoxError> + Send + Sync + 'static,
    {
        self.add_factory(Lifetime::Transient, factory)
    }

    /// Registers an already constructed singleton.
    pub fn add_singleton_instance<T: Send + Sync + 'static>(&mut self, instance: T) -> &mut Self {
        self.push_unchecked::<T>(Lifetime::Singleton, Source::Instance(Arc::new(instance)))
    }

    /// Registers a factory producing the trait object `I`, with one instance
    /// shared for the life of the provider.
    ///
    /// Unlike [`add_singleton_as`](Self::add_singleton_as), the trait needs no
    /// `#[abstraction]`, so traits from other crates can be registered. The
    /// concrete type behind the returned `Arc` is not known, so errors and
    /// logs name these registrations by `I`.
    pub fn add_singleton_as_with<I, F>(&mut self, factory: F) -> &mut Self
    where
        I: ?Sized + Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<Arc<I>, BoxError> + Send + Sync + 'static,
    {
        self.add_factory_as(Lifetime::Singleton, factory)
    }

    /// Registers a factory producing the trait object `I`, with one instance
    /// per scope.
    pub fn add_scoped_as_with<I, F>(&mut self, factory: F) -> &mut Self
    where
        I: ?Sized + Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<Arc<I>, BoxError> + Send + Sync + 'static,
    {
        self.add_factory_as(Lifetime::Scoped, factory)
    }

    /// Registers a factory producing the trait object `I`, run on every
    /// resolution.
    ///
    /// The returned `Arc` is stored behind one more allocation, so each
    /// resolution allocates once beyond what the factory does.
    pub fn add_transient_as_with<I, F>(&mut self, factory: F) -> &mut Self
    where
        I: ?Sized + Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<Arc<I>, BoxError> + Send + Sync + 'static,
    {
        self.add_factory_as(Lifetime::Transient, factory)
    }

    /// Registers an already constructed trait object as a singleton.
    pub fn add_singleton_instance_as<I>(&mut self, instance: Arc<I>) -> &mut Self
    where
        I: ?Sized + Send + Sync + 'static,
    {
        self.push_trait_object::<I>(Lifetime::Singleton, Source::Instance(Arc::new(instance)))
    }

    /// Rejects at build any declared dependency that has no registration.
    ///
    /// Without it, a missing dependency surfaces when the service is first
    /// resolved. A `Vec<Arc<T>>` dependency is never missing, since it may be
    /// empty, and factories are not checked because their dependencies are
    /// not declared.
    pub fn validate_dependencies(&mut self) -> &mut Self {
        self.validation.dependencies = true;
        self
    }

    /// Rejects at build any singleton that depends on a scoped service,
    /// directly or through transients.
    ///
    /// Without it, such a singleton fails to resolve with
    /// [`ResolveError::ScopedFromRoot`](crate::ResolveError::ScopedFromRoot);
    /// it never silently keeps one scope's instance.
    pub fn validate_scopes(&mut self) -> &mut Self {
        self.validation.scopes = true;
        self
    }

    /// Freezes the registrations into a provider.
    ///
    /// Fails if registered services depend on each other in a cycle, or if an
    /// enabled check finds a problem; every problem found is reported.
    pub fn build(self) -> Result<ServiceProvider, BuildError> {
        let registrations = &self.registrations;
        let mut issues: Vec<BuildIssue> = find_cycles(registrations)
            .into_iter()
            .map(|path| BuildIssue::Cycle { path })
            .collect();
        if self.validation.dependencies {
            issues.extend(
                find_missing(registrations)
                    .into_iter()
                    .map(|path| BuildIssue::MissingDependency { path }),
            );
        }
        if self.validation.scopes {
            issues.extend(
                find_captive(registrations)
                    .into_iter()
                    .map(|path| BuildIssue::CaptiveDependency { path }),
            );
        }
        if !issues.is_empty() {
            return Err(BuildError::new(issues));
        }
        Ok(ServiceProvider::new(Container::new(self.registrations)))
    }

    fn add<T: Service>(&mut self, lifetime: Lifetime) -> &mut Self {
        self.add_service::<T, T>(lifetime, concrete_cast::<T>())
    }

    fn add_service<I: ?Sized + 'static, T: Service>(
        &mut self,
        lifetime: Lifetime,
        cast: Cast,
    ) -> &mut Self {
        let construct: ConstructFn =
            Arc::new(|resolver| T::construct(resolver).map(|service| Arc::new(service) as Erased));
        self.push(Registration {
            key: Key::of::<I>(),
            implementation: type_name::<T>(),
            lifetime,
            source: Source::Construct(construct),
            dependencies: Some(T::dependencies),
            cast,
            slot: 0,
        })
    }

    fn create_factory_constr<F, T>(factory: F) -> ConstructFn
    where
        F: Fn(&Resolver<'_>) -> Result<T, BoxError> + Send + Sync + 'static,
        T: Send + Sync + 'static,
    {
        Arc::new(move |resolver| match factory(resolver) {
            Ok(service) => Ok(Arc::new(service) as Erased),
            Err(error) => Err(resolver.failure(error)),
        })
    }

    fn add_factory<T, F>(&mut self, lifetime: Lifetime, factory: F) -> &mut Self
    where
        T: Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<T, BoxError> + Send + Sync + 'static,
    {
        let construct: ConstructFn = Self::create_factory_constr(factory);
        self.push_unchecked::<T>(lifetime, Source::Construct(construct))
    }

    /// Registers a factory or instance whose dependencies are not declared.
    fn push_unchecked<T: Send + Sync + 'static>(
        &mut self,
        lifetime: Lifetime,
        source: Source,
    ) -> &mut Self {
        let key = Key::of::<T>();
        self.push(Registration {
            key,
            implementation: key.name,
            lifetime,
            source,
            dependencies: None,
            cast: concrete_cast::<T>(),
            slot: 0,
        })
    }

    fn add_factory_as<I, F>(&mut self, lifetime: Lifetime, factory: F) -> &mut Self
    where
        I: ?Sized + Send + Sync + 'static,
        F: Fn(&Resolver<'_>) -> Result<Arc<I>, BoxError> + Send + Sync + 'static,
    {
        let construct: ConstructFn = Self::create_factory_constr(factory);
        self.push_trait_object::<I>(lifetime, Source::Construct(construct))
    }

    /// Registers an `Arc<I>` stored as is, since an unsized `I` cannot be
    /// erased directly.
    fn push_trait_object<I>(&mut self, lifetime: Lifetime, source: Source) -> &mut Self
    where
        I: ?Sized + Send + Sync + 'static,
    {
        let key = Key::of::<I>();
        let cast: fn(Erased) -> Option<Arc<I>> = |instance| {
            instance
                .downcast::<Arc<I>>()
                .ok()
                .map(|arc| Arc::clone(arc.as_ref()))
        };
        self.push(Registration {
            key,
            implementation: key.name,
            lifetime,
            source,
            dependencies: None,
            cast: Box::new(cast),
            slot: 0,
        })
    }

    fn push(&mut self, registration: Registration) -> &mut Self {
        self.registrations.push(registration);
        self
    }
}

/// A boxed `fn(Erased) -> Option<Arc<I>>` for the registered service type `I`.
type Cast = Box<dyn Any + Send + Sync>;

fn trait_cast<I, T>() -> Cast
where
    I: ?Sized + CastFrom<T> + 'static,
    T: Send + Sync + 'static,
{
    let cast: fn(Erased) -> Option<Arc<I>> = |instance| instance.downcast::<T>().ok().map(I::cast);
    Box::new(cast)
}

fn concrete_cast<T: Send + Sync + 'static>() -> Cast {
    let cast: fn(Erased) -> Option<Arc<T>> = |instance| instance.downcast().ok();
    Box::new(cast)
}
