// Snapshots match the compiler pinned in CI's `ui` job; diagnostics change
// between releases. Regenerate with:
// TRYBUILD=overwrite cargo +1.99.0 test -p hosting --test ui -- --ignored
#[test]
#[ignore = "pinned to one toolchain; run by CI's ui job"]
fn derive_diagnostics() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
