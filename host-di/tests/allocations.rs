#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unsafe_code)]

use host_di::{Dependency, ResolveError, Resolver, Service, ServiceCollection};
use std::alloc::{GlobalAlloc, Layout, System};
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

struct Transaction;

impl Service for Transaction {
    fn dependencies() -> Vec<Dependency> {
        Vec::new()
    }

    fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Transaction)
    }
}

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
}
