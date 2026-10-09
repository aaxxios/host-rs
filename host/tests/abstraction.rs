#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use host::{BuildIssue, ResolveError, Service, ServiceCollection, abstraction};
use std::sync::Arc;

#[abstraction]
trait Sender: Send + Sync {
    fn name(&self) -> &'static str;
}

#[abstraction]
trait Repo<K>: Send + Sync {
    fn key(&self) -> &'static str;
}

#[derive(Service)]
struct Smtp;

#[derive(Service)]
struct Ses;

impl Sender for Smtp {
    fn name(&self) -> &'static str {
        "smtp"
    }
}

impl Sender for Ses {
    fn name(&self) -> &'static str {
        "ses"
    }
}

#[derive(Service)]
struct Mailer {
    primary: Arc<dyn Sender>,
    all: Vec<Arc<dyn Sender>>,
}

fn names(senders: &[Arc<dyn Sender>]) -> Vec<&'static str> {
    senders.iter().map(|sender| sender.name()).collect()
}

fn addr<T: ?Sized>(arc: &Arc<T>) -> *const () {
    Arc::as_ptr(arc) as *const ()
}

#[test]
fn derived_service_receives_last_and_all() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_as::<dyn Sender, Smtp>()
        .add_singleton_as::<dyn Sender, Ses>()
        .add_transient::<Mailer>();
    let mailer = services.build().unwrap().resolve::<Mailer>().unwrap();

    assert_eq!(mailer.primary.name(), "ses");
    assert_eq!(names(&mailer.all), ["smtp", "ses"]);
}

#[test]
fn mixed_lifetimes_under_one_trait() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_as::<dyn Sender, Smtp>()
        .add_transient_as::<dyn Sender, Ses>();
    let provider = services.build().unwrap();

    let first = provider.resolve_all::<dyn Sender>().unwrap();
    let second = provider.resolve_all::<dyn Sender>().unwrap();

    assert_eq!(addr(&first[0]), addr(&second[0]));
    assert_ne!(addr(&first[1]), addr(&second[1]));
}

#[test]
fn concrete_and_trait_registrations_are_independent() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<Smtp>()
        .add_singleton_as::<dyn Sender, Smtp>();
    let provider = services.build().unwrap();

    let concrete = provider.resolve::<Smtp>().unwrap();
    let as_trait = provider.resolve::<dyn Sender>().unwrap();

    assert_ne!(addr(&concrete), addr(&as_trait));
}

#[test]
fn same_implementation_twice_gives_two_instances() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_as::<dyn Sender, Smtp>()
        .add_singleton_as::<dyn Sender, Smtp>();
    let provider = services.build().unwrap();

    let all = provider.resolve_all::<dyn Sender>().unwrap();

    assert_eq!(all.len(), 2);
    assert_ne!(addr(&all[0]), addr(&all[1]));
}

#[derive(Service)]
struct MemoryRepo;

impl Repo<u8> for MemoryRepo {
    fn key(&self) -> &'static str {
        "u8"
    }
}

#[test]
fn generic_trait_instantiations_are_distinct() {
    let mut services = ServiceCollection::new();
    services.add_singleton_as::<dyn Repo<u8>, MemoryRepo>();
    let provider = services.build().unwrap();

    assert_eq!(provider.resolve::<dyn Repo<u8>>().unwrap().key(), "u8");
    assert!(matches!(
        provider.resolve::<dyn Repo<u16>>(),
        Err(ResolveError::NotRegistered { .. })
    ));
}

#[derive(Service)]
struct Composite {
    all: Vec<Arc<dyn Sender>>,
}

impl Sender for Composite {
    fn name(&self) -> &'static str {
        self.all.first().map_or("none", |sender| sender.name())
    }
}

#[test]
fn composite_over_its_own_trait_is_a_build_cycle() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_as::<dyn Sender, Smtp>()
        .add_singleton_as::<dyn Sender, Composite>();

    let err = services.build().err().unwrap();

    assert!(matches!(err.issues(), [BuildIssue::Cycle { .. }]));
}

#[derive(Service)]
struct Logging {
    inner: Arc<Smtp>,
}

impl Sender for Logging {
    fn name(&self) -> &'static str {
        self.inner.name()
    }
}

#[test]
fn decorator_over_a_concrete_is_not_a_cycle() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<Smtp>()
        .add_singleton_as::<dyn Sender, Logging>();
    let provider = services.build().unwrap();

    assert_eq!(provider.resolve::<dyn Sender>().unwrap().name(), "smtp");
}

// Deliberately not an `#[abstraction]`, like a trait from another crate.
trait Plain: Send + Sync {
    fn id(&self) -> u32;
}

struct Fixed(u32);

impl Plain for Fixed {
    fn id(&self) -> u32 {
        self.0
    }
}

#[test]
fn unmarked_trait_via_factory() {
    let mut services = ServiceCollection::new();
    services.add_singleton_as_with::<dyn Plain, _>(|_| Ok(Arc::new(Fixed(7))));
    let provider = services.build().unwrap();

    let first = provider.resolve::<dyn Plain>().unwrap();
    let second = provider.resolve::<dyn Plain>().unwrap();

    assert_eq!(first.id(), 7);
    assert_eq!(addr(&first), addr(&second));
}

#[test]
fn unmarked_trait_via_instance() {
    let mut services = ServiceCollection::new();
    services.add_singleton_instance_as::<dyn Plain>(Arc::new(Fixed(3)));

    let plain = services.build().unwrap().resolve::<dyn Plain>().unwrap();

    assert_eq!(plain.id(), 3);
}

#[test]
fn transient_as_factory_runs_per_resolve() {
    let mut services = ServiceCollection::new();
    services.add_transient_as_with::<dyn Plain, _>(|_| Ok(Arc::new(Fixed(1))));
    let provider = services.build().unwrap();

    let first = provider.resolve::<dyn Plain>().unwrap();
    let second = provider.resolve::<dyn Plain>().unwrap();

    assert_ne!(addr(&first), addr(&second));
}

#[test]
fn scoped_as_factory_runs_once_per_scope() {
    let mut services = ServiceCollection::new();
    services.add_scoped_as_with::<dyn Plain, _>(|_| Ok(Arc::new(Fixed(1))));
    let provider = services.build().unwrap();
    let scope = provider.create_scope();

    let first = scope.resolve::<dyn Plain>().unwrap();
    let second = scope.resolve::<dyn Plain>().unwrap();
    let other = provider.create_scope().resolve::<dyn Plain>().unwrap();

    assert_eq!(addr(&first), addr(&second));
    assert_ne!(addr(&first), addr(&other));
}

#[test]
fn factories_and_derived_mix_in_resolve_all() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton_as::<dyn Sender, Smtp>()
        .add_singleton_as_with::<dyn Sender, _>(|_| Ok(Arc::new(Ses)));
    let provider = services.build().unwrap();

    assert_eq!(
        names(&provider.resolve_all::<dyn Sender>().unwrap()),
        ["smtp", "ses"]
    );
}

#[abstraction]
trait Spelled
where
    Self: Send + Sync,
{
    fn word(&self) -> &'static str;
}

impl Spelled for Smtp {
    fn word(&self) -> &'static str {
        "where"
    }
}

#[test]
fn send_sync_may_be_written_as_where_self() {
    let mut services = ServiceCollection::new();
    services.add_singleton_as::<dyn Spelled, Smtp>();

    let spelled = services.build().unwrap().resolve::<dyn Spelled>().unwrap();

    assert_eq!(spelled.word(), "where");
}
