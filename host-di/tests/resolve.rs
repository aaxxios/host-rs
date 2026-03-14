#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use host_di::{Dependency, ResolveError, Resolver, Service, ServiceCollection};
use std::any::type_name;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

fn within_timeout<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(body());
    });
    rx.recv_timeout(Duration::from_secs(5))
        .expect("resolution did not finish")
}

macro_rules! leaf {
    ($name:ident, $counter:ident) => {
        static $counter: AtomicUsize = AtomicUsize::new(0);

        struct $name;

        impl Service for $name {
            fn dependencies() -> Vec<Dependency> {
                Vec::new()
            }

            fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
                $counter.fetch_add(1, SeqCst);
                Ok($name)
            }
        }
    };
}

leaf!(Database, DATABASE_CALLS);
leaf!(Config, CONFIG_CALLS);
leaf!(Clock, CLOCK_CALLS);
leaf!(Session, SESSION_CALLS);

struct Repository {
    db: Arc<Database>,
}

impl Service for Repository {
    fn dependencies() -> Vec<Dependency> {
        vec![Dependency::on::<Database>()]
    }

    fn construct(resolver: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Repository {
            db: resolver.resolve()?,
        })
    }
}

struct UserService {
    repository: Arc<Repository>,
}

impl Service for UserService {
    fn dependencies() -> Vec<Dependency> {
        vec![Dependency::on::<Repository>()]
    }

    fn construct(resolver: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(UserService {
            repository: resolver.resolve()?,
        })
    }
}

#[test]
fn singleton_is_shared() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<Config>();
    let provider = services.build().unwrap();

    let first = provider.resolve::<Config>().unwrap();
    let second = provider.resolve::<Config>().unwrap();

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(CONFIG_CALLS.load(SeqCst), 1);
}

#[test]
fn transient_is_new_each_time() {
    let mut services = ServiceCollection::new();
    services.add_transient::<Clock>();
    let provider = services.build().unwrap();

    let first = provider.resolve::<Clock>().unwrap();
    let second = provider.resolve::<Clock>().unwrap();

    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(CLOCK_CALLS.load(SeqCst), 2);
}

#[test]
fn dependencies_are_injected() {
    let mut services = ServiceCollection::new();
    services
        .add_transient::<UserService>()
        .add_singleton::<Repository>()
        .add_singleton::<Database>();
    let provider = services.build().unwrap();

    let users = provider.resolve::<UserService>().unwrap();
    let repository = provider.resolve::<Repository>().unwrap();
    let db = provider.resolve::<Database>().unwrap();

    assert!(Arc::ptr_eq(&users.repository, &repository));
    assert!(Arc::ptr_eq(&repository.db, &db));
}

#[test]
fn missing_registration_reports_path() {
    let mut services = ServiceCollection::new();
    services.add_transient::<UserService>();
    let provider = services.build().unwrap();

    let err = provider.resolve::<UserService>().err().unwrap();

    assert!(matches!(&err, ResolveError::NotRegistered { path }
        if path == &[type_name::<UserService>(), type_name::<Repository>()]));
    assert!(err.to_string().starts_with("service not registered: "));
}

#[test]
fn last_registration_wins() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<Session>()
        .add_transient::<Session>();
    let provider = services.build().unwrap();

    let first = provider.resolve::<Session>().unwrap();
    let second = provider.resolve::<Session>().unwrap();

    assert!(!Arc::ptr_eq(&first, &second));
}

static FLAKY_CALLS: AtomicUsize = AtomicUsize::new(0);

struct Flaky;

impl Service for Flaky {
    fn dependencies() -> Vec<Dependency> {
        Vec::new()
    }

    fn construct(resolver: &Resolver<'_>) -> Result<Self, ResolveError> {
        if FLAKY_CALLS.fetch_add(1, SeqCst) == 0 {
            return Err(resolver.construction_error(io::Error::other("not ready")));
        }
        Ok(Flaky)
    }
}

#[test]
fn failed_construction_is_not_cached() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<Flaky>();
    let provider = services.build().unwrap();

    let err = provider.resolve::<Flaky>().err().unwrap();
    assert!(matches!(&err, ResolveError::Construction { path, .. }
        if path == &[type_name::<Flaky>()]));
    assert!(provider.resolve::<Flaky>().is_ok());
    assert_eq!(FLAKY_CALLS.load(SeqCst), 2);
}

static PANICKY_CALLS: AtomicUsize = AtomicUsize::new(0);

struct Panicky;

impl Service for Panicky {
    fn dependencies() -> Vec<Dependency> {
        Vec::new()
    }

    fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
        if PANICKY_CALLS.fetch_add(1, SeqCst) == 0 {
            panic!("constructor panicked");
        }
        Ok(Panicky)
    }
}

#[test]
fn panicking_constructor_does_not_wedge_container() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<Panicky>();
    let provider = services.build().unwrap();

    let first = panic::catch_unwind(AssertUnwindSafe(|| provider.resolve::<Panicky>()));
    assert!(first.is_err());

    let second = within_timeout(move || provider.resolve::<Panicky>().is_ok());
    assert!(second);
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
fn singleton_constructed_once_under_contention() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<Slow>();
    let provider = services.build().unwrap();
    let barrier = Arc::new(Barrier::new(16));

    let handles: Vec<_> = (0..16)
        .map(|_| {
            let provider = provider.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                provider.resolve::<Slow>().unwrap()
            })
        })
        .collect();
    let instances: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    assert_eq!(SLOW_CALLS.load(SeqCst), 1);
    assert!(instances.iter().all(|i| Arc::ptr_eq(i, &instances[0])));
}

leaf!(Transaction, TRANSACTION_CALLS);

#[test]
fn scoped_from_root_is_rejected() {
    let mut services = ServiceCollection::new();
    services.add_scoped::<Transaction>();
    let provider = services.build().unwrap();

    let err = provider.resolve::<Transaction>().err().unwrap();

    assert!(matches!(&err, ResolveError::ScopedFromRoot { path }
        if path == &[type_name::<Transaction>()]));
    assert_eq!(TRANSACTION_CALLS.load(SeqCst), 0);
}
