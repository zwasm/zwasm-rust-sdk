//! # zwasm-sdk
//!
//! Safe Rust bindings for [zwasm](https://github.com/zwasm/zwasm), a WebAssembly
//! runtime written in Zig.
//!
//! The types map onto the [wasm-c-api](https://github.com/WebAssembly/wasm-c-api)
//! object model that zwasm 2.x exposes, and the ownership model follows
//! [wasmtime](https://docs.rs/wasmtime): the [`Store`] owns every
//! object created through it, and the other types are `Copy` handles naming an
//! object inside a store. Using a handle means passing the store back in, so the
//! borrow checker keeps every use inside the store's lifetime, and a handle used
//! with the wrong store panics.
//!
//! | Type | C type | Role |
//! |------|--------|------|
//! | [`Engine`] | `wasm_engine_t` | Compilation environment; `Clone`, one thread per process |
//! | [`Store`] | `wasm_store_t` | Owns the runtime state for one thread |
//! | [`Module`] | `wasm_module_t` | A validated module |
//! | [`Instance`] | `wasm_instance_t` | An instantiated module |
//! | [`Func`] | `wasm_func_t` | A callable function |
//! | [`Val`] | `wasm_val_t` | An i32/i64/f32/f64 value |
//! | [`Memory`], [`Global`], [`Table`] | `wasm_memory_t`, ... | Runtime entities |
//! | [`WasiConfig`] | `zwasm_wasi_config_t` | WASI 0.1 host setup |
//!
//! The store frees everything on drop — children before parents, then the C store,
//! then its reference to the engine. zwasm resolves every deallocation through
//! store and engine back-pointers, so that order is what makes the drop safe, and
//! there is nothing to release by hand.
//!
//! ## Example
//!
//! ```
//! use zwasm_sdk::{Engine, Instance, Module, Store, Val};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // (module (func (export "add") (param i32 i32) (result i32)
//! //   (i32.add (local.get 0) (local.get 1))))
//! let wasm: &[u8] = &[
//!     0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f,
//!     0x01, 0x7f, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00,
//!     0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
//! ];
//!
//! let engine = Engine::new()?;
//! let mut store = Store::new(&engine)?;
//! let module = Module::new(&mut store, wasm)?;
//! let instance = Instance::new(&mut store, &module, &[])?;
//!
//! let add = instance.get_func(&mut store, "add").ok_or("no export named add")?;
//! let mut results = [Val::I32(0)];
//! add.call(&mut store, &[Val::I32(10), Val::I32(32)], &mut results)?;
//! assert_eq!(results, [Val::I32(42)]);
//! # Ok(())
//! # }
//! ```
//!
//! ## WASI
//!
//! Build a [`WasiConfig`] and install it on the store before
//! instantiating. The store takes ownership of the config, so it is passed by value.
//!
//! ```no_run
//! use zwasm_sdk::{Engine, Store, WasiConfig};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let engine = Engine::new()?;
//! let mut store = Store::new(&engine)?;
//!
//! let mut wasi = WasiConfig::new()?;
//! wasi.set_args(&["prog", "--flag"])?;
//! wasi.set_envs(&[("KEY", "VALUE")])?;
//! wasi.preopen_dir("/host/dir", "/")?;
//! wasi.inherit_stdio();
//!
//! store.set_wasi(wasi);
//! # Ok(())
//! # }
//! ```
//!
//! Imports of `wasi_snapshot_preview1.*` then resolve against that host.
//!
//! ## Host functions
//!
//! [`Func::new`](func::Func::new) wraps a Rust closure so a guest can call into
//! Rust, taking the signature as [`ValType`] lists, and
//! [`Instance::new`](instance::Instance::new) takes the resulting functions as
//! imports, in the order the module declares them —
//! [`Module::imports`](module::Module::imports) reports that order, so a caller
//! who did not write the module can still match its own functions by name.
//!
//! The closure sees its arguments and nothing else: reading the caller's memory
//! needs an instance that zwasm's callback does not carry. [`Func::new_host`]
//! remains for an embedder that already has an `extern "C"` callback, and is
//! `unsafe` because the function type is built from raw `zwasm_sys` types.
//!
//! [`Func::new_host`]: func::Func::new_host
//!
//! ## Observability
//!
//! Five engine events can be listened to, so that an embedder can watch a guest
//! without instrumenting it: [`Engine::set_compile_hook`],
//! [`Engine::set_instantiate_hook`], [`Engine::set_trap_hook`],
//! [`Engine::set_fuel_exhausted_hook`] and
//! [`Engine::set_memory_growth_hook`], each with a `clear_*` beside it.
//!
//! ```
//! use std::cell::Cell;
//! use std::rc::Rc;
//! use zwasm_sdk::{Engine, Module, Store};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let wasm: &[u8] = &[
//! #     0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f,
//! #     0x03, 0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x06, 0x01, 0x04,
//! #     0x00, 0x41, 0x07, 0x0b,
//! # ];
//! let engine = Engine::new()?;
//!
//! let traps = Rc::new(Cell::new(0u32));
//! let counted = Rc::clone(&traps);
//! engine.set_trap_hook(move |_instance, kind, message| {
//!     // Borrowed for this call only: copy anything you keep.
//!     eprintln!("trap: {kind:?} {message}");
//!     counted.set(counted.get() + 1);
//! });
//!
//! let mut store = Store::new(&engine)?;
//! let module = Module::new(&mut store, wasm)?;
//! # let _ = module;
//! assert_eq!(traps.get(), 0);
//! # Ok(())
//! # }
//! ```
//!
//! Install them before the engine is used, and remember that clones share one
//! C engine, so a hook set through any of them is set for all. A hook fires in
//! the middle of the operation it reports: record what is needed and return,
//! because reaching the engine, its stores or its instances from inside one is
//! undefined.
//!
//! A hook reports an event, not a duration — zwasm's core carries no clock, so
//! timing is the embedder's to do. What each hook does and does not promise is
//! on the method; the promises are narrower than the names suggest.
//!
//! ## Build requirements
//!
//! [Zig](https://ziglang.org/) 0.16.0 must be on `PATH`. The zwasm C library is
//! built from the vendored submodule and linked statically, so nothing has to be
//! installed on the target machine.

