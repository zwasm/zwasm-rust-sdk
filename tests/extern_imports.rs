//! Imports that are not functions.
//!
//! `Instance::new` takes `&[Extern]`, so a module importing a memory, global
//! or table can be instantiated with one the host made or another instance
//! exported. zwasm binds those three on the interpreter only (measured on
//! 2.9.0; the C API gives no reason when it declines), so the engine each
//! test lands on is part of what it checks.

use zwasm_sdk::{
    Engine, EngineKind, Error, Extern, ExternKind, Func, Global, Instance, Memory, Module, Store,
    Table, Val, ValType,
};

// (module (import "env" "mem" (memory 1))
//         (func (export "peek") (result i32) (i32.load (i32.const 0))))
const MEM_IMPORT: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x02,
    0x0c, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x03, 0x6d, 0x65, 0x6d, 0x02, 0x00, 0x01, 0x03, 0x02, 0x01,
    0x00, 0x07, 0x08, 0x01, 0x04, 0x70, 0x65, 0x65, 0x6b, 0x00, 0x00, 0x0a, 0x09, 0x01, 0x07, 0x00,
    0x41, 0x00, 0x28, 0x02, 0x00, 0x0b,
];

// (module (import "env" "mem" (memory 2)))
const MEM_IMPORT_MIN2: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x02, 0x0c, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x03,
    0x6d, 0x65, 0x6d, 0x02, 0x00, 0x02,
];

// (module (import "env" "g" (global i32))
//         (func (export "get") (result i32) (global.get 0)))
const GLOBAL_IMPORT: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x02,
    0x0a, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x01, 0x67, 0x03, 0x7f, 0x00, 0x03, 0x02, 0x01, 0x00, 0x07,
    0x07, 0x01, 0x03, 0x67, 0x65, 0x74, 0x00, 0x00, 0x0a, 0x06, 0x01, 0x04, 0x00, 0x23, 0x00, 0x0b,
];

// (module (import "env" "t" (table 1 funcref))
//         (func (export "size") (result i32) (table.size 0)))
const TABLE_IMPORT: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x02,
    0x0b, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x01, 0x74, 0x01, 0x70, 0x00, 0x01, 0x03, 0x02, 0x01, 0x00,
    0x07, 0x08, 0x01, 0x04, 0x73, 0x69, 0x7a, 0x65, 0x00, 0x00, 0x0a, 0x07, 0x01, 0x05, 0x00, 0xfc,
    0x10, 0x00, 0x0b,
];

// (module (import "env" "f" (func (result i32)))
//         (func (export "call") (result i32) (call 0)))
const FUNC_IMPORT: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x02,
    0x09, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x01, 0x66, 0x00, 0x00, 0x03, 0x02, 0x01, 0x00, 0x07, 0x08,
    0x01, 0x04, 0x63, 0x61, 0x6c, 0x6c, 0x00, 0x01, 0x0a, 0x06, 0x01, 0x04, 0x00, 0x10, 0x00, 0x0b,
];

// (module (import "env" "f" (func)) (import "env" "g" (global i32))
//         (import "env" "t" (table 1 funcref)) (import "env" "mem" (memory 1)))
const ALL_FOUR: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x02, 0x27,
    0x04, 0x03, 0x65, 0x6e, 0x76, 0x01, 0x66, 0x00, 0x00, 0x03, 0x65, 0x6e, 0x76, 0x01, 0x67, 0x03,
    0x7f, 0x00, 0x03, 0x65, 0x6e, 0x76, 0x01, 0x74, 0x01, 0x70, 0x00, 0x01, 0x03, 0x65, 0x6e, 0x76,
    0x03, 0x6d, 0x65, 0x6d, 0x02, 0x00, 0x01,
];

// (module (memory (export "mem") 1))
const MEM_EXPORT: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x05, 0x03, 0x01, 0x00, 0x01, 0x07, 0x07, 0x01,
    0x03, 0x6d, 0x65, 0x6d, 0x02, 0x00,
];

// (module (table (export "t") 1 10 funcref))
const TABLE_EXPORT: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x04, 0x05, 0x01, 0x70, 0x01, 0x01, 0x0a, 0x07,
    0x05, 0x01, 0x01, 0x74, 0x01, 0x00,
];

fn new_store() -> (Engine, Store) {
    let engine = Engine::new().unwrap();
    let store = Store::new(&engine).unwrap();
    (engine, store)
}

/// Calls the nullary `i32` export `name` and returns what it produced.
fn call_i32(store: &mut Store, instance: &Instance, name: &str) -> i32 {
    let f = instance.get_func(store, name).unwrap();
    let mut results = [Val::I32(0)];
    f.call(store, &[], &mut results).unwrap();
    match results[0] {
        Val::I32(n) => n,
        other => panic!("expected an i32, got {other:?}"),
    }
}

