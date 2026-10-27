# Contributing

Thanks for taking an interest. Bug reports, documentation fixes and code are
all welcome.

## Before you start

- Questions and design ideas belong in
  [Discussions](https://github.com/aaxxios/hosting-rs/discussions).
- Bugs and concrete feature requests belong in
  [Issues](https://github.com/aaxxios/hosting-rs/issues).
- Issues labelled `good first issue` are scoped to be done without knowing the
  whole codebase. Comment on one before starting so two people don't pick up
  the same thing.
- For anything larger, open an issue first and agree on the approach. A PR
  that changes the public API without that conversation will likely be asked
  to start there.

## Design principles

When trade-offs come up, they are settled in this order:

1. **Soundness.** No `unsafe`, no panics in library code, and errors that
   name the types and dependency path involved.
2. **Ease of use.** Registration should read like the .NET equivalent where
   Rust allows it. Where it doesn't, the difference is documented.
3. **Efficiency.** Resolution avoids allocation where it can; the benchmarks
   in `hosting-di/benches` and `hosting-di/tests/allocations.rs` guard this.

A few decisions are settled and not up for change:

- Constructors are synchronous. Async setup belongs in a hosted service.
- Dependency cycles are always rejected by `build()`; other graph checks are
  opt-in.
- The core stays runtime-neutral. Integrations such as tokio go behind
  features.
- New dependencies in the core crates need a strong reason.

## Workspace

| Crate | Contents |
|---|---|
| `hosting` | Facade that re-exports the others; holds the UI tests. |
| `hosting-di` | Service collection, provider, scopes and resolution. |
| `hosting-macros` | `#[derive(Service)]` and `#[abstraction]`. |
| `hosting-core` | `Host` and `HostBuilder`. |

## Checks

CI runs the following; run them before opening a PR:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo +1.85 check --workspace --all-features
```

Clippy denies `unwrap`, `expect` and `panic` outside tests. Return an error
instead.

Compile-fail tests for macro diagnostics are pinned to rustc 1.99.0, since
error wording changes between releases. If you change a macro or a trait
bound that shows up in an error, regenerate them:

```sh
rustup toolchain install 1.99.0
TRYBUILD=overwrite cargo +1.99.0 test -p hosting --test ui -- --ignored
```

and check the new `.stderr` files read well; they are what users see.

## Pull requests

- Keep each PR to one change. Unrelated fixes go in their own PR.
- Add a test that fails without your change.
- Public items need doc comments. Prefer a short example over a long
  explanation.
- Commit subjects are lowercase and imperative, under about 60 characters,
  without a trailing period (`reject scoped services resolved from the root`).
  Explain *why* in the body when the diff doesn't make it obvious.

## License

By contributing you agree that your work is released under the
[MIT license](LICENSE).
