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
