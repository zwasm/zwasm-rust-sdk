//! What a module asks to be given.
//!
//! `Instance::new` takes imports by position, which is workable only for a
//! caller who wrote the module. These read the import section so a caller who
//! did not can order its own functions by name.

use zwasm_sdk::{Engine, Error, ExternKind, Func, Instance, Module, Store, Val, ValType};

// (module (import "env" "h" (func)))
const ONE_FUNC: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x02, 0x09,
    0x01, 0x03, 0x65, 0x6e, 0x76, 0x01, 0x68, 0x00, 0x00,
];

// (module (func (export "f")))
const NO_IMPORTS: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03, 0x02,
    0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x04, 0x01, 0x02, 0x00, 0x0b,
];

// (module (import "a" "one" (func)) (import "b" "mem" (memory 1))
//         (import "c" "glob" (global i32)) (import "d" "tbl" (table 1 funcref))
//         (import "" "empty" (func)))
const EVERY_KIND: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x02, 0x2f,
    0x05, 0x01, 0x61, 0x03, 0x6f, 0x6e, 0x65, 0x00, 0x00, 0x01, 0x62, 0x03, 0x6d, 0x65, 0x6d, 0x02,
    0x00, 0x01, 0x01, 0x63, 0x04, 0x67, 0x6c, 0x6f, 0x62, 0x03, 0x7f, 0x00, 0x01, 0x64, 0x03, 0x74,
    0x62, 0x6c, 0x01, 0x70, 0x00, 0x01, 0x00, 0x05, 0x65, 0x6d, 0x70, 0x74, 0x79, 0x00, 0x00,
];

// (module (import "hosts" "second" (func $second (result i32)))
//         (import "hosts" "first"  (func $first  (result i32)))
//         (func (export "call_first")  (result i32) (call $first))
//         (func (export "call_second") (result i32) (call $second)))
//
// Declared second-then-first on purpose: a caller that pairs its own functions
// with the slots in its own order gets them backwards.
const TWO_FUNCS: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x02,
    0x1e, 0x02, 0x05, 0x68, 0x6f, 0x73, 0x74, 0x73, 0x06, 0x73, 0x65, 0x63, 0x6f, 0x6e, 0x64, 0x00,
    0x00, 0x05, 0x68, 0x6f, 0x73, 0x74, 0x73, 0x05, 0x66, 0x69, 0x72, 0x73, 0x74, 0x00, 0x00, 0x03,
    0x03, 0x02, 0x00, 0x00, 0x07, 0x1c, 0x02, 0x0a, 0x63, 0x61, 0x6c, 0x6c, 0x5f, 0x66, 0x69, 0x72,
    0x73, 0x74, 0x00, 0x02, 0x0b, 0x63, 0x61, 0x6c, 0x6c, 0x5f, 0x73, 0x65, 0x63, 0x6f, 0x6e, 0x64,
    0x00, 0x03, 0x0a, 0x0b, 0x02, 0x04, 0x00, 0x10, 0x01, 0x0b, 0x04, 0x00, 0x10, 0x00, 0x0b,
];

fn compile(store: &mut Store, wasm: &[u8]) -> Module {
    Module::new(store, wasm).unwrap()
}

#[test]
fn a_func_import_reports_its_module_field_and_kind() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = compile(&mut store, ONE_FUNC);

    let imports = module.imports(&store).unwrap();
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].module(), "env");
    assert_eq!(imports[0].name(), "h");
    assert_eq!(imports[0].kind(), ExternKind::Func);
}

#[test]
fn a_module_that_imports_nothing_reports_nothing() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = compile(&mut store, NO_IMPORTS);

    assert!(module.imports(&store).unwrap().is_empty());
}

// Order is the contract: `Instance::new` takes imports by position, so a list
// that reported them in any other order would be worse than none.
//
// The empty module name at the end is not decoration: `(import "" "empty" ...)`
// is valid wasm, and an empty name arrives from C as {size: 0, data: null},
// which is the case `name_bytes` exists for.
#[test]
fn every_kind_is_reported_in_declaration_order() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = compile(&mut store, EVERY_KIND);

    let imports = module.imports(&store).unwrap();
    let seen: Vec<(&str, &str, ExternKind)> = imports
        .iter()
        .map(|i| (i.module(), i.name(), i.kind()))
        .collect();

    assert_eq!(
        seen,
        [
            ("a", "one", ExternKind::Func),
            ("b", "mem", ExternKind::Memory),
            ("c", "glob", ExternKind::Global),
            ("d", "tbl", ExternKind::Table),
            ("", "empty", ExternKind::Func),
        ]
    );
}

// (module (import "env" "before" (func))
//         (import "env" "error" (tag (param i32)))
//         (import "env" "after" (func)))
const TAG_BETWEEN_FUNCS: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x60, 0x00, 0x00, 0x60, 0x01,
    0x7f, 0x00, 0x02, 0x27, 0x03, 0x03, 0x65, 0x6e, 0x76, 0x06, 0x62, 0x65, 0x66, 0x6f, 0x72, 0x65,
    0x00, 0x00, 0x03, 0x65, 0x6e, 0x76, 0x05, 0x65, 0x72, 0x72, 0x6f, 0x72, 0x04, 0x00, 0x01, 0x03,
    0x65, 0x6e, 0x76, 0x05, 0x61, 0x66, 0x74, 0x65, 0x72, 0x00, 0x00,
];

