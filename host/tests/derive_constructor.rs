#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use host::{BuildIssue, ResolveError, Service, ServiceCollection};
use std::any::type_name;
use std::io;
use std::sync::Arc;

struct Config {
    url: String,
}

#[derive(Service)]
#[service(constructor = connect)]
struct Database {
    url: String,
}

impl Database {
    fn connect(config: Arc<Config>) -> Result<Self, io::Error> {
        if config.url.is_empty() {
            return Err(io::Error::other("missing url"));
        }
        Ok(Database {
            url: config.url.clone(),
        })
    }
}

#[derive(Service)]
#[service(constructor = new)]
struct Counter {
    start: u32,
}

impl Counter {
    fn new() -> Self {
        Counter { start: 7 }
    }
}

#[derive(Service, Default)]
#[service(constructor = default)]
struct Settings {
    retries: u32,
}

#[derive(Service)]
#[service(constructor = new)]
struct Ping {
    _pong: Arc<Pong>,
}

impl Ping {
    fn new(pong: Arc<Pong>) -> Self {
        Ping { _pong: pong }
    }
}

#[derive(Service)]
struct Pong {
    _ping: Arc<Ping>,
}

#[test]
fn constructor_params_are_injected() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_instance(Config {
            url: "pg://".into(),
        })
        .add_singleton::<Database>();
    let provider = services.build().unwrap();

    assert_eq!(provider.resolve::<Database>().unwrap().url, "pg://");
}

#[test]
fn infallible_constructor_returning_self() {
    let mut services = ServiceCollection::new();
    services.add_transient::<Counter>();

    let counter = services.build().unwrap().resolve::<Counter>().unwrap();

    assert_eq!(counter.start, 7);
}

#[test]
fn constructor_error_is_construction_error() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_instance(Config { url: String::new() })
        .add_singleton::<Database>();
    let provider = services.build().unwrap();

    let err = provider.resolve::<Database>().err().unwrap();

    let ResolveError::Construction { path, source } = &err else {
        panic!("unexpected error: {err}");
    };
    assert_eq!(path, &[type_name::<Database>()]);
    assert!(source.downcast_ref::<io::Error>().is_some());
}

#[test]
fn default_as_constructor() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<Settings>();

    let settings = services.build().unwrap().resolve::<Settings>().unwrap();

    assert_eq!(settings.retries, 0);
}

#[test]
fn constructor_dependencies_feed_cycle_check() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<Ping>().add_singleton::<Pong>();

    let err = services.build().err().unwrap();

    assert!(matches!(err.issues(), [BuildIssue::Cycle { .. }]));
}
