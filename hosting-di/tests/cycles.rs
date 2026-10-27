#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hosting_di::{
    BuildIssue, Dependency, ResolveError, Resolver, Service, ServiceCollection, ServiceProvider,
};
use std::any::type_name;
use std::sync::{Arc, OnceLock, mpsc};
use std::thread;
use std::time::Duration;

macro_rules! depends_on {
    ($name:ident => $($dep:ident),+) => {
        struct $name;

        impl Service for $name {
            fn dependencies() -> Vec<Dependency> {
                vec![$(Dependency::on::<$dep>()),*]
            }

            fn construct(resolver: &Resolver<'_>) -> Result<Self, ResolveError> {
                $(resolver.resolve::<$dep>()?;)*
                Ok($name)
            }
        }
    };
}

depends_on!(A => B);
depends_on!(B => C);
depends_on!(C => A);
depends_on!(Selfish => Selfish);
depends_on!(X => Y);
depends_on!(Y => X);
depends_on!(UsesFactory => FactoryMade);

struct Leaf;

impl Service for Leaf {
    fn dependencies() -> Vec<Dependency> {
        Vec::new()
    }

    fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Leaf)
    }
}

struct FactoryMade;
struct Left;
struct Right;

fn name<T>() -> &'static str {
    type_name::<T>()
}

#[test]
fn build_rejects_cycle_with_full_path() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<A>()
        .add_singleton::<B>()
        .add_singleton::<C>();

    let err = services.build().err().unwrap();

    assert!(matches!(err.issues(), [BuildIssue::Cycle { path }]
        if path == &[name::<A>(), name::<B>(), name::<C>(), name::<A>()]));
    assert_eq!(
        err.to_string(),
        format!(
            "dependency cycle: {} -> {} -> {} -> {}",
            name::<A>(),
            name::<B>(),
            name::<C>(),
            name::<A>()
        )
    );
}

#[test]
fn cycle_path_starts_at_earliest_registration() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<B>()
        .add_singleton::<C>()
        .add_singleton::<A>();

    let err = services.build().err().unwrap();

    assert!(matches!(err.issues(), [BuildIssue::Cycle { path }]
        if path == &[name::<B>(), name::<C>(), name::<A>(), name::<B>()]));
}

#[test]
fn build_rejects_self_dependency() {
    let mut services = ServiceCollection::new();
    services.add_scoped::<Selfish>();

    let err = services.build().err().unwrap();

    assert!(matches!(err.issues(), [BuildIssue::Cycle { path }]
        if path == &[name::<Selfish>(), name::<Selfish>()]));
}

#[test]
fn build_reports_every_cycle() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<A>()
        .add_singleton::<B>()
        .add_singleton::<C>()
        .add_singleton::<X>()
        .add_singleton::<Y>()
        .add_singleton::<Leaf>();

    let err = services.build().err().unwrap();

    assert_eq!(err.issues().len(), 2);
}

#[test]
fn build_rejects_transient_cycle() {
    let mut services = ServiceCollection::new();
    services.add_transient::<X>().add_transient::<Y>();

    assert!(services.build().is_err());
}

#[test]
fn unregistered_dependencies_are_not_build_errors() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<A>();

    assert!(services.build().is_ok());
}

#[test]
fn factory_cycle_is_a_resolution_error() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_with(|r| {
            r.resolve::<UsesFactory>()?;
            Ok(FactoryMade)
        })
        .add_singleton::<UsesFactory>();
    let provider = services.build().unwrap();

    let err = provider.resolve::<FactoryMade>().err().unwrap();

    assert!(matches!(&err, ResolveError::Cycle { path }
        if path == &[name::<FactoryMade>(), name::<UsesFactory>(), name::<FactoryMade>()]));
}

struct Locator;

#[test]
fn cycle_through_captured_provider_is_a_resolution_error() {
    let provider_slot = Arc::new(OnceLock::<ServiceProvider>::new());
    let captured = Arc::clone(&provider_slot);
    let mut services = ServiceCollection::new();
    services.add_singleton_with(move |_| {
        if let Some(provider) = captured.get() {
            provider.resolve::<Locator>()?;
        }
        Ok(Locator)
    });
    let provider = services.build().unwrap();
    let _ = provider_slot.set(provider.clone());

    let err = provider.resolve::<Locator>().err().unwrap();

    assert!(matches!(&err, ResolveError::Cycle { path }
        if path.first() == Some(&name::<Locator>()) && path.last() == Some(&name::<Locator>())));
}

#[test]
fn concurrent_factory_cycle_does_not_deadlock() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_with(|r| {
            thread::sleep(Duration::from_millis(20));
            r.resolve::<Right>()?;
            Ok(Left)
        })
        .add_singleton_with(|r| {
            thread::sleep(Duration::from_millis(20));
            r.resolve::<Left>()?;
            Ok(Right)
        });
    let provider = services.build().unwrap();
    let (tx, rx) = mpsc::channel();

    for start_left in [true, false] {
        let provider = provider.clone();
        let tx = tx.clone();
        thread::spawn(move || {
            let err = if start_left {
                provider.resolve::<Left>().map(|_| ()).err()
            } else {
                provider.resolve::<Right>().map(|_| ()).err()
            };
            let _ = tx.send(err);
        });
    }

    for _ in 0..2 {
        let err = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("cyclic resolution deadlocked");
        assert!(matches!(err, Some(ResolveError::Cycle { .. })));
    }
}
