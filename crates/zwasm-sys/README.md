# zwasm-sys

zwasm-sys provides low-level Rust bindings to the zwasm C API.
This crate offers FFI access to the zwasm WebAssembly runtime for use in higher-level Rust libraries and applications.

Bindings are generated with `bindgen` from three headers that zwasm installs:

| Header | Surface |
|--------|---------|
| `wasm.h` | The standard [wasm-c-api](https://github.com/WebAssembly/wasm-c-api) |
| `zwasm.h` | zwasm extensions (fuel, memory limits, interruption, engine selection) |
| `wasi.h` | WASI 0.1 host setup |

## Supported Rust Version

A recent stable Rust compiler is recommended. (Rust 2021 edition)

## Build and Link Requirements

- Requires [Zig](https://ziglang.org/) 0.16.0 to be installed and available in your PATH (used to build the zwasm C library).
- The zwasm C API is built automatically from the submodule during the build; no manual installation is needed.
- The library is linked **statically**, so nothing has to be installed on the machine that runs your binary.
- Supported platforms: **Unix-like systems (Linux, macOS)** only. Windows is not supported.

On docs.rs there is no Zig toolchain, so the build script skips the Zig build and generates the bindings from the vendored headers instead.

If you encounter build issues, please ensure Zig is installed and your environment supports building C libraries with Zig.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
zwasm-sys = "0.3"
```

## Version Compatibility

| zwasm-sys | zwasm C API |
|-----------|-------------|
| 0.3.x     | 2.7.x       |
| 0.2.x     | 2.6.x       |
| 0.1.x     | 1.11.x      |

zwasm 2.0 replaced the custom C API with the standard wasm-c-api, so none of the 0.1 symbols carry over.

## ⚠️ Safety and Usage Notes

This crate provides raw FFI bindings to the zwasm C API, generated automatically via `bindgen`.
**All functions and types exposed by zwasm-sys are inherently unsafe.**

- You must use `unsafe` blocks to call these functions.
- No safety guarantees are provided by this crate. It is your responsibility to ensure:
	- All pointers passed to FFI functions are valid and properly aligned.
	- Lifetimes and memory management follow the C API's requirements.
	- You handle error codes and null pointers as documented in the C API.
- Incorrect usage may lead to undefined behavior, memory corruption, or security vulnerabilities.

**Recommendation:**
Use this crate as a building block for higher-level, safe Rust libraries.
If you need a safe and ergonomic API, consider using or creating a wrapper crate that encapsulates unsafe usage.

### Example (unsafe FFI usage)

```rust
use zwasm_sys::*;

unsafe {
    let engine = wasm_engine_new();
    let store = wasm_store_new(engine);
    // ... check for null, build a wasm_byte_vec_t, call wasm_module_new, etc.
    wasm_store_delete(store);
    wasm_engine_delete(engine);
}
```

## API Reference

- [zwasm C API Documentation](https://github.com/zwasm/zwasm/blob/v2.7.0/docs/reference/c_api.md)

## License

- **Rust code in this repository (including zwasm-sys):** MIT License
- **zwasm C API (submodule):** See [zwasm LICENSE](https://github.com/zwasm/zwasm/blob/v2.7.0/LICENSE) for details. You must comply with the license of the zwasm C library in addition to the MIT License for Rust code.

## Contributing & Issue Reporting

Contributions, bug reports, and feature requests are welcome!

- Please open an issue if you find a bug or have a question.
- Pull requests are encouraged for improvements, documentation, or new tests.
- For major changes, please open an issue first to discuss what you would like to change.

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for details.
