#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use host_di::{Dependency, ResolveError, Resolver, Service, ServiceCollection, ServiceProvider};
use std::any::type_name;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

struct Transaction;

impl Service for Transaction {
    fn dependencies() -> Vec<Dependency> {
        Vec::new()
    }

    fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Transaction)
    }
}

struct Clock;

impl Service for Clock {
    fn dependencies() -> Vec<Dependency> {
        Vec::new()
    }

    fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Clock)
    }
}

struct Handler {
    tx: Arc<Transaction>,
}

impl Service for Handler {
    fn dependencies() -> Vec<Dependency> {
        vec![Dependency::on::<Transaction>()]
    }

    fn construct(resolver: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Handler {
            tx: resolver.resolve()?,
        })
    }
}

struct Cache {
    _tx: Arc<Transaction>,
}

impl Service for Cache {
    fn dependencies() -> Vec<Dependency> {
        vec![Dependency::on::<Transaction>()]
    }

    fn construct(resolver: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Cache {
            _tx: resolver.resolve()?,
        })
    }
}

fn provider() -> ServiceProvider {
    let mut services = ServiceCollection::new();
    services
        .add_scoped::<Transaction>()
        .add_singleton::<Clock>()
        .add_transient::<Handler>()
        .add_singleton::<Cache>();
    services.build().unwrap()
}

#[test]
fn scoped_is_shared_within_scope() {
    let scope = provider().create_scope();

    let first = scope.resolve::<Transaction>().unwrap();
    let second = scope.resolve::<Transaction>().unwrap();

    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn scoped_differs_across_scopes() {
    let provider = provider();

    let first = provider.create_scope().resolve::<Transaction>().unwrap();
    let second = provider.create_scope().resolve::<Transaction>().unwrap();

    assert!(!Arc::ptr_eq(&first, &second));
}

#[test]
fn singleton_is_shared_across_scopes() {
    let provider = provider();

    let first = provider.create_scope().resolve::<Clock>().unwrap();
    let second = provider.create_scope().resolve::<Clock>().unwrap();

    assert!(Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(&first, &provider.resolve::<Clock>().unwrap()));
}

#[test]
fn transient_in_scope_can_depend_on_scoped() {
    let scope = provider().create_scope();

    let handler = scope.resolve::<Handler>().unwrap();

    assert!(Arc::ptr_eq(
        &handler.tx,
        &scope.resolve::<Transaction>().unwrap()
    ));
}

#[test]
fn singleton_depending_on_scoped_fails_from_scope() {
    let scope = provider().create_scope();

    let err = scope.resolve::<Cache>().err().unwrap();

    assert!(matches!(&err, ResolveError::ScopedFromRoot { path }
        if path == &[type_name::<Cache>(), type_name::<Transaction>()]));
}

static SLOW_CALLS: AtomicUsize = AtomicUsize::new(0);

struct Slow;

impl Service for Slow {
    fn dependencies() -> Vec<Dependency> {
        Vec::new()
    }

    fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
        SLOW_CALLS.fetch_add(1, SeqCst);
        thread::sleep(Duration::from_millis(20));
        Ok(Slow)
    }
}

#[test]
fn scoped_constructed_once_under_contention() {
    let mut services = ServiceCollection::new();
    services.add_scoped::<Slow>();
    let scope = Arc::new(services.build().unwrap().create_scope());
    let barrier = Arc::new(Barrier::new(16));

    let handles: Vec<_> = (0..16)
        .map(|_| {
            let scope = Arc::clone(&scope);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                scope.resolve::<Slow>().unwrap()
            })
        })
        .collect();
    let instances: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    assert_eq!(SLOW_CALLS.load(SeqCst), 1);
    assert!(instances.iter().all(|i| Arc::ptr_eq(i, &instances[0])));
}

#[test]
fn scope_outlives_provider() {
    let provider = provider();
    let scope = provider.create_scope();
    drop(provider);

    let resolved = thread::spawn(move || {
        scope.resolve::<Clock>().is_ok() && scope.resolve::<Transaction>().is_ok()
    })
    .join()
    .unwrap();

    assert!(resolved);
}
