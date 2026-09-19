//! What a module asks to be given.
//!
//! `Instance::new` takes imports by position, which is workable only for a
//! caller who wrote the module. These read the import section so a caller who
//! did not can order its own functions by name.

use zwasm_sdk::{Engine, ExternKind, Func, Instance, Module, Store, Val};

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

    let imports = module.imports(&store);
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

    assert!(module.imports(&store).is_empty());
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

    let imports = module.imports(&store);
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

// zwasm drops tag imports — base `wasm.h` has no tag type, so its introspection
// skips them (`src/api/module_introspect.zig:126`) — and the C call returns no
// status, so nothing here can tell a shortened list from a shorter module.
//
// Pinned rather than left to be discovered: `imports`'s doc claims exactly this
// shape, and zwasm/zwasm#475 asks for an entry point that is complete or
// fails. When that lands, this test fails and the doc it guards comes off with
// it.
#[test]
fn a_tag_import_is_dropped_and_shifts_what_follows() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = compile(&mut store, TAG_BETWEEN_FUNCS);

    let imports = module.imports(&store);
    assert_eq!(imports.len(), 2, "the module declares three");
    assert_eq!((imports[0].module(), imports[0].name()), ("env", "before"));

    // Declared at index 2, reported at index 1.
    assert_eq!((imports[1].module(), imports[1].name()), ("env", "after"));
}

unsafe extern "C" fn returns_one(
    _args: *const zwasm_sys::wasm_val_vec_t,
    results: *mut zwasm_sys::wasm_val_vec_t,
) -> *mut zwasm_sys::wasm_trap_t {
    let res = (*results).data;
    (*res).kind = zwasm_sys::wasm_valkind_enum_WASM_I32 as u8;
    (*res).of.i32_ = 1;
    std::ptr::null_mut()
}

unsafe extern "C" fn returns_two(
    _args: *const zwasm_sys::wasm_val_vec_t,
    results: *mut zwasm_sys::wasm_val_vec_t,
) -> *mut zwasm_sys::wasm_trap_t {
    let res = (*results).data;
    (*res).kind = zwasm_sys::wasm_valkind_enum_WASM_I32 as u8;
    (*res).of.i32_ = 2;
    std::ptr::null_mut()
}

/// A `() -> i32` host function. Built by hand because `Func::new_host` is the
/// only door today — #30 is the safe one.
fn host_func(store: &mut Store, callback: zwasm_sys::wasm_func_callback_t) -> Func {
    let mut params = zwasm_sys::wasm_valtype_vec_t {
        size: 0,
        data: std::ptr::null_mut(),
    };
    let mut results = zwasm_sys::wasm_valtype_vec_t {
        size: 0,
        data: std::ptr::null_mut(),
    };
    let result_type =
        unsafe { zwasm_sys::wasm_valtype_new(zwasm_sys::wasm_valkind_enum_WASM_I32 as u8) };
    unsafe {
        zwasm_sys::wasm_valtype_vec_new_empty(&mut params);
        zwasm_sys::wasm_valtype_vec_new(&mut results, 1, &result_type);
    };
    let functype = unsafe { zwasm_sys::wasm_functype_new(&mut params, &mut results) };
    let func = unsafe { Func::new_host(store, functype, callback) }.unwrap();
    unsafe { zwasm_sys::wasm_functype_delete(functype) };
    func
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
    let first = host_func(&mut store, Some(returns_one));
    let second = host_func(&mut store, Some(returns_two));

    // What the module wants, in the order it wants it.
    let ordered: Vec<Func> = module
        .imports(&store)
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
