#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use host::{BuildIssue, Service, ServiceCollection};
use std::sync::Arc;

#[derive(Service)]
struct Database;

#[derive(Service)]
struct Repository {
    db: Arc<Database>,
}

#[derive(Service)]
struct Pair(Arc<Database>, Arc<Repository>);

#[derive(Service)]
struct Repo<D: Send + Sync + 'static> {
    db: Arc<D>,
}

#[derive(Service)]
#[service(crate = ::host)]
struct Explicit;

#[derive(Service)]
struct Ping {
    _pong: Arc<Pong>,
}

#[derive(Service)]
struct Pong {
    _ping: Arc<Ping>,
}

#[test]
fn derived_fields_are_injected() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<Database>()
        .add_transient::<Repository>();
    let provider = services.build().unwrap();

    let repository = provider.resolve::<Repository>().unwrap();

    assert!(Arc::ptr_eq(
        &repository.db,
        &provider.resolve::<Database>().unwrap()
    ));
}

#[test]
fn tuple_struct_is_injected() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<Database>()
        .add_singleton::<Repository>()
        .add_transient::<Pair>();
    let provider = services.build().unwrap();

    let pair = provider.resolve::<Pair>().unwrap();

    assert!(Arc::ptr_eq(&pair.0, &pair.1.db));
}

#[test]
fn generic_struct_is_injected() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<Database>()
        .add_singleton::<Repo<Database>>();
    let provider = services.build().unwrap();

    let repo = provider.resolve::<Repo<Database>>().unwrap();

    assert!(Arc::ptr_eq(
        &repo.db,
        &provider.resolve::<Database>().unwrap()
    ));
}

#[test]
fn crate_override_compiles_and_resolves() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<Explicit>();

    assert!(services.build().unwrap().resolve::<Explicit>().is_ok());
}

#[test]
fn derived_dependencies_feed_cycle_check() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<Ping>().add_singleton::<Pong>();

    let err = services.build().err().unwrap();

    assert!(matches!(err.issues(), [BuildIssue::Cycle { .. }]));
}
