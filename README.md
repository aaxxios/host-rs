# host

Dependency injection and application hosting for Rust, modeled on
`Microsoft.Extensions.DependencyInjection` and `Microsoft.Extensions.Hosting`
from .NET.

Services are plain structs. You describe their dependencies once, register
them with a lifetime, and the container builds them on request.

```rust
use host::{Service, ServiceCollection};
use std::sync::Arc;

#[derive(Service)]
struct Database;

#[derive(Service)]
struct Repository {
    db: Arc<Database>,
}

let mut services = ServiceCollection::new();
services
    .add_singleton::<Database>()
    .add_scoped::<Repository>();
let provider = services.build()?;

let scope = provider.create_scope();
let repository = scope.resolve::<Repository>()?;
```

## Status

Early development. The dependency injection container is usable; the host
lifecycle and configuration are not written yet. The API will change and the
crates are not published.

| Area | State |
|---|---|
| Container: lifetimes, scopes, factories, instances | done |
| `#[derive(Service)]` and named constructors | done |
| Trait registration and `resolve_all` | done |
| Build-time validation | done |
| Hosted services, graceful shutdown, signals | planned |
| Configuration sources and typed options | planned |

## Lifetimes

| Method | One instance per |
|---|---|
| `add_singleton` | provider |
| `add_scoped` | scope |
| `add_transient` | resolution |

The lifetime is chosen when registering, not on the type, so the same struct
can be a singleton in one application and scoped in another.

A singleton is always built outside any scope. If it depends on a scoped
service, resolving it fails with `ResolveError::ScopedFromRoot` instead of
keeping one scope's instance for the life of the process. Transients are
built in the scope that asked for them.

## Declaring services

By default every field is a dependency and must be `Arc<T>` or `Vec<Arc<T>>`.
When a struct needs setup, name a constructor instead; its parameters are
injected and it may return `Self` or `Result<Self, E>`:

```rust
#[derive(Service)]
#[service(constructor = connect)]
struct Database {
    url: String,
}

impl Database {
    fn connect(config: Arc<Config>) -> Result<Self, std::io::Error> {
        Ok(Database { url: config.url.clone() })
    }
}
```

Types from other crates can be registered with a factory or as an existing
value:

```rust
services
    .add_singleton_with(|r| Ok(Client::new(r.resolve::<Config>()?.timeout)))
    .add_singleton_instance(Config::default());
```

## Traits

Mark a trait with `#[abstraction]` once, then register implementations of
it. A single resolve returns the last registration; `resolve_all` and
`Vec<Arc<dyn Trait>>` return all of them in registration order.

```rust
use host::abstraction;

#[abstraction]
trait Notifier: Send + Sync {
    fn notify(&self, message: &str);
}

services
    .add_singleton_as::<dyn Notifier, Email>()
    .add_singleton_as::<dyn Notifier, Sms>();

let last = provider.resolve::<dyn Notifier>()?;     // Sms
let all = provider.resolve_all::<dyn Notifier>()?; // [Email, Sms]
```

Traits that can't carry the attribute, such as ones from other crates, can be
registered through a factory with `add_singleton_as_with`.

Registering a type again adds a registration rather than replacing one. Use
`try_add_singleton` and its variants to register a default only when nothing
is registered yet, as with `TryAdd` in .NET.

## Validation

`build()` always rejects dependency cycles. Two more checks are opt-in:

```rust
services
    .validate_dependencies() // every declared dependency is registered
    .validate_scopes();      // no singleton depends on a scoped service

let err = services.build().unwrap_err();
for issue in err.issues() {
    eprintln!("{issue}");
}
// dependency cycle: app::Ping -> app::Pong -> app::Ping
// dependency not registered: app::Database -> app::Config
// singleton depends on scoped service: app::Cache -> app::Handler -> app::Transaction
```

Every problem is reported at once. Unlike .NET, these checks are not enabled
automatically in development; turn them on where you want them.

## Errors

Resolution errors name the types involved and the path that led to the
failure, for example
`service not registered: app::UserService -> app::Repository`. A constructor
that fails is not cached, so the next resolution tries again.

## Crates

| Crate | Contents |
|---|---|
| `host` | The crate to depend on. Re-exports everything below. |
| `host-di` | Service collection, provider, scopes and resolution. |
| `host-macros` | `#[derive(Service)]` and `#[abstraction]`. |
| `host-core` | `Host` and `HostBuilder`. |

## Development

The minimum supported Rust version is 1.85.

```sh
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo bench -p host-di --bench resolve
```

Compile-fail tests for macro diagnostics are pinned to one compiler, since
error wording changes between releases:

```sh
cargo +1.99.0 test -p host --test ui -- --ignored
```

## License

MIT. See [LICENSE](LICENSE).
