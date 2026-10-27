#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hosting_di::{Dependency, ResolveError, Resolver, Service, ServiceCollection};
use std::cell::RefCell;

thread_local! {
    static DROPPED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn dropped() -> Vec<&'static str> {
    DROPPED.with(|log| log.borrow_mut().drain(..).collect())
}

macro_rules! tracked {
    ($($name:ident),*) => {$(
        struct $name;

        impl Service for $name {
            fn dependencies() -> Vec<Dependency> {
                Vec::new()
            }

            fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
                Ok($name)
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                DROPPED.with(|log| log.borrow_mut().push(stringify!($name)));
            }
        }
    )*};
}

tracked!(A, B, C, First, Second);

#[test]
fn singletons_drop_in_reverse_creation_order() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<A>()
        .add_singleton::<B>()
        .add_singleton::<C>();
    let provider = services.build().unwrap();

    provider.resolve::<C>().unwrap();
    provider.resolve::<A>().unwrap();
    provider.resolve::<B>().unwrap();
    assert!(dropped().is_empty());
    drop(provider);

    assert_eq!(dropped(), ["B", "A", "C"]);
}

#[test]
fn scoped_drop_in_reverse_creation_order_when_scope_drops() {
    let mut services = ServiceCollection::new();
    services
        .add_scoped::<A>()
        .add_scoped::<B>()
        .add_scoped::<C>();
    let provider = services.build().unwrap();
    let scope = provider.create_scope();

    scope.resolve::<C>().unwrap();
    scope.resolve::<A>().unwrap();
    scope.resolve::<B>().unwrap();
    drop(scope);

    assert_eq!(dropped(), ["B", "A", "C"]);
}

#[test]
fn instances_drop_after_later_constructed_singletons() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_instance(First)
        .add_singleton_instance(Second)
        .add_singleton::<A>()
        .add_singleton::<B>();
    let provider = services.build().unwrap();

    provider.resolve::<B>().unwrap();
    provider.resolve::<A>().unwrap();
    drop(provider);

    assert_eq!(dropped(), ["A", "B", "Second", "First"]);
}

#[test]
fn scoped_drop_before_singletons_when_scope_outlives_provider() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<A>().add_scoped::<B>();
    let provider = services.build().unwrap();
    let scope = provider.create_scope();

    scope.resolve::<A>().unwrap();
    scope.resolve::<B>().unwrap();
    drop(provider);
    drop(scope);

    assert_eq!(dropped(), ["B", "A"]);
}

#[test]
fn singletons_outlive_provider_while_a_scope_holds_the_container() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<A>();
    let provider = services.build().unwrap();
    let scope = provider.create_scope();

    provider.resolve::<A>().unwrap();
    drop(provider);
    assert!(dropped().is_empty());
    drop(scope);

    assert_eq!(dropped(), ["A"]);
}
