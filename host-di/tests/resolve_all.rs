#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use host_di::__private::Inject;
use host_di::{BuildIssue, Dependency, ResolveError, Resolver, Service, ServiceCollection};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

struct Tag(&'static str);

fn names(tags: &[Arc<Tag>]) -> Vec<&'static str> {
    tags.iter().map(|tag| tag.0).collect()
}

#[test]
fn resolve_all_returns_every_registration_in_order() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_with(|_| Ok(Tag("a")))
        .add_singleton_with(|_| Ok(Tag("b")));
    let provider = services.build().unwrap();

    assert_eq!(names(&provider.resolve_all::<Tag>().unwrap()), ["a", "b"]);
}

#[test]
fn resolve_all_respects_each_lifetime() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_with(|_| Ok(Tag("a")))
        .add_transient_with(|_| Ok(Tag("b")));
    let provider = services.build().unwrap();

    let first = provider.resolve_all::<Tag>().unwrap();
    let second = provider.resolve_all::<Tag>().unwrap();

    assert!(Arc::ptr_eq(&first[0], &second[0]));
    assert!(!Arc::ptr_eq(&first[1], &second[1]));
}

#[test]
fn resolve_all_with_no_registrations_is_empty() {
    let provider = ServiceCollection::new().build().unwrap();

    assert!(provider.resolve_all::<Tag>().unwrap().is_empty());
}

#[test]
fn single_resolve_still_gets_the_last_registration() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_with(|_| Ok(Tag("a")))
        .add_singleton_with(|_| Ok(Tag("b")));
    let provider = services.build().unwrap();

    assert_eq!(provider.resolve::<Tag>().unwrap().0, "b");
}

#[test]
fn resolve_all_from_root_rejects_scoped_members() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_with(|_| Ok(Tag("a")))
        .add_scoped_with(|_| Ok(Tag("b")));
    let provider = services.build().unwrap();

    assert!(matches!(
        provider.resolve_all::<Tag>(),
        Err(ResolveError::ScopedFromRoot { .. })
    ));
    assert_eq!(
        provider.create_scope().resolve_all::<Tag>().unwrap().len(),
        2
    );
}

#[test]
fn failing_member_fails_the_call_and_is_retried() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let mut services = ServiceCollection::new();
    services
        .add_singleton_with(|_| Ok(Tag("a")))
        .add_singleton_with(move |_| {
            if counter.fetch_add(1, SeqCst) == 0 {
                return Err(io::Error::other("not ready").into());
            }
            Ok(Tag("b"))
        });
    let provider = services.build().unwrap();

    assert!(matches!(
        provider.resolve_all::<Tag>(),
        Err(ResolveError::Construction { .. })
    ));
    assert_eq!(names(&provider.resolve_all::<Tag>().unwrap()), ["a", "b"]);
    assert_eq!(calls.load(SeqCst), 2);
}

struct Mailer {
    all: Vec<Arc<Tag>>,
}

impl Service for Mailer {
    fn dependencies() -> Vec<Dependency> {
        vec![<Vec<Arc<Tag>> as Inject>::dependency()]
    }

    fn construct(resolver: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Mailer {
            all: Inject::inject(resolver)?,
        })
    }
}

#[test]
fn vec_dependency_is_injected() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_with(|_| Ok(Tag("a")))
        .add_singleton_with(|_| Ok(Tag("b")))
        .add_transient::<Mailer>();
    let provider = services.build().unwrap();

    assert_eq!(
        names(&provider.resolve::<Mailer>().unwrap().all),
        ["a", "b"]
    );
}

struct A;
struct B;

impl Service for A {
    fn dependencies() -> Vec<Dependency> {
        vec![Dependency::all::<B>()]
    }

    fn construct(resolver: &Resolver<'_>) -> Result<Self, ResolveError> {
        resolver.resolve_all::<B>()?;
        Ok(A)
    }
}

impl Service for B {
    fn dependencies() -> Vec<Dependency> {
        vec![Dependency::on::<A>()]
    }

    fn construct(resolver: &Resolver<'_>) -> Result<Self, ResolveError> {
        resolver.resolve::<A>()?;
        Ok(B)
    }
}

#[test]
fn vec_dependency_cycle_is_rejected_at_build() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<A>().add_singleton::<B>();

    let err = services.build().err().unwrap();

    assert!(matches!(err.issues(), [BuildIssue::Cycle { .. }]));
}