fn write_i32(store: &mut Store, memory: &Memory, at: usize, value: i32) {
    memory.data_mut(store)[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn a_memory_the_host_made_is_what_the_guest_reads() {
    let (_engine, mut store) = new_store();
    let memory = Memory::new(&mut store, 1, None).unwrap();
    write_i32(&mut store, &memory, 0, 42);

    let module = Module::new(&mut store, MEM_IMPORT).unwrap();
    let instance = Instance::new(&mut store, &module, &[memory.into()]).unwrap();
    assert_eq!(call_i32(&mut store, &instance, "peek"), 42);
}

#[test]
fn an_imported_memory_stays_shared_after_instantiation() {
    let (_engine, mut store) = new_store();
    let memory = Memory::new(&mut store, 1, None).unwrap();
    let module = Module::new(&mut store, MEM_IMPORT).unwrap();
    let instance = Instance::new(&mut store, &module, &[memory.into()]).unwrap();

    write_i32(&mut store, &memory, 0, 7);
    assert_eq!(call_i32(&mut store, &instance, "peek"), 7);
    write_i32(&mut store, &memory, 0, -1);
    assert_eq!(call_i32(&mut store, &instance, "peek"), -1);
}

#[test]
fn a_func_still_goes_in_through_into() {
    let (_engine, mut store) = new_store();
    let host = Func::new(&mut store, &[], &[ValType::I32], |_, results| {
        results[0] = Val::I32(9);
        Ok(())
    })
    .unwrap();

    let module = Module::new(&mut store, FUNC_IMPORT).unwrap();
    let instance = Instance::new(&mut store, &module, &[host.into()]).unwrap();
    assert_eq!(call_i32(&mut store, &instance, "call"), 9);
}

#[test]
fn a_global_the_host_made_is_what_the_guest_reads() {
    let (_engine, mut store) = new_store();
    let global = Global::new(&mut store, Val::I32(5), false).unwrap();

    let module = Module::new(&mut store, GLOBAL_IMPORT).unwrap();
    let instance = Instance::new(&mut store, &module, &[global.into()]).unwrap();
    assert_eq!(call_i32(&mut store, &instance, "get"), 5);
}

#[test]
fn a_table_the_host_made_keeps_its_size() {
    let (_engine, mut store) = new_store();
    let table = Table::new(&mut store, 2, None).unwrap();

    let module = Module::new(&mut store, TABLE_IMPORT).unwrap();
    let instance = Instance::new(&mut store, &module, &[table.into()]).unwrap();
    assert_eq!(call_i32(&mut store, &instance, "size"), 2);
}

// zwasm 2.9.0: the JIT does not bind a host-made memory, global or table, and
// declines with no reason.
#[test]
fn the_jit_declines_a_memory_import() {
    let (_engine, mut store) = new_store();
    let memory = Memory::new(&mut store, 1, None).unwrap();
    let module = Module::new(&mut store, MEM_IMPORT).unwrap();

    let err = Instance::new_with_engine(&mut store, &module, &[memory.into()], EngineKind::Jit)
        .unwrap_err();
    match err {
        Error::Message(message) => assert!(message.contains("Jit"), "{message}"),
        other => panic!("expected Error::Message, got {other:?}"),
    }
}

#[test]
fn auto_runs_a_module_importing_a_memory_on_the_interpreter() {
    let (_engine, mut store) = new_store();
    let memory = Memory::new(&mut store, 1, None).unwrap();
    let module = Module::new(&mut store, MEM_IMPORT).unwrap();

    let instance =
        Instance::new_with_engine(&mut store, &module, &[memory.into()], EngineKind::Auto).unwrap();
    assert_eq!(instance.engine(&store), EngineKind::Interp);
}

#[test]
fn a_memory_another_instance_exported_is_shared_with_the_importer() {
    let (_engine, mut store) = new_store();
    let exporter = Module::new(&mut store, MEM_EXPORT).unwrap();
    let exporter =
        Instance::new_with_engine(&mut store, &exporter, &[], EngineKind::Interp).unwrap();
    let memory = exporter.get_memory(&mut store, "mem").unwrap();
    write_i32(&mut store, &memory, 0, 11);

    let importer = Module::new(&mut store, MEM_IMPORT).unwrap();
    let importer = Instance::new(&mut store, &importer, &[memory.into()]).unwrap();
    assert_eq!(call_i32(&mut store, &importer, "peek"), 11);
}

// zwasm gives no reason for any of these (zwasm/zwasm#353), so each is the
// same `Error::Message` the engine decline above is.
#[test]
fn a_mismatched_import_list_is_an_error_not_a_panic() {
    let (_engine, mut store) = new_store();
    let memory = Memory::new(&mut store, 1, None).unwrap();
    let host = Func::new(&mut store, &[], &[], |_, _| Ok(())).unwrap();

    let module = Module::new(&mut store, MEM_IMPORT).unwrap();
    let wrong_kind = Instance::new(&mut store, &module, &[host.into()]).unwrap_err();
    let too_few = Instance::new(&mut store, &module, &[]).unwrap_err();
    let module = Module::new(&mut store, MEM_IMPORT_MIN2).unwrap();
    let too_small = Instance::new(&mut store, &module, &[memory.into()]).unwrap_err();

    for err in [wrong_kind, too_few, too_small] {
        assert!(matches!(err, Error::Message(_)), "{err:?}");
    }
}

#[test]
#[should_panic]
fn a_memory_from_another_store_panics_as_a_func_does() {
    let (engine, mut store_a) = new_store();
    let mut store_b = Store::new(&engine).unwrap();
    let memory = Memory::new(&mut store_b, 1, None).unwrap();

    let module = Module::new(&mut store_a, MEM_IMPORT).unwrap();
    let _ = Instance::new(&mut store_a, &module, &[memory.into()]);
}

#[test]
fn extern_kind_matches_what_the_module_asks_for() {
    let (_engine, mut store) = new_store();
    let module = Module::new(&mut store, ALL_FOUR).unwrap();

    let host = Func::new(&mut store, &[], &[], |_, _| Ok(())).unwrap();
    let externs: [Extern; 4] = [
        host.into(),
        Global::new(&mut store, Val::I32(0), false).unwrap().into(),
        Table::new(&mut store, 1, None).unwrap().into(),
        Memory::new(&mut store, 1, None).unwrap().into(),
    ];

    let wanted: Vec<ExternKind> = module
        .imports(&store)
        .unwrap()
        .iter()
        .map(|import| import.kind())
        .collect();
    let given: Vec<ExternKind> = externs.iter().map(Extern::kind).collect();
    assert_eq!(given, wanted);
    assert!(externs[0].into_func().is_some());
    assert!(externs[3].into_memory().is_some() && externs[3].into_func().is_none());

    Instance::new(&mut store, &module, &externs).unwrap();
}

// zwasm binds a table import by value (zwasm/zwasm#449). Its stop-gap for a
// host-made table is to refuse every later grow, on both sides.
#[test]
fn a_host_table_stops_growing_once_imported() {
    let (_engine, mut store) = new_store();
    let table = Table::new(&mut store, 1, Some(10)).unwrap();
    assert_eq!(table.grow(&mut store, 1).unwrap(), 1);

    let module = Module::new(&mut store, TABLE_IMPORT).unwrap();
    let instance = Instance::new(&mut store, &module, &[table.into()]).unwrap();
    assert_eq!(call_i32(&mut store, &instance, "size"), 2);

    assert!(table.grow(&mut store, 1).is_err());
    assert_eq!(table.size(&store), 2);
}

// The same defect with no stop-gap: a table another instance exported, once
// imported, would leave one side on freed memory after a grow. This crate
// refuses it before zwasm sees it.
#[test]
fn a_table_another_instance_exported_is_refused_as_an_import() {
    let (_engine, mut store) = new_store();
    let exporter = Module::new(&mut store, TABLE_EXPORT).unwrap();
    let exporter =
        Instance::new_with_engine(&mut store, &exporter, &[], EngineKind::Interp).unwrap();
    let table = exporter.get_table(&mut store, "t").unwrap();

    let importer = Module::new(&mut store, TABLE_IMPORT).unwrap();
    let err = Instance::new(&mut store, &importer, &[table.into()]).unwrap_err();
    match err {
        Error::Message(message) => assert!(message.contains("zwasm/zwasm#449"), "{message}"),
        other => panic!("expected Error::Message, got {other:?}"),
    }
}

// The store check covers every slot before any recoverable refusal, so a
// cross-store import still panics when an exported table precedes it.
#[test]
#[should_panic]
fn a_cross_store_import_after_an_exported_table_still_panics() {
    let (engine, mut store_a) = new_store();
    let mut store_b = Store::new(&engine).unwrap();
    let exporter = Module::new(&mut store_a, TABLE_EXPORT).unwrap();
    let exporter =
        Instance::new_with_engine(&mut store_a, &exporter, &[], EngineKind::Interp).unwrap();
    let table = exporter.get_table(&mut store_a, "t").unwrap();
    let foreign = Memory::new(&mut store_b, 1, None).unwrap();

    let module = Module::new(&mut store_a, ALL_FOUR).unwrap();
    let _ = Instance::new(&mut store_a, &module, &[table.into(), foreign.into()]);
}
