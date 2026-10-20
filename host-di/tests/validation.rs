#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use host_di::{BuildIssue, Dependency, ResolveError, Resolver, Service, ServiceCollection};
use std::any::type_name;

macro_rules! service {
    ($name:ident => $($dependency:expr),*) => {
        struct $name;

        impl Service for $name {
            fn dependencies() -> Vec<Dependency> {
                vec![$($dependency),*]
            }

            fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
                Ok($name)
            }
        }
    };
}

service!(Leaf =>);
service!(NeedsLeaf => Dependency::on::<Leaf>());
service!(NeedsMissing => Dependency::on::<Missing>());
service!(AlsoNeedsMissing => Dependency::on::<Missing>());
service!(NeedsAllMissing => Dependency::all::<Missing>());

struct Missing;

fn name<T>() -> &'static str {
    type_name::<T>()
}

#[test]
fn missing_dependency_is_not_checked_by_default() {
    let mut services = ServiceCollection::new();
    services.add_singleton::<NeedsMissing>();

    assert!(services.build().is_ok());
}

#[test]
fn missing_dependency_is_reported_when_enabled() {
    let mut services = ServiceCollection::new();
    services
        .validate_dependencies()
        .add_singleton::<Leaf>()
        .add_singleton::<NeedsLeaf>()
        .add_singleton::<NeedsMissing>();

    let err = services.build().err().unwrap();

    assert!(
        matches!(err.issues(), [BuildIssue::MissingDependency { path }]
        if path == &[name::<NeedsMissing>(), name::<Missing>()])
    );
    assert_eq!(
        err.to_string(),
        format!(
            "dependency not registered: {} -> {}",
            name::<NeedsMissing>(),
            name::<Missing>()
        )
    );
}

#[test]
fn every_missing_dependency_is_reported() {
    let mut services = ServiceCollection::new();
    services
        .validate_dependencies()
        .add_singleton::<NeedsMissing>()
        .add_transient::<AlsoNeedsMissing>();

    assert_eq!(services.build().err().unwrap().issues().len(), 2);
}

#[test]
fn empty_vec_dependency_is_not_missing() {
    let mut services = ServiceCollection::new();
    services
        .validate_dependencies()
        .add_singleton::<NeedsAllMissing>();

    assert!(services.build().is_ok());
}

#[test]
fn factories_are_not_checked_for_dependencies() {
    let mut services = ServiceCollection::new();
    services.validate_dependencies().add_singleton_with(|r| {
        r.resolve::<Missing>()?;
        Ok(Leaf)
    });

    assert!(services.build().is_ok());
}

service!(Tx =>);
service!(Cache => Dependency::on::<Tx>());
service!(Handler => Dependency::on::<Tx>());
service!(CacheViaHandler => Dependency::on::<Handler>());
service!(Inner => Dependency::on::<Tx>());
service!(Outer => Dependency::on::<Inner>());
service!(Request => Dependency::on::<Tx>(), Dependency::on::<Handler>());
service!(Ping => Dependency::on::<Pong>());
service!(Pong => Dependency::on::<Ping>());

#[test]
fn captive_dependency_is_not_checked_by_default() {
    let mut services = ServiceCollection::new();
    services.add_scoped::<Tx>().add_singleton::<Cache>();

    assert!(services.build().is_ok());
}

#[test]
fn singleton_on_scoped_is_reported() {
    let mut services = ServiceCollection::new();
    services
        .validate_scopes()
        .add_scoped::<Tx>()
        .add_singleton::<Cache>();

    let err = services.build().err().unwrap();

    assert!(
        matches!(err.issues(), [BuildIssue::CaptiveDependency { path }]
        if path == &[name::<Cache>(), name::<Tx>()])
    );
    assert_eq!(
        err.to_string(),
        format!(
            "singleton depends on scoped service: {} -> {}",
            name::<Cache>(),
            name::<Tx>()
        )
    );
}

#[test]
fn singleton_through_transient_on_scoped_is_reported() {
    let mut services = ServiceCollection::new();
    services
        .validate_scopes()
        .add_scoped::<Tx>()
        .add_transient::<Handler>()
        .add_singleton::<CacheViaHandler>();

    let err = services.build().err().unwrap();

    assert!(
        matches!(err.issues(), [BuildIssue::CaptiveDependency { path }]
        if path == &[name::<CacheViaHandler>(), name::<Handler>(), name::<Tx>()])
    );
}

#[test]
fn outer_singleton_is_not_reported_twice() {
    let mut services = ServiceCollection::new();
    services
        .validate_scopes()
        .add_scoped::<Tx>()
        .add_singleton::<Inner>()
        .add_singleton::<Outer>();

    let err = services.build().err().unwrap();

    assert!(
        matches!(err.issues(), [BuildIssue::CaptiveDependency { path }]
        if path == &[name::<Inner>(), name::<Tx>()])
    );
}

#[test]
fn scoped_and_transient_may_depend_on_scoped() {
    let mut services = ServiceCollection::new();
    services
        .validate_scopes()
        .add_scoped::<Tx>()
        .add_transient::<Handler>()
        .add_scoped::<Request>();

    assert!(services.build().is_ok());
}

#[test]
fn factories_are_not_checked_for_scopes() {
    let mut services = ServiceCollection::new();
    services
        .validate_scopes()
        .add_scoped::<Tx>()
        .add_singleton_with(|r| {
            r.resolve::<Tx>()?;
            Ok(Leaf)
        });

    assert!(services.build().is_ok());
}

#[test]
fn every_kind_of_issue_is_reported_together() {
    let mut services = ServiceCollection::new();
    services
        .validate_dependencies()
        .validate_scopes()
        .add_singleton::<NeedsMissing>()
        .add_scoped::<Tx>()
        .add_singleton::<Cache>()
        .add_singleton::<Ping>()
        .add_singleton::<Pong>();

    let err = services.build().err().unwrap();

    assert!(matches!(
        err.issues(),
        [
            BuildIssue::Cycle { .. },
            BuildIssue::MissingDependency { .. },
            BuildIssue::CaptiveDependency { .. },
        ]
    ));
}
