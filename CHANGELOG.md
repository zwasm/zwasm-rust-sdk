# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2026-08-31
### Changed
- **Breaking.** Rewrote the SDK against zwasm 2.x. zwasm 2.0 replaced its custom C API with the standard [wasm-c-api](https://github.com/WebAssembly/wasm-c-api), so nothing from 0.1 carries over. `Module::new` no longer takes bytes alone, `Module::invoke` is gone, and `Config`, `Imports` and `CancelHandle` have no replacement yet
- **Breaking.** The store owns everything. `Func`, `Global`, `Instance`, `Memory`, `Module` and `Table` are `Copy` handles with no destructor of their own; the methods on them borrow the store, and using one with a store it does not belong to panics rather than reaching freed memory. This follows wasmtime's model, and it is what makes the handles safe to hold: in the wasm-c-api every object is freed through the store, so nothing can outlive it
- Updated the bundled zwasm C API to 2.6.0
- The C library is now linked statically. Consumers no longer need `libzwasm.so` on the machine that runs the binary
- Building needs no network. zwasm's root package declares no dependencies, so `zig build static-lib` fetches nothing even on a cold Zig cache
- Bindings are now generated from `wasm.h`, `zwasm.h` and `wasi.h` rather than a single header
- `Error::Trap` now carries a machine-readable `TrapKind` beside the message, so a host can tell a guest trap from a cancellation or a fuel exhaustion without matching on the message text

### Added
- `Engine`, `Store`, `Module`, `Instance` and `Func`, mirroring the wasm-c-api object model
- `Val`, a typed enum over `wasm_val_t`, replacing the `u64` arrays of 0.1
- `Memory`, `Global` and `Table`
- `Instance::get_func` for looking up an exported function by name
- `Func::new_host` and an `imports` argument on `Instance::new`, so a guest can call into Rust. Such a function can also be called directly, which runs its callback with no instance in between
- `WasiConfig` covering the whole `wasi.h` surface (args, envs, preopens, stdio and env inheritance), installed with `Store::set_wasi` and removed with `Store::unset_wasi`
- `Error::WasiExit { code }`, the status a WASI guest passed to `proc_exit`. A WASI command reaches `proc_exit` even when it succeeds, so this is the ordinary end of a successful run and `code` is what says which it was — wasmtime reports the same event as `I32Exit`
- `runtime_version()`, the semver of the linked zwasm. Not this crate's version, and not a build identity: zwasm's compile-time options do not appear in it, so nothing should branch on it to decide whether a feature is present
- `Debug` on every public type, per the Rust API guidelines
- Every public type is re-exported from the crate root, so `use zwasm_sdk::{Engine, Store, Module}` replaces one `use` line per module. The modules stay public and the longer paths keep working
- Cross-compilation. `cargo zigbuild --target <triple>` builds the C library for the target rather than the host; `aarch64-unknown-linux-musl` and `x86_64-unknown-linux-musl` are covered in CI

### Fixed
- The C library was built for the build host even when cross-compiling, so `cargo zigbuild --target aarch64-unknown-linux-musl` produced a host-architecture `libzwasm.a` and failed at link time
- Moving the vendored zwasm did not rebuild anything. The build script declared no inputs, so a submodule bump left the previously built library and its bindings in place while the tests reported green against them
- The crate that reaches crates.io is now built and checked in CI, and asserted to carry no vendored Zig packages
- Missing `-lm`, which zwasm requires on glibc older than 2.34
- Missing `-Wl,-z,noexecstack` on Linux, needed because Zig emits no `.note.GNU-stack` section
- Removed an `-Wl,-rpath` pointing into `OUT_DIR`, which baked a build-host path into the binary
- Corrected the `CONTRIBUTING.md` link in the zwasm-sys README

### Removed
- The `examples/` directory and the `nix` dev-dependency, both of which only built against the 0.1 API

## [0.1.1] - 2026-08-13
### Changed
- Moved the repository to the zwasm organization and updated the repository and upstream URLs
- Updated the bundled zwasm C API to 1.11.1, the final release of the v1 line. The C header is unchanged, so the generated bindings are identical
- Pinned all GitHub Actions to full-length commit SHAs and added a Dependabot config

### Fixed
- docs.rs builds. The build script now skips the Zig build when `DOCS_RS` is set and generates the bindings from the bundled header
- Unresolved intra-doc links to `Config`, `Module`, `WasiConfig`, and `Imports`
- Removed Windows from the supported platform list in the crate docs. Only Linux and macOS are supported

## [0.1.0] - 2026-04-26
### Added
- Initial release of zwasm-sdk core API
- Safe Rust bindings for zwasm C API via zwasm-sys
- Unit tests (normal, error, edge cases)
- Integration tests and E2E tests using examples
- Practical examples: run_wasm, host_imports, memory_io, wasi_config
- CI with cargo fmt, clippy, test (Linux/macOS)
