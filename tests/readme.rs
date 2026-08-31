//! The README's examples, compiled.
//!
//! They are not doctests — nothing in the normal build reads README.md — so an
//! API change can leave them stale with every check still green. That happened
//! once; this file is what notices next time.
//!
//! Kept in sync by hand. If these stop matching the README, fix the README.

use zwasm_sdk::{Engine, Instance, Module, Store, Val, WasiConfig};

// (module (func (export "add") (param i32 i32) (result i32)
//   (i32.add (local.get 0) (local.get 1))))
const ADD_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01,
    0x7f, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00, 0x0a, 0x09,
    0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
];

// README "Quick Start". The example reads the module from a file; here the
// bytes are inline so the test can also run it.
#[test]
fn readme_quick_start() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = ADD_WASM;

    let engine = Engine::new()?;
    let mut store = Store::new(&engine)?;
    let module = Module::new(&mut store, wasm)?;
    let instance = Instance::new(&mut store, &module, &[])?;

    let add = instance
        .get_func(&mut store, "add")
        .ok_or("no export named add")?;
    let mut results = [Val::I32(0)];
    add.call(&mut store, &[Val::I32(10), Val::I32(32)], &mut results)?;
    println!("results = {results:?}");

    assert_eq!(results, [Val::I32(42)]);
    Ok(())
}

// README "WASI".
#[test]
fn readme_wasi_setup() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::new()?;
    let mut store = Store::new(&engine)?;

    let mut wasi = WasiConfig::new()?;
    wasi.set_args(&["prog", "--flag"])?;
    wasi.set_envs(&[("KEY", "VALUE")])?;
    wasi.preopen_dir(std::env::temp_dir().to_str().unwrap(), "/")?;
    wasi.inherit_stdio();

    store.set_wasi(wasi);
    Ok(())
}
