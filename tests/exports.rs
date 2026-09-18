//! Reaching what a module exports, not just what it exports as a function.
//!
//! Every accessor walks the same path — resolve the name against the module's
//! export section, reuse that index against the instance's — and differs only
//! in the conversion at the end. These tests exercise all four so a change to
//! the shared walk cannot break one kind quietly.

use zwasm_sdk::{Engine, EngineKind, Instance, Module, Store, Val};

// (module
//   (memory (export "mem") 1)
//   (global (export "g") i32 (i32.const 7))
//   (table (export "t") 2 funcref)
//   (func (export "f") (result i32) (i32.const 1))
//   (func (export "poke") (param $at i32) (param $v i32)
//     (i32.store8 (local.get $at) (local.get $v)))
//   (func (export "size") (result i32) (memory.size)))
const EXPORTS: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x0a, 0x02, 0x60, 0x00, 0x01, 0x7f, 0x60,
    0x02, 0x7f, 0x7f, 0x00, 0x03, 0x04, 0x03, 0x00, 0x01, 0x00, 0x04, 0x04, 0x01, 0x70, 0x00, 0x02,
    0x05, 0x03, 0x01, 0x00, 0x01, 0x06, 0x06, 0x01, 0x7f, 0x00, 0x41, 0x07, 0x0b, 0x07, 0x21, 0x06,
    0x03, 0x6d, 0x65, 0x6d, 0x02, 0x00, 0x01, 0x67, 0x03, 0x00, 0x01, 0x74, 0x01, 0x00, 0x01, 0x66,
    0x00, 0x00, 0x04, 0x70, 0x6f, 0x6b, 0x65, 0x00, 0x01, 0x04, 0x73, 0x69, 0x7a, 0x65, 0x00, 0x02,
    0x0a, 0x15, 0x03, 0x04, 0x00, 0x41, 0x01, 0x0b, 0x09, 0x00, 0x20, 0x00, 0x20, 0x01, 0x3a, 0x00,
    0x00, 0x0b, 0x04, 0x00, 0x3f, 0x00, 0x0b,
];

// (module (global (export "cb") funcref (ref.null func))
//         (global (export "n") i32 (i32.const 7)))
const REFERENCE_GLOBAL: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x06, 0x0b, 0x02, 0x70, 0x00, 0xd0, 0x70, 0x0b,
    0x7f, 0x00, 0x41, 0x07, 0x0b, 0x07, 0x0a, 0x02, 0x02, 0x63, 0x62, 0x03, 0x00, 0x01, 0x6e, 0x03,
    0x01,
];

fn instantiate_bytes(wasm: &[u8]) -> (Store, Instance) {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, wasm).unwrap();
    let instance = Instance::new_with_engine(&mut store, &module, &[], EngineKind::Interp).unwrap();
    (store, instance)
}

fn instantiate() -> (Store, Instance) {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, EXPORTS).unwrap();
    let instance = Instance::new_with_engine(&mut store, &module, &[], EngineKind::Interp).unwrap();
    (store, instance)
}

// What #26 was filed for: a guest writes into its own memory and the host reads
// it back. `Memory::new` builds a memory no module can see, so before this there
// was no path from an instance to the bytes a guest actually uses.
#[test]
fn a_guests_memory_can_be_read_by_the_host() {
    let (mut store, instance) = instantiate();
    let poke = instance.get_func(&mut store, "poke").unwrap();
    let mut no_results = Vec::new();
    poke.call(&mut store, &[Val::I32(5), Val::I32(42)], &mut no_results)
        .unwrap();

    let memory = instance.get_memory(&mut store, "mem").unwrap();
    assert_eq!(memory.size(&store), 1);
    assert_eq!(memory.data(&store)[5], 42);
}

#[test]
fn an_exported_global_carries_its_declared_value() {
    let (mut store, instance) = instantiate();
    let global = instance.get_global(&mut store, "g").unwrap();
    assert_eq!(global.get(&store), Val::I32(7));
}

