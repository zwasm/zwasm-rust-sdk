//! # zwasm-sdk
//!
//! Safe Rust bindings for [zwasm](https://github.com/zwasm/zwasm), a WebAssembly
//! runtime written in Zig.
//!
//! The types map onto the [wasm-c-api](https://github.com/WebAssembly/wasm-c-api)
//! object model that zwasm 2.x exposes, and the ownership model follows
//! [wasmtime](https://docs.rs/wasmtime): the [`Store`](store::Store) owns every
//! object created through it, and the other types are `Copy` handles naming an
//! object inside a store. Using a handle means passing the store back in, so the
//! borrow checker keeps every use inside the store's lifetime, and a handle used
//! with the wrong store panics.
//!
//! | Type | C type | Role |
//! |------|--------|------|
//! | [`Engine`](engine::Engine) | `wasm_engine_t` | Compilation environment; `Clone + Send + Sync` |
//! | [`Store`](store::Store) | `wasm_store_t` | Owns the runtime state for one thread |
//! | [`Module`](module::Module) | `wasm_module_t` | A validated module |
//! | [`Instance`](instance::Instance) | `wasm_instance_t` | An instantiated module |
//! | [`Func`](func::Func) | `wasm_func_t` | A callable function |
//! | [`Val`](val::Val) | `wasm_val_t` | An i32/i64/f32/f64 value |
//! | [`Memory`](memory::Memory), [`Global`](global::Global), [`Table`](table::Table) | `wasm_memory_t`, ... | Runtime entities |
//! | [`WasiConfig`](wasi::WasiConfig) | `zwasm_wasi_config_t` | WASI 0.1 host setup |
//!
//! The store frees everything on drop — children before parents, then the C store,
//! then its reference to the engine. zwasm resolves every deallocation through
//! store and engine back-pointers, so that order is what makes the drop safe, and
//! there is nothing to release by hand.
//!
//! ## Example
//!
//! ```
//! use zwasm_sdk::engine::Engine;
//! use zwasm_sdk::instance::Instance;
//! use zwasm_sdk::module::Module;
//! use zwasm_sdk::store::Store;
//! use zwasm_sdk::val::Val;
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
//! Build a [`WasiConfig`](wasi::WasiConfig) and install it on the store before
//! instantiating. The store takes ownership of the config, so it is passed by value.
//!
//! ```no_run
//! use zwasm_sdk::engine::Engine;
//! use zwasm_sdk::store::Store;
//! use zwasm_sdk::wasi::WasiConfig;
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
//! [`Func::new_host`](func::Func::new_host) wraps a C callback so a guest can call
//! into Rust, and [`Instance::new`](instance::Instance::new) takes the resulting
//! functions as imports, in the order the module declares them. It is an `unsafe`
//! function: the type is still built from raw `zwasm_sys` types, and a safe builder
//! for function types is not implemented yet.
//!
//! ## Build requirements
//!
//! [Zig](https://ziglang.org/) 0.16.0 must be on `PATH`. The zwasm C library is
//! built from the vendored submodule and linked statically, so nothing has to be
//! installed on the target machine.

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
