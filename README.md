# zwasm Rust SDK

zwasm-sdk provides safe, idiomatic Rust bindings to the [zwasm](https://github.com/zwasm/zwasm) WebAssembly runtime.

## Supported Rust Version

A recent stable Rust compiler is recommended. (Rust 2021 edition)

## Build and Platform Requirements

- Requires [Zig](https://ziglang.org/) 0.16.0 in your PATH (used to build the zwasm C library)
- Supported platforms: **Linux (x86_64, aarch64), macOS (x86_64, aarch64)**

The zwasm C library is built from the vendored submodule and linked statically, so no shared library has to be installed on the machine that runs your binary.

Building it needs no network: zwasm's root package declares no dependencies, so
`zig build static-lib` fetches nothing even on a cold Zig cache.

Cross-compiling works through [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild):
`cargo zigbuild --target <triple>` builds the C library for that target rather
than for the build host. CI covers `aarch64-unknown-linux-musl` and
`x86_64-unknown-linux-musl`.

A glibc version on the target — `--target x86_64-unknown-linux-gnu.2.28` — is
honoured with cargo-zigbuild 0.23.4 or later, which exports the resolved target
for build scripts to read. On older versions the C library is built for
whatever glibc Zig defaults to, while the link step targets the version asked
for.

`ZWASM_ZIG_TARGET` overrides the target handed to Zig outright, for anything
`-Dtarget` accepts — a glibc floor, CPU features, an ABI variant. It takes
precedence over everything above, and is the way to reach a floor below Zig's
default without cargo-zigbuild.

## Version Compatibility

| zwasm-sdk | zwasm-sys | zwasm C API |
|-----------|-----------|-------------|
| 0.2.x     | 0.2.x     | 2.6.x       |
| 0.1.x     | 0.1.x     | 1.11.x      |

zwasm 2.0 replaced the custom C API with the standard [wasm-c-api](https://github.com/WebAssembly/wasm-c-api), so 0.2 is a full rewrite with no path from 0.1. See [CHANGELOG.md](CHANGELOG.md).

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
zwasm-sdk = "0.2"
```

## Example

```rust
use zwasm_sdk::{Engine, Instance, Module, Store, Val};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = std::fs::read("your_module.wasm")?;

    let engine = Engine::new()?;
    let mut store = Store::new(&engine)?;
    let module = Module::new(&mut store, &wasm)?;
    let instance = Instance::new(&mut store, &module, &[])?;

    let add = instance
        .get_func(&mut store, "add")
        .ok_or("no export named add")?;
    let mut results = [Val::I32(0)];
    add.call(&mut store, &[Val::I32(10), Val::I32(32)], &mut results)?;
    println!("results = {results:?}");
    Ok(())
}
```

`Engine`, `Store`, `Module`, `Instance` and `Func` mirror the wasm-c-api object model. Each one frees its C counterpart on drop.

## WASI

Build a `WasiConfig` and install it on the store before instantiating. The store takes ownership of the config, so it is passed by value.

```rust
use zwasm_sdk::{Engine, Store, WasiConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::new()?;
    let mut store = Store::new(&engine)?;

    let mut wasi = WasiConfig::new()?;
    wasi.set_args(&["prog", "--flag"])?;
    wasi.set_envs(&[("KEY", "VALUE")])?;
    wasi.preopen_dir("/host/dir", "/")?;
    wasi.inherit_stdio();

    store.set_wasi(wasi);
    Ok(())
}
```

Imports of `wasi_snapshot_preview1.*` then resolve against that host. WASI 0.1 is supported; the Component Model and WASI 0.2 surfaces of zwasm are not wrapped yet.

## Safety and Usage Notes

All FFI unsafety is encapsulated, except for host functions: `Func::new_host` still takes a raw `wasm_functype_t` and a C callback. A safe builder for function types is not implemented yet.

`Engine` is `Send + Sync`. `Store` and everything derived from it are single-threaded.

For low-level access, see [zwasm-sys](crates/zwasm-sys).

## API Reference

- [API documentation on docs.rs](https://docs.rs/zwasm-sdk)
- [zwasm C API documentation](https://github.com/zwasm/zwasm/blob/v2.6.0/docs/reference/c_api.md)

## License

MIT License. See [LICENSE](LICENSE) for details.

## Contributing

Contributions, bug reports, and feature requests are welcome!
See [CONTRIBUTING.md](CONTRIBUTING.md) for details.
