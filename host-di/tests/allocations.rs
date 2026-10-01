#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unsafe_code)]

use host_di::{CastFrom, Dependency, ResolveError, Resolver, Service, ServiceCollection};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every method forwards to `System` with the caller's arguments unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, SeqCst);
        // SAFETY: forwarded from the caller, who upholds `GlobalAlloc::alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, SeqCst);
        // SAFETY: as for `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, SeqCst);
        // SAFETY: `ptr` and `layout` come from this allocator, which is `System`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` and `layout` come from this allocator, which is `System`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations_during(f: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.load(SeqCst);
    f();
    ALLOCATIONS.load(SeqCst) - before
}

struct Database;

impl Service for Database {
    fn dependencies() -> Vec<Dependency> {
        Vec::new()
    }

    fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Database)
    }
}

trait Store: Send + Sync {}

impl Store for Database {}

impl<T: Store + 'static> CastFrom<T> for dyn Store {
    fn cast(from: Arc<T>) -> Arc<Self> {
        from
    }
}

struct Transaction;

impl Service for Transaction {
    fn dependencies() -> Vec<Dependency> {
        Vec::new()
    }

    fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Transaction)
    }
}

macro_rules! scoped {
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
    )*};
}

scoped!(S1, S2, S3, S4, S5);

// One test per binary: the counter is process-wide.
#[test]
fn resolving_existing_instances_does_not_allocate() {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<Database>()
        .add_scoped::<Transaction>();
    let provider = services.build().unwrap();
    provider.resolve::<Database>().unwrap();

    let singleton = allocations_during(|| {
        for _ in 0..1000 {
            provider.resolve::<Database>().unwrap();
        }
    });
    assert_eq!(singleton, 0, "resolving a built singleton");

    let mut scope = None;
    let create = allocations_during(|| scope = Some(provider.create_scope()));
    assert_eq!(create, 0, "creating a scope");
    let scope = scope.unwrap();

    let singleton_in_scope = allocations_during(|| {
        for _ in 0..1000 {
            scope.resolve::<Database>().unwrap();
        }
    });
    assert_eq!(singleton_in_scope, 0, "resolving a singleton from a scope");

    scope.resolve::<Transaction>().unwrap();
    let scoped = allocations_during(|| {
        for _ in 0..1000 {
            scope.resolve::<Transaction>().unwrap();
        }
    });
    assert_eq!(scoped, 0, "resolving a built scoped instance");

    let mut services = ServiceCollection::new();
    services.add_singleton_as::<dyn Store, Database>();
    let provider = services.build().unwrap();
    provider.resolve::<dyn Store>().unwrap();
    let as_trait = allocations_during(|| {
        for _ in 0..1000 {
            provider.resolve::<dyn Store>().unwrap();
        }
    });
    assert_eq!(as_trait, 0, "resolving a built singleton as a trait");

    // Beyond one `Arc` per instance, a scope's storage is a single allocation
    // made on its first scoped resolution, however many instances it holds.
    let mut services = ServiceCollection::new();
    services
        .add_scoped::<S1>()
        .add_scoped::<S2>()
        .add_scoped::<S3>()
        .add_scoped::<S4>()
        .add_scoped::<S5>();
    let provider = services.build().unwrap();
    let one = allocations_during(|| {
        let scope = provider.create_scope();
        scope.resolve::<S1>().unwrap();
    });
    let five = allocations_during(|| {
        let scope = provider.create_scope();
        scope.resolve::<S1>().unwrap();
        scope.resolve::<S2>().unwrap();
        scope.resolve::<S3>().unwrap();
        scope.resolve::<S4>().unwrap();
        scope.resolve::<S5>().unwrap();
    });
    assert_eq!(five - one, 4, "scope bookkeeping per instance");
}
