//! A host ceiling on how far one instance can grow its memory.
//!
//! The ceiling is enforced the way the spec fails a grow — `memory.grow`
//! returns `-1` and the memory stays where it was. Nothing traps, and nothing
//! reaches the host, so every test here reads the guest's own return value.

use zwasm_sdk::{Engine, EngineKind, Func, Instance, Module, Store, Val};

// (module (memory (export "mem") 1)
//   (func (export "g") (param $d i32) (result i32) (memory.grow (local.get $d)))
//   (func (export "size") (result i32) (memory.size)))
const MEM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x0a, 0x02, 0x60, 0x01, 0x7f, 0x01, 0x7f,
    0x60, 0x00, 0x01, 0x7f, 0x03, 0x03, 0x02, 0x00, 0x01, 0x05, 0x03, 0x01, 0x00, 0x01, 0x07, 0x12,
    0x03, 0x03, 0x6d, 0x65, 0x6d, 0x02, 0x00, 0x01, 0x67, 0x00, 0x00, 0x04, 0x73, 0x69, 0x7a, 0x65,
    0x00, 0x01, 0x0a, 0x0d, 0x02, 0x06, 0x00, 0x20, 0x00, 0x40, 0x00, 0x0b, 0x04, 0x00, 0x3f, 0x00,
    0x0b,
];

/// The spec's `memory.grow` failure value.
const GROW_FAILED: i32 = -1;

/// A store holding one instance of `MEM` on `kind`, with its two exports.
///
/// The exports are resolved once and carried: `get_func` allocates a fresh C
/// handle the store owns until it drops.
fn memory_instance(kind: EngineKind) -> (Store, Instance, Func, Func) {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, MEM).unwrap();
    let instance = Instance::new_with_engine(&mut store, &module, &[], kind).unwrap();
    let grow = instance.get_func(&mut store, "g").unwrap();
    let size = instance.get_func(&mut store, "size").unwrap();
    (store, instance, grow, size)
}

/// Calls the guest's `memory.grow`, returning what it returned.
fn grow(store: &mut Store, f: &Func, delta: i32) -> i32 {
    let mut results = vec![Val::I32(0); f.result_arity(store)];
    f.call(store, &[Val::I32(delta)], &mut results)
        .expect("growing past a cap is a return value, never a trap");
    match &results[0] {
        Val::I32(n) => *n,
        other => panic!("memory.grow returned {other:?}"),
    }
}

fn size(store: &mut Store, f: &Func) -> i32 {
    let mut results = vec![Val::I32(0); f.result_arity(store)];
    f.call(store, &[], &mut results).unwrap();
    match &results[0] {
        Val::I32(n) => *n,
        other => panic!("memory.size returned {other:?}"),
    }
}

// The issue's first acceptance criterion, restated: it fails the grow, it does
// not trap. #21 was filed saying this produces `TrapKind::OutOfMemory`; it does
// not, and `grow`'s `expect` above is what holds that correction in place.
#[test]
fn growing_past_the_cap_fails_the_grow_rather_than_trapping() {
    let (mut store, instance, g, sz) = memory_instance(EngineKind::Interp);
    instance.set_memory_pages_limit(&mut store, 2);

    assert_eq!(grow(&mut store, &g, 1), 1, "1 -> 2 is at the cap");
    assert_eq!(grow(&mut store, &g, 1), GROW_FAILED, "2 -> 3 is past it");
    assert_eq!(size(&mut store, &sz), 2, "a failed grow changes nothing");
}

// The issue's second acceptance criterion.
#[test]
fn clearing_the_cap_lets_the_same_grow_through() {
    let (mut store, instance, g, sz) = memory_instance(EngineKind::Interp);
    instance.set_memory_pages_limit(&mut store, 2);
    assert_eq!(grow(&mut store, &g, 2), GROW_FAILED);

    instance.clear_memory_pages_limit(&mut store);
    assert_eq!(grow(&mut store, &g, 2), 1, "the same grow now succeeds");
    assert_eq!(size(&mut store, &sz), 3);
}

// An instance nobody capped grows freely: the cap is opt-in, not a default.
#[test]
fn an_uncapped_instance_grows() {
    let (mut store, _instance, g, sz) = memory_instance(EngineKind::Interp);
    assert_eq!(grow(&mut store, &g, 40), 1);
    assert_eq!(size(&mut store, &sz), 41);
}

