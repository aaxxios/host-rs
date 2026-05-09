#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use criterion::{Criterion, criterion_group, criterion_main};
use host_di::{Dependency, ResolveError, Resolver, Service, ServiceCollection, ServiceProvider};
use std::hint::black_box;
use std::sync::Arc;

struct Database;

impl Service for Database {
    fn dependencies() -> Vec<Dependency> {
        Vec::new()
    }

    fn construct(_: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Database)
    }
}

struct Repository {
    _db: Arc<Database>,
}

impl Service for Repository {
    fn dependencies() -> Vec<Dependency> {
        vec![Dependency::on::<Database>()]
    }

    fn construct(resolver: &Resolver<'_>) -> Result<Self, ResolveError> {
        Ok(Repository {
            _db: resolver.resolve()?,
        })
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

fn provider() -> ServiceProvider {
    let mut services = ServiceCollection::new();
    services
        .add_singleton::<Database>()
        .add_transient::<Repository>()
        .add_scoped::<Transaction>();
    services.build().unwrap()
}

fn resolve_singleton(c: &mut Criterion) {
    let provider = provider();
    provider.resolve::<Database>().unwrap();
    c.bench_function("resolve_singleton", |b| {
        b.iter(|| black_box(provider.resolve::<Database>().unwrap()));
    });
}

fn resolve_transient(c: &mut Criterion) {
    let provider = provider();
    c.bench_function("resolve_transient", |b| {
        b.iter(|| black_box(provider.resolve::<Repository>().unwrap()));
    });
}

fn scope_resolve_scoped(c: &mut Criterion) {
    let provider = provider();
    c.bench_function("scope_resolve_scoped", |b| {
        b.iter(|| {
            let scope = provider.create_scope();
            black_box(scope.resolve::<Transaction>().unwrap())
        });
    });
}

criterion_group!(
    benches,
    resolve_singleton,
    resolve_transient,
    scope_resolve_scoped
);
criterion_main!(benches);
