//! Every public type prints. Two of them print in a shape worth pinning.

use zwasm_sdk::{
    Engine, Error, Func, Global, Instance, Memory, Module, Store, Table, TrapKind, Val, WasiConfig,
};

// (module (func (export "f")))
const EMPTY_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03, 0x02,
    0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x04, 0x01, 0x02, 0x00, 0x0b,
];

fn assert_debug<T: std::fmt::Debug>() {}

// The Rust API guidelines ask every public type for a Debug (C-DEBUG). This
// fails to compile rather than fails at runtime, which is the point: a new
// public type without one is caught when it is added.
#[test]
fn every_public_type_implements_debug() {
    assert_debug::<Engine>();
    assert_debug::<Error>();
    assert_debug::<Func>();
    assert_debug::<Global>();
    assert_debug::<Instance>();
    assert_debug::<Memory>();
    assert_debug::<Module>();
    assert_debug::<Store>();
    assert_debug::<Table>();
    assert_debug::<TrapKind>();
    assert_debug::<Val>();
    assert_debug::<WasiConfig>();
}

// Store's Debug is written out to keep its six ownership registries from
// printing as six lists of raw addresses. This pins the summary rather than the
// contents: counts, and no vector anywhere in the output.
#[test]
fn a_store_prints_what_it_holds_rather_than_which_pointers() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, EMPTY_WASM).unwrap();
    let _instance = Instance::new(&mut store, &module, &[]).unwrap();

    let printed = format!("{store:?}");

    assert!(printed.contains("modules: 1"), "{printed}");
    assert!(printed.contains("instances: 1"), "{printed}");
    assert!(printed.contains("tables: 0"), "{printed}");
    assert!(
        !printed.contains('['),
        "a registry printed as a list, so the pointers are back: {printed}"
    );
}

// An Engine is a handle; its clones share the C engine. The address is the only
// fact it carries, and printing it is what makes that sharing visible.
#[test]
fn engine_debug_shows_which_handles_share_an_engine() {
    let engine = Engine::new().unwrap();
    let clone = engine.clone();
    let separate = Engine::new().unwrap();

    assert_eq!(format!("{engine:?}"), format!("{clone:?}"));
    assert_ne!(format!("{engine:?}"), format!("{separate:?}"));
}