use zwasm_sys as sys;

pub mod engine;
pub mod error;
pub mod func;
pub mod global;
pub mod instance;
pub mod memory;
pub mod module;
pub mod store;
pub mod table;
pub mod val;
pub mod wasi;

// Every public type is also reachable from the crate root, which is how
// wasmtime presents its own — `use zwasm_sdk::{Engine, Store}` rather than one
// line per module. The modules stay public so that the longer paths, and the
// `crate::store::Store` links in these docs, keep working.
//
// A new public type belongs here as well as in its module.
pub use crate::engine::Engine;
pub use crate::error::{Error, TrapKind};
pub use crate::func::Func;
pub use crate::global::Global;
pub use crate::instance::{EngineKind, Instance};
pub use crate::memory::Memory;
pub use crate::module::{ExternKind, ImportType, Module};
pub use crate::store::Store;
pub use crate::table::Table;
pub use crate::val::{Val, ValType};
pub use crate::wasi::WasiConfig;

/// The version of the zwasm C library this binary is linked against.
///
/// Not the version of this crate, which is `CARGO_PKG_VERSION` and moves
/// independently: 0.2 of the SDK wraps 2.x of zwasm.
///
/// # This is a version, not a build identity
///
/// zwasm's `-Dwasm`, `-Dwasi` and `-Dengine` are compile-time options, and two
/// builds of the same commit differing in all three return the same string
/// (zwasm's ADR-0221). So `"2.6.0"` says which release the library was built
/// from; it does not promise the library holds everything that release can do.
/// Do not branch on it to decide whether a feature is present.
///
/// # Panics
///
/// When the C library breaks its own contract by returning NULL or a string
/// that is not UTF-8. Neither is a condition a caller can act on.
///
/// Requires zwasm 2.6.0 or later; the symbol does not exist before it.
pub fn runtime_version() -> &'static str {
    let ptr = unsafe { sys::zwasm_version() };
    assert!(
        !ptr.is_null(),
        "zwasm_version() returned NULL, which its header rules out"
    );

    // SAFETY: the pointer is non-null by the assertion above, and zwasm.h
    // documents the string as static storage that is never freed — which is
    // what makes the 'static in the return type sound rather than merely
    // accepted by the compiler.
    let c_str = unsafe { std::ffi::CStr::from_ptr(ptr) };

    c_str
        .to_str()
        .expect("zwasm_version() returned a string that is not UTF-8")
}
