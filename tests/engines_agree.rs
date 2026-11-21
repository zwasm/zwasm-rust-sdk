//! The two engines have to agree on what a module computes.
//!
//! One shape is kept here, the one zwasm 2.12.1 was released for: an `if`
//! whose then arm ends in `unreachable`, so only the else arm reaches the
//! `end`. On 2.12.0 the JIT took the `if`'s result from the value below it and
//! returned 0, with no trap (GHSA-grpg-98rp-93g7). zwasm tests the shapes
//! around this one; what this checks is that the zwasm bundled here has the fix.

use zwasm_sdk::{Engine, EngineKind, Instance, Module, Store, Val};

// (module
//   (func (export "else_arm") (param i32) (result i32)
//     i32.const 5
//     local.get 0
//     if (result i32)
//       unreachable
//     else
//       i32.const 7
//     end
//     i32.sub))
const WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
    0x03, 0x02, 0x01, 0x00, 0x07, 0x0c, 0x01, 0x08, 0x65, 0x6c, 0x73, 0x65, 0x5f, 0x61, 0x72, 0x6d,
    0x00, 0x00, 0x0a, 0x10, 0x01, 0x0e, 0x00, 0x41, 0x05, 0x20, 0x00, 0x04, 0x7f, 0x00, 0x05, 0x41,
    0x07, 0x0b, 0x6b, 0x0b,
];

/// Calls `else_arm(arg)` on `kind`, having checked that `kind` is what ran.
fn else_arm(kind: EngineKind, arg: i32) -> i32 {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, WASM).unwrap();
    let instance = Instance::new_with_engine(&mut store, &module, &[], kind).unwrap();
    assert_eq!(instance.engine(&store), kind);
    let func = instance.get_func(&mut store, "else_arm").unwrap();
    let mut results = [Val::I32(0)];
    func.call(&mut store, &[Val::I32(arg)], &mut results)
        .unwrap();
    match results[0] {
        Val::I32(v) => v,
        other => panic!("else_arm returned {other:?}"),
    }
}

#[test]
fn an_if_whose_then_arm_traps_takes_its_result_from_the_else_arm() {
    for kind in [EngineKind::Jit, EngineKind::Interp] {
        assert_eq!(else_arm(kind, 0), 5 - 7, "on {kind:?}");
    }
}
