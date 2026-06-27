#[test]
fn derive_diagnostics() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