#[test]
fn an_exported_table_carries_its_declared_size() {
    let (mut store, instance) = instantiate();
    let table = instance.get_table(&mut store, "t").unwrap();
    assert_eq!(table.size(&store), 2);
}

// The C conversion returns null for an export of the wrong kind, and the copy
// passes null through, so asking for the wrong kind is indistinguishable from
// asking for a name that is not there. Both are `None`, on purpose.
#[test]
fn asking_for_the_wrong_kind_is_none() {
    let (mut store, instance) = instantiate();
    assert!(instance.get_memory(&mut store, "f").is_none());
    assert!(instance.get_func(&mut store, "mem").is_none());
    assert!(instance.get_global(&mut store, "t").is_none());
    assert!(instance.get_table(&mut store, "g").is_none());
}

#[test]
fn a_name_that_is_not_exported_is_none() {
    let (mut store, instance) = instantiate();
    assert!(instance.get_func(&mut store, "nope").is_none());
    assert!(instance.get_memory(&mut store, "nope").is_none());
    assert!(instance.get_global(&mut store, "nope").is_none());
    assert!(instance.get_table(&mut store, "nope").is_none());
}

// `Instance::set_memory_pages_limit` caps the guest's `memory.grow`, measured in
// its own tests. It does not cap a host that reaches the same memory through
// `get_memory` — a door that did not exist until this change, so the asymmetry
// is new and is what `set_memory_pages_limit`'s doc now states.
//
// Not a sandbox hole: nothing in a module can reach `Memory::grow`, so the only
// party that can pass the cap is the one that set it.
#[test]
fn a_host_side_grow_is_not_bounded_by_the_instance_cap() {
    let (mut store, instance) = instantiate();
    let memory = instance.get_memory(&mut store, "mem").unwrap();
    instance.set_memory_pages_limit(&mut store, 2);

    assert_eq!(
        memory.grow(&mut store, 1).unwrap(),
        1,
        "1 -> 2 is at the cap"
    );
    assert_eq!(memory.grow(&mut store, 1).unwrap(), 2, "2 -> 3 passes it");

    // And the guest sees the pages the host added.
    let size = instance.get_func(&mut store, "size").unwrap();
    let mut results = vec![Val::I32(0)];
    size.call(&mut store, &[], &mut results).unwrap();
    assert_eq!(results, [Val::I32(3)]);
}

// A global holding a reference is declined rather than returned. `Val` models
// only the four numeric types, so `Global::get` on one would panic, and nothing
// on `Global` reports its type for a caller to check first — so a handle here
// would be one whose only read method crashes. wasmtime returns it, because its
// `Val` carries reference variants; #45 is that gap.
//
// The numeric global beside it in the same module is the control: the filter
// has to decline one kind, not globals in general.
#[test]
fn a_reference_global_is_declined_and_a_numeric_one_is_not() {
    let (mut store, instance) = instantiate_bytes(REFERENCE_GLOBAL);

    assert!(
        instance.get_global(&mut store, "cb").is_none(),
        "a funcref global has no representation to hand back"
    );

    let numeric = instance
        .get_global(&mut store, "n")
        .expect("a numeric global in the same module still resolves");
    assert_eq!(numeric.get(&store), Val::I32(7));
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn get_memory_with_a_foreign_store_panics() {
    let (_store_a, instance) = instantiate();
    let engine = Engine::new().unwrap();
    let mut store_b = Store::new(&engine).unwrap();
    let _ = instance.get_memory(&mut store_b, "mem");
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn get_global_with_a_foreign_store_panics() {
    let (_store_a, instance) = instantiate();
    let engine = Engine::new().unwrap();
    let mut store_b = Store::new(&engine).unwrap();
    let _ = instance.get_global(&mut store_b, "g");
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn get_table_with_a_foreign_store_panics() {
    let (_store_a, instance) = instantiate();
    let engine = Engine::new().unwrap();
    let mut store_b = Store::new(&engine).unwrap();
    let _ = instance.get_table(&mut store_b, "t");
}
