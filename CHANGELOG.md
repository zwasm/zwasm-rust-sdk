# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]
### Changed
- **Breaking.** `Engine` is no longer `Send` or `Sync`. zwasm 2.7.0 states that its engine is single-threaded per *process*, not per store: stores share process-global state, so a thread deleting one store can free memory a call on another thread is still reading, even with no handle shared between them. The `unsafe impl` this crate carried let safe code build exactly that. Removing it is necessary rather than sufficient — two engines created independently on two threads are no safer — and nothing here prevents that yet
- Updated the bundled zwasm C API to 2.7.0

### Added
- `Instance::get_memory`, `Instance::get_global` and `Instance::get_table`, so a module's exported memory, global and table can be reached rather than only its functions. Reaching a guest's own memory is what was missing: `Memory::new` builds one the host owns and no module can see, so there was no path from an instantiated module to the bytes it actually uses. This is what `wasmi-benchmarks` needs for `read_memory` / `write_memory` (zwasm/zwasm#296). The four accessors share one walk over the export section, so a name that is not exported and an export of the wrong kind both come back as `None`. `get_global` also declines a global holding a reference rather than a number: `Val` models only the four numeric types, so a handle to one could only be read by panicking (#45)
- `Val` is `Copy`. It holds only the four numeric types, so a result can be read out of a slice by pattern match rather than by reference or a clone. wasmtime's `Val` is `Copy` too, with reference variants on it: those work because its `Rooted<T>` is a `Copy` handle into the store, which is the same model the handles here already use
- `Instance::set_memory_pages_limit` and `Instance::clear_memory_pages_limit`, a host ceiling on how far one instance can grow memory 0. The unit is wasm pages of 64 KiB, so a ceiling meant as 64 MiB is `1024`. It does not trap: a `memory.grow` past the cap returns the spec's own grow failure, `-1`, and leaves the memory where it was — in particular this is unrelated to `TrapKind::OutOfMemory`, which zwasm raises for an allocator failure or the GC heap's ceiling. It is also not the module's declared maximum, which belongs to the module and is what `Memory::grow` checks; this one is per instance and may sit below it
- `Instance::set_fuel`, `Instance::disable_fuel` and `Instance::fuel_remaining`, which make `TrapKind::OutOfFuel` reachable — the kind was public but nothing here could give a guest a budget to exhaust. The budget is per instance, where wasmtime meters a whole `Store`, and `set_fuel` re-arms rather than adds, so an instance that ran out runs again once re-armed. `fuel_remaining` is `None` for an unmetered instance and `Some(0)` for one that ran out, which are different states. A unit is engine-specific — the interpreter counts instructions executed, the JIT counts poll-site crossings — so a budget only means something portable when the engine is pinned with `new_with_engine`
- `Instance::new_with_engine` and `Instance::engine`, wrapping `zwasm_instance_new_ex` and `zwasm_instance_engine`. The engine stays zwasm's choice by default — `Instance::new` is `new_with_engine` with `EngineKind::Auto`, which is what stock `wasm_instance_new` already passed — and the accessor reports which engine actually ran, never `Auto`, so a caller can record what the default resolved to instead of assuming it
- `TrapKind::InvalidModule` and `TrapKind::Unsupported`, for the two kinds zwasm 2.7.0 added. Neither is a guest fault, and neither is something a different engine choice fixes where it appears: the first comes from instantiation when the JIT judges a module invalid, the second from a call the engine has no implementation for, and `AUTO` falls back to the interpreter for neither
- The Zig target can be set with `ZWASM_ZIG_TARGET`, and is otherwise taken from `CARGO_ZIGBUILD_TARGET_<target>` or `CARGO_ZIGBUILD_TARGET` when cargo-zigbuild 0.23.4 or later exports them. Without any of these the triple is computed as before, so a build that sets nothing is unchanged. This is what lets a build ask for a glibc floor: the version in `--target x86_64-unknown-linux-gnu.2.28` never reached the C half before, so it was built for whatever glibc Zig defaults to

### Fixed
- `Error::Trap` no longer carries a trailing NUL in its message, so a trap printed as `unreachable\0` now prints as `unreachable`. `wasm.h` has always declared the message null-terminated and counted the terminator in its size; zwasm began honouring that in 2.7.0, and this crate had been reading the size verbatim

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
