//! UB tests against the safe public API.
#![forbid(unsafe_code)]

mod log;
mod mem;

/// Programs under `compile_fail/` must be rejected by the compiler.
#[test]
fn compile_fail() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ub_safe/compile_fail");
    let any = std::fs::read_dir(&dir)
        .map(|d| d.flatten().any(|e| e.path().extension().is_some_and(|x| x == "rs")))
        .unwrap_or(false);
    if any {
        trybuild::TestCases::new().compile_fail(dir.join("*.rs"));
    }
}