// Lowering the cap under the size already allocated shrinks nothing and stops
// everything after it — including a grow of zero pages, which succeeds at any
// cap the size has not already passed. Measured; `set_memory_pages_limit`'s doc
// states it.
#[test]
fn a_cap_below_the_current_size_shrinks_nothing_and_stops_everything() {
    let (mut store, instance, g, sz) = memory_instance(EngineKind::Interp);
    assert_eq!(grow(&mut store, &g, 3), 1);
    assert_eq!(size(&mut store, &sz), 4);

    instance.set_memory_pages_limit(&mut store, 2);
    assert_eq!(size(&mut store, &sz), 4, "the four pages stay allocated");
    assert_eq!(grow(&mut store, &g, 0), GROW_FAILED, "even a no-op grow");
    assert_eq!(grow(&mut store, &g, 1), GROW_FAILED);
}

#[test]
fn a_zero_page_grow_succeeds_at_a_cap_the_size_has_not_passed() {
    for cap in [4, 9] {
        let (mut store, instance, g, sz) = memory_instance(EngineKind::Interp);
        assert_eq!(grow(&mut store, &g, 3), 1);
        instance.set_memory_pages_limit(&mut store, cap);
        assert_eq!(
            grow(&mut store, &g, 0),
            size(&mut store, &sz),
            "a zero-page grow reports the size under cap {cap}"
        );
    }
}

// Unlike fuel, whose unit differs per engine, the cap is a page count and both
// engines answer identically.
#[test]
fn both_engines_enforce_the_cap_the_same_way() {
    let mut outcomes = Vec::new();
    for kind in [EngineKind::Interp, EngineKind::Jit] {
        let (mut store, instance, g, sz) = memory_instance(kind);
        instance.set_memory_pages_limit(&mut store, 3);
        outcomes.push((
            grow(&mut store, &g, 2),
            grow(&mut store, &g, 1),
            size(&mut store, &sz),
        ));
    }
    assert_eq!(outcomes[0], outcomes[1], "interp and JIT disagree");
    assert_eq!(outcomes[0], (1, GROW_FAILED, 3));
}

// (module (memory 1) (func $s (drop (memory.grow (i32.const 100)))) (start $s)
//   (func (export "size") (result i32) (memory.size))
//   (func (export "g") (param i32) (result i32) (memory.grow (local.get 0))))
const START_GROWS: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x0d, 0x03, 0x60, 0x00, 0x00, 0x60, 0x00,
    0x01, 0x7f, 0x60, 0x01, 0x7f, 0x01, 0x7f, 0x03, 0x04, 0x03, 0x00, 0x01, 0x02, 0x05, 0x03, 0x01,
    0x00, 0x01, 0x07, 0x0c, 0x02, 0x04, 0x73, 0x69, 0x7a, 0x65, 0x00, 0x01, 0x01, 0x67, 0x00, 0x02,
    0x08, 0x01, 0x00, 0x0a, 0x16, 0x03, 0x08, 0x00, 0x41, 0xe4, 0x00, 0x40, 0x00, 0x1a, 0x0b, 0x04,
    0x00, 0x3f, 0x00, 0x0b, 0x06, 0x00, 0x20, 0x00, 0x40, 0x00, 0x0b,
];

// A start function runs inside `Instance::new`, so it grows before there is an
// instance to cap, and the cap does not reclaim what it took. The C ABI takes
// no limits on instantiation, so this is the shape of the hole rather than a
// bug to fix here — #41 tracks it, blocked on zwasm/zwasm#465. Pinned so the
// docs that describe it stay true, and so it breaks if a pre-start entry point
// ever arrives.
#[test]
fn a_start_function_grows_before_any_cap_can_exist() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, START_GROWS).unwrap();
    let instance = Instance::new_with_engine(&mut store, &module, &[], EngineKind::Interp).unwrap();
    let g = instance.get_func(&mut store, "g").unwrap();
    let sz = instance.get_func(&mut store, "size").unwrap();

    assert_eq!(
        size(&mut store, &sz),
        101,
        "the start function already grew"
    );

    instance.set_memory_pages_limit(&mut store, 10);
    assert_eq!(size(&mut store, &sz), 101, "a later cap reclaims nothing");
    assert_eq!(
        grow(&mut store, &g, 1),
        GROW_FAILED,
        "it only stops what is next"
    );
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn setting_a_cap_with_a_foreign_store_panics() {
    let (_store_a, instance, _g, _sz) = memory_instance(EngineKind::Interp);
    let engine = Engine::new().unwrap();
    let mut store_b = Store::new(&engine).unwrap();
    instance.set_memory_pages_limit(&mut store_b, 1);
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn clearing_a_cap_with_a_foreign_store_panics() {
    let (_store_a, instance, _g, _sz) = memory_instance(EngineKind::Interp);
    let engine = Engine::new().unwrap();
    let mut store_b = Store::new(&engine).unwrap();
    instance.clear_memory_pages_limit(&mut store_b);
}
