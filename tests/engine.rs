//! Engine selection: which engine an instance was asked for, and which one it
//! actually got.
//!
//! `AUTO` is a request, not an answer — `Instance::engine` is what reports the
//! engine that ran, so a benchmark or a differential test can record what the
//! default resolved to rather than assuming it.

use zwasm_sdk::{Engine, EngineKind, Instance, Module, Store};

// (module (func (export "f") (result i32) (i32.const 7)))
const WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
    0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x06, 0x01, 0x04, 0x00, 0x41,
    0x07, 0x0b,
];

/// Instantiates `WASM` on `kind` and reports the engine that ran it.
fn resolved(kind: Option<EngineKind>) -> EngineKind {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, WASM).unwrap();
    let instance = match kind {
        Some(kind) => Instance::new_with_engine(&mut store, &module, &[], kind).unwrap(),
        None => Instance::new(&mut store, &module, &[]).unwrap(),
    };
    instance.engine(&store)
}

#[test]
fn forcing_the_interpreter_gets_the_interpreter() {
    assert_eq!(resolved(Some(EngineKind::Interp)), EngineKind::Interp);
}

#[test]
fn forcing_the_jit_gets_the_jit() {
    assert_eq!(resolved(Some(EngineKind::Jit)), EngineKind::Jit);
}

// `zwasm.h` says the stock `wasm_instance_new` is AUTO, and `Instance::new`
// forwards to `new_with_engine(.., Auto)` on that basis. This measures the
// claim instead of inheriting it: if the stock entry point ever stops being
// AUTO, the two resolve differently and this fails.
#[test]
fn the_default_constructor_is_auto() {
    assert_eq!(resolved(None), resolved(Some(EngineKind::Auto)));
}

// AUTO is what you asked for; the accessor is what you got. An instance never
// reports AUTO, however it was created.
#[test]
fn the_resolved_kind_is_never_auto() {
    for asked in [None, Some(EngineKind::Auto)] {
        assert_ne!(resolved(asked), EngineKind::Auto);
    }
}

// zwasm's engine kinds are C defines rather than a Zig enum, so an added kind
// arrives as a number this crate has not seen. Carrying it keeps that from
// being a panic.
#[test]
fn an_unknown_kind_round_trips() {
    assert_eq!(EngineKind::from(99), EngineKind::Unknown(99));
}

// Each ZWASM_ENGINE_* constant maps to the variant named after it. The numbers
// come from the bindings rather than being written out again, so bumping the
// submodule to a zwasm that renumbers a kind breaks this instead of silently
// mislabelling engines.
#[test]
fn every_constant_maps_to_the_variant_named_after_it() {
    use zwasm_sys as sys;

    let pairs: &[(u32, EngineKind)] = &[
        (sys::ZWASM_ENGINE_AUTO, EngineKind::Auto),
        (sys::ZWASM_ENGINE_JIT, EngineKind::Jit),
        (sys::ZWASM_ENGINE_INTERP, EngineKind::Interp),
    ];

    for &(code, expected) in pairs {
        assert_eq!(
            EngineKind::from(code as i32),
            expected,
            "ZWASM_ENGINE_* constant {code} maps to the wrong variant"
        );
    }
}

/// The header is the authority on how many kinds exist, so the sweep reads it.
///
/// Bounding the sweep by the table above instead makes the check satisfy
/// itself: when zwasm appends a kind, the old bound still passes and the new
/// constant falls silently to `EngineKind::Unknown`. This is the shape that
/// caught `ZWASM_TRAP_WASI_EXIT` — see `trap_kind.rs`.
#[test]
fn every_kind_the_header_declares_has_a_variant() {
    let header = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/zwasm-sys/zwasm/include/zwasm.h"
    );
    let text =
        std::fs::read_to_string(header).unwrap_or_else(|e| panic!("cannot read {header}: {e}"));

    let mut declared = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("#define ZWASM_ENGINE_") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let (Some(suffix), Some(value)) = (parts.next(), parts.next()) else {
            continue;
        };
        let value: i32 = value.parse().unwrap_or_else(|e| {
            panic!("ZWASM_ENGINE_{suffix} has a non-integer value {value:?}: {e}")
        });
        declared.push((suffix.to_string(), value));
    }

    // A parse that silently matched nothing would pass for the wrong reason.
    assert!(
        !declared.is_empty(),
        "no ZWASM_ENGINE_* defines found in {header}"
    );

    let unmapped: Vec<String> = declared
        .iter()
        .filter(|(_, value)| matches!(EngineKind::from(*value), EngineKind::Unknown(_)))
        .map(|(suffix, value)| format!("ZWASM_ENGINE_{suffix} = {value}"))
        .collect();

    assert!(
        unmapped.is_empty(),
        "the header declares kinds EngineKind maps to Unknown: {}",
        unmapped.join(", ")
    );
}

// The store check guards the new constructor too, not just `Instance::new`.
#[test]
#[should_panic(expected = "store it does not belong to")]
fn instantiating_a_foreign_module_with_an_engine_panics() {
    let engine = Engine::new().unwrap();
    let mut store_a = Store::new(&engine).unwrap();
    let mut store_b = Store::new(&engine).unwrap();
    let module = Module::new(&mut store_b, WASM).unwrap();
    let _ = Instance::new_with_engine(&mut store_a, &module, &[], EngineKind::Interp);
}
