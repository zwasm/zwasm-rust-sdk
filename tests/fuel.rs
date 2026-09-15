//! A fuel budget is what makes `TrapKind::OutOfFuel` reachable: without one the
//! kind is public but nothing in this crate can produce it.
//!
//! The budget is per instance, and a unit means something different on each
//! engine, so the tests that care about a count pin the engine.

use zwasm_sdk::{Engine, EngineKind, Error, Instance, Module, Store, TrapKind, Val};

// (module (func (export "f") (param $n i32) (result i32) (local $i i32)
//   (block $done (loop $l
//     (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
//     (local.set $i (i32.add (local.get $i) (i32.const 1)))
//     (br $l)))
//   (local.get $i)))
const LOOP: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
    0x03, 0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x1e, 0x01, 0x1c, 0x01,
    0x01, 0x7f, 0x02, 0x40, 0x03, 0x40, 0x20, 0x01, 0x20, 0x00, 0x4f, 0x0d, 0x01, 0x20, 0x01, 0x41,
    0x01, 0x6a, 0x21, 0x01, 0x0c, 0x00, 0x0b, 0x0b, 0x20, 0x01, 0x0b,
];

/// A store holding one instance of `LOOP` on `kind`, plus that instance.
fn loop_instance(kind: EngineKind) -> (Store, Instance) {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, LOOP).unwrap();
    let instance = Instance::new_with_engine(&mut store, &module, &[], kind).unwrap();
    (store, instance)
}

/// Runs the loop `n` times.
fn run(store: &mut Store, instance: &Instance, n: i32) -> Result<(), Error> {
    let f = instance.get_func(store, "f").unwrap();
    let mut results = vec![Val::I32(0); f.result_arity(store)];
    f.call(store, &[Val::I32(n)], &mut results)
}

// The issue's first acceptance criterion: a guest that would finish runs out
// instead, and says so with a kind rather than only a message.
#[test]
fn a_guest_past_its_budget_traps_with_out_of_fuel() {
    let (mut store, instance) = loop_instance(EngineKind::Interp);
    instance.set_fuel(&mut store, 1_000);

    let err = run(&mut store, &instance, 100_000).expect_err("the budget cannot cover this");
    assert_eq!(err.trap_kind(), Some(TrapKind::OutOfFuel));
}

// The issue's second acceptance criterion.
#[test]
fn remaining_is_none_until_a_budget_is_armed() {
    let (mut store, instance) = loop_instance(EngineKind::Interp);
    assert_eq!(instance.fuel_remaining(&store), None);

    instance.set_fuel(&mut store, 1_234);
    assert_eq!(instance.fuel_remaining(&store), Some(1_234));
}

// `None` is "unmetered", not "nothing left" — an exhausted instance is still
// metered, and reports the zero it reached.
#[test]
fn an_exhausted_budget_reads_zero_rather_than_none() {
    let (mut store, instance) = loop_instance(EngineKind::Interp);
    instance.set_fuel(&mut store, 1_000);
    run(&mut store, &instance, 100_000).unwrap_err();

    assert_eq!(instance.fuel_remaining(&store), Some(0));
}

// `set_fuel` re-arms rather than adds, so an instance that ran out is not spent.
#[test]
fn re_arming_lets_an_exhausted_instance_run_again() {
    let (mut store, instance) = loop_instance(EngineKind::Interp);
    instance.set_fuel(&mut store, 1_000);
    run(&mut store, &instance, 100_000).unwrap_err();

    instance.set_fuel(&mut store, 10_000_000);
    run(&mut store, &instance, 100_000).expect("a re-armed budget covers the same call");
}

#[test]
fn disabling_the_budget_unmeters_the_instance() {
    let (mut store, instance) = loop_instance(EngineKind::Interp);
    instance.set_fuel(&mut store, 1_000);
    run(&mut store, &instance, 100_000).unwrap_err();

    instance.disable_fuel(&mut store);
    assert_eq!(instance.fuel_remaining(&store), None);
    run(&mut store, &instance, 100_000).expect("an unmetered guest runs to completion");
}

/// What one call of `n` iterations costs on `kind`.
fn cost(kind: EngineKind, n: i32) -> u64 {
    let (mut store, instance) = loop_instance(kind);
    let budget = 10_000_000;
    instance.set_fuel(&mut store, budget);
    run(&mut store, &instance, n).expect("the budget covers this");
    budget - instance.fuel_remaining(&store).expect("still metered")
}

// `zwasm.h` says a unit is engine-specific: the interpreter counts instructions
// executed, the JIT counts poll-site crossings — function entry plus loop
// back-edges. The JIT half is structural, so it is asserted exactly; the
// interpreter half is however many instructions the loop body happens to be, so
// only the inequality is. Together they are why a budget means nothing portable
// unless the engine is pinned, which is what `set_fuel`'s doc claims.
#[test]
fn a_unit_means_something_different_on_each_engine() {
    let n = 1_000;
    let jit = cost(EngineKind::Jit, n);
    let interp = cost(EngineKind::Interp, n);

    assert_eq!(
        jit,
        n as u64 + 1,
        "the JIT should charge one function entry plus one per back-edge"
    );
    assert!(
        interp > jit,
        "counting instructions should cost more than counting poll sites: \
         interp={interp}, jit={jit}"
    );
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn set_fuel_with_a_foreign_store_panics() {
    let (_store_a, instance) = loop_instance(EngineKind::Interp);
    let engine = Engine::new().unwrap();
    let mut store_b = Store::new(&engine).unwrap();
    instance.set_fuel(&mut store_b, 1);
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn disable_fuel_with_a_foreign_store_panics() {
    let (_store_a, instance) = loop_instance(EngineKind::Interp);
    let engine = Engine::new().unwrap();
    let mut store_b = Store::new(&engine).unwrap();
    instance.disable_fuel(&mut store_b);
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn fuel_remaining_with_a_foreign_store_panics() {
    let (_store_a, instance) = loop_instance(EngineKind::Interp);
    let engine = Engine::new().unwrap();
    let store_b = Store::new(&engine).unwrap();
    let _ = instance.fuel_remaining(&store_b);
}
