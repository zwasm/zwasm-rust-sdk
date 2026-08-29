//! `runtime_version()` reports the vendored zwasm, not this crate.
//!
//! The expected value is read from the submodule's manifest rather than written
//! here. A literal would have to be updated on every bump, and the day it is
//! forgotten the test still passes against a stale build — which is the failure
//! this file exists to catch.

/// `.version = "2.5.0",` in `build.zig.zon`, without depending on a TOML/ZON
/// parser for one field.
fn manifest_version() -> String {
    let manifest = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/zwasm-sys/zwasm/build.zig.zon"
    );
    let text =
        std::fs::read_to_string(manifest).unwrap_or_else(|e| panic!("cannot read {manifest}: {e}"));

    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(".version") else {
            continue;
        };
        let Some(open) = rest.find('"') else { continue };
        let rest = &rest[open + 1..];
        let Some(close) = rest.find('"') else {
            continue;
        };
        return rest[..close].to_string();
    }
    panic!("no .version field found in {manifest}");
}

// The accessor is build_options-backed upstream, so this pins two things at
// once: that zwasm still reports what its own manifest says, and that the
// library under test was rebuilt for the submodule currently checked out. A
// stale libzwasm.a shows up here as a version from the previous pin.
#[test]
fn the_runtime_version_is_the_vendored_one() {
    assert_eq!(zwasm_sdk::runtime_version(), manifest_version());
}

// Two versions travel in this repo and they are not the same number.
#[test]
fn the_runtime_version_is_not_the_crate_version() {
    let runtime = zwasm_sdk::runtime_version();
    assert!(
        !runtime.is_empty(),
        "the runtime version should not be empty"
    );
    assert_ne!(
        runtime,
        env!("CARGO_PKG_VERSION"),
        "runtime_version() appears to be reporting the crate's version"
    );
}
