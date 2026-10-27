#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use hosting_di::{ResolveError, ServiceCollection};
use std::any::type_name;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

struct Config {
    port: u16,
}

struct Client {
    port: u16,
}

struct Missing;

#[test]
fn singleton_factory_runs_once_and_resolves_dependencies() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let mut services = ServiceCollection::new();
    services
        .add_singleton_instance(Config { port: 8080 })
        .add_singleton_with(move |r| {
            counter.fetch_add(1, SeqCst);
            Ok(Client {
                port: r.resolve::<Config>()?.port,
            })
        });
    let provider = services.build().unwrap();

    let first = provider.resolve::<Client>().unwrap();
    let second = provider.resolve::<Client>().unwrap();

    assert_eq!(first.port, 8080);
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(calls.load(SeqCst), 1);
}

#[test]
fn transient_factory_runs_per_resolve() {
    let mut services = ServiceCollection::new();
    services.add_transient_with(|_| Ok(Client { port: 1 }));
    let provider = services.build().unwrap();

    let first = provider.resolve::<Client>().unwrap();
    let second = provider.resolve::<Client>().unwrap();

    assert!(!Arc::ptr_eq(&first, &second));
}

#[test]
fn scoped_factory_runs_once_per_scope() {
    let mut services = ServiceCollection::new();
    services.add_scoped_with(|_| Ok(Client { port: 1 }));
    let provider = services.build().unwrap();
    let scope = provider.create_scope();

    let first = scope.resolve::<Client>().unwrap();
    let second = scope.resolve::<Client>().unwrap();
    let other = provider.create_scope().resolve::<Client>().unwrap();

    assert!(Arc::ptr_eq(&first, &second));
    assert!(!Arc::ptr_eq(&first, &other));
}

#[test]
fn factory_error_is_wrapped_with_path() {
    let mut services = ServiceCollection::new();
    services.add_singleton_with::<Client, _>(|_| Err(io::Error::other("refused").into()));
    let provider = services.build().unwrap();

    let err = provider.resolve::<Client>().err().unwrap();

    let ResolveError::Construction { path, source } = &err else {
        panic!("unexpected error: {err}");
    };
    assert_eq!(path, &[type_name::<Client>()]);
    assert!(source.downcast_ref::<io::Error>().is_some());
}

#[test]
fn factory_dependency_error_keeps_original_path() {
    let mut services = ServiceCollection::new();
    services.add_singleton_with(|r| {
        r.resolve::<Missing>()?;
        Ok(Client { port: 1 })
    });
    let provider = services.build().unwrap();

    let err = provider.resolve::<Client>().err().unwrap();

    assert!(matches!(&err, ResolveError::NotRegistered { path }
        if path == &[type_name::<Client>(), type_name::<Missing>()]));
}

#[test]
fn instance_is_returned_as_is() {
    let mut services = ServiceCollection::new();
    services.add_singleton_instance(Config { port: 8080 });
    let provider = services.build().unwrap();

    let first = provider.resolve::<Config>().unwrap();
    let second = provider.create_scope().resolve::<Config>().unwrap();

    assert_eq!(first.port, 8080);
    assert!(Arc::ptr_eq(&first, &second));
}