// zwasm 2.7.0 dropped a tag import from the list — base `wasm.h` had no tag
// type in its introspection — so the function after it was reported one index
// early, and the C call's lack of a status hid the shift. 2.8.0 reports it at
// its position (zwasm/zwasm#475). Pinned because `imports`'s doc claims
// positional fidelity, and because a positional caller binds every later
// import to the wrong declaration if this ever regresses.
#[test]
fn a_tag_import_is_reported_at_its_position() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = compile(&mut store, TAG_BETWEEN_FUNCS);

    let imports = module.imports(&store).unwrap();
    let seen: Vec<_> = imports
        .iter()
        .map(|i| (i.module(), i.name(), i.kind()))
        .collect();
    assert_eq!(
        seen,
        [
            ("env", "before", ExternKind::Func),
            ("env", "error", ExternKind::Tag),
            ("env", "after", ExternKind::Func),
        ]
    );
}

// (module (import "env" "m" (memory i64 4294967296)))
//
// A memory64 minimum one past u32. Valid: memory64 allows up to 2^48 pages.
const MEM64_MIN_PAST_U32: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x02, 0x0e, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x01,
    0x6d, 0x02, 0x04, 0x80, 0x80, 0x80, 0x80, 0x10,
];

// (module (import "env" "m" (memory i64 4294967295)))
const MEM64_MIN_AT_U32: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x02, 0x0e, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x01,
    0x6d, 0x02, 0x04, 0xff, 0xff, 0xff, 0xff, 0x0f,
];

// The wasm-c-api's `wasm_limits_t` is u32, so zwasm cannot build an import
// type for this memory and `zwasm_module_imports_ex` answers `false`
// (zwasm/zwasm#670). Through 0.4.0, which used the stock call, the same module
// came back as an empty list — a module that imports nothing. This is the one
// known way to reach the failure without exhausting memory; if zwasm/zwasm#670
// is fixed by saturating the limit, as its table path does, this route closes
// and the test will need another.
#[test]
fn a_memory64_import_past_u32_is_an_error_not_an_empty_list() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = compile(&mut store, MEM64_MIN_PAST_U32);

    let err = module.imports(&store).unwrap_err();
    assert!(matches!(err, Error::Message(_)), "{err:?}");
    assert_eq!(err.to_string(), "failed to list the module's imports");
}

// The boundary: one page less and the import type fits, so the list is
// complete. Pins that the failure above is the u32 limit, not memory64 itself.
#[test]
fn a_memory64_import_at_u32_is_listed() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = compile(&mut store, MEM64_MIN_AT_U32);

    let imports = module.imports(&store).unwrap();
    let seen: Vec<_> = imports
        .iter()
        .map(|i| (i.module(), i.name(), i.kind()))
        .collect();
    assert_eq!(seen, [("env", "m", ExternKind::Memory)]);
}

/// A `() -> i32` host function returning `n`.
fn constant_host_func(store: &mut Store, n: i32) -> Func {
    Func::new(store, &[], &[ValType::I32], move |_, results| {
        results[0] = Val::I32(n);
        Ok(())
    })
    .unwrap()
}

// The issue's second acceptance criterion, and the reason this exists: order a
// `Vec<Func>` from the import list without having seen the module's source.
//
// The module declares `second` before `first`. A caller pairing its own
// functions in its own order would bind them backwards and every assertion
// below would come out swapped — which is what makes this a test of the
// ordering rather than of instantiation.
#[test]
fn imports_can_order_the_funcs_instance_new_expects() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = compile(&mut store, TWO_FUNCS);

    // What the host has, keyed the way a host thinks: by name.
    let first = constant_host_func(&mut store, 1);
    let second = constant_host_func(&mut store, 2);

    // What the module wants, in the order it wants it.
    let ordered: Vec<Func> = module
        .imports(&store)
        .unwrap()
        .iter()
        .map(|import| {
            assert_eq!(import.module(), "hosts");
            match import.name() {
                "first" => first,
                "second" => second,
                other => panic!("no host function for {other}"),
            }
        })
        .collect();

    let instance = Instance::new(&mut store, &module, &ordered).unwrap();
    for (export, expected) in [("call_first", 1), ("call_second", 2)] {
        let f = instance.get_func(&mut store, export).unwrap();
        let mut results = vec![Val::I32(0); f.result_arity(&store)];
        f.call(&mut store, &[], &mut results).unwrap();
        assert_eq!(results, [Val::I32(expected)], "{export}");
    }
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn imports_with_a_foreign_store_panics() {
    let engine = Engine::new().unwrap();
    let mut store_a = Store::new(&engine).unwrap();
    let module = compile(&mut store_a, ONE_FUNC);
    let store_b = Store::new(&engine).unwrap();
    let _ = module.imports(&store_b);
}
