#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hosting_di::{CastFrom, Dependency, ResolveError, Resolver, Service, ServiceCollection};
use std::sync::Arc;

trait Sender: Send + Sync {
    fn name(&self) -> &'static str;
}

impl<T: Sender + 'static> CastFrom<T> for dyn Sender {
    fn cast(from: Arc<T>) -> Arc<Self> {
        from
    }
}

macro_rules! sender {
    ($type:ident, $name:literal) => {
        struct $type;

        impl Sender for $type {
            fn name(&self) -> &'static str {
                $name
            }
        }

        impl Service for $type {
            fn dependencies() -> Vec<Dependency> {
                Vec::new()
            }

            fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
                Ok($type)
            }
        }
    };
}

sender!(Smtp, "smtp");
sender!(Ses, "ses");

#[test]
fn try_add_registers_when_absent() {
    let mut services = ServiceCollection::new();
    services.try_add_singleton::<Smtp>();

    assert!(services.contains::<Smtp>());
    let provider = services.build().unwrap();
    assert!(Arc::ptr_eq(
        &provider.resolve::<Smtp>().unwrap(),
        &provider.resolve::<Smtp>().unwrap()
    ));
}

#[test]
fn try_add_keeps_the_existing_registration() {
    let mut services = ServiceCollection::new();
    services.add_transient::<Smtp>().try_add_singleton::<Smtp>();
    let provider = services.build().unwrap();

    assert_eq!(provider.resolve_all::<Smtp>().unwrap().len(), 1);
    assert!(!Arc::ptr_eq(
        &provider.resolve::<Smtp>().unwrap(),
        &provider.resolve::<Smtp>().unwrap()
    ));
}

#[test]
fn try_add_as_checks_the_trait_not_the_implementation() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_as::<dyn Sender, Smtp>()
        .try_add_singleton_as::<dyn Sender, Ses>()
        .try_add_scoped_as::<dyn Sender, Ses>()
        .try_add_transient_as::<dyn Sender, Ses>();
    let provider = services.build().unwrap();

    let names: Vec<_> = provider
        .resolve_all::<dyn Sender>()
        .unwrap()
        .iter()
        .map(|sender| sender.name())
        .collect();
    assert_eq!(names, ["smtp"]);
}

#[test]
fn try_add_scoped_and_transient_register_when_absent() {
    let mut services = ServiceCollection::new();
    services
        .try_add_scoped::<Smtp>()
        .try_add_transient_as::<dyn Sender, Ses>();
    let provider = services.build().unwrap();
    let scope = provider.create_scope();

    assert!(Arc::ptr_eq(
        &scope.resolve::<Smtp>().unwrap(),
        &scope.resolve::<Smtp>().unwrap()
    ));
    assert_eq!(provider.resolve::<dyn Sender>().unwrap().name(), "ses");
}

#[test]
fn contains_sees_factories_and_instances() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_with(|_| Ok(Smtp))
        .add_singleton_instance_as::<dyn Sender>(Arc::new(Ses));

    assert!(services.contains::<Smtp>());
    assert!(services.contains::<dyn Sender>());
    assert!(!services.contains::<Ses>());
}
