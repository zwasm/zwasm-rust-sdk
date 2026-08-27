//! Ownership tests for the store-context model: the store owns every object
//! created through it, handles are copies that only work with their own store,
//! and the store's drop frees everything in a child-before-parent order that
//! zwasm's back-pointer walking deallocation requires.
//!
//! Handles outliving their store is a compile error under this model (the
//! `compile_fail` doctest on `Store` pins that), so the use-after-free class
//! from issue #5 has no runtime test here — it cannot be written.

use zwasm_sdk::engine::Engine;
use zwasm_sdk::func::Func;
use zwasm_sdk::global::Global;
use zwasm_sdk::instance::Instance;
use zwasm_sdk::memory::Memory;
use zwasm_sdk::module::Module;
use zwasm_sdk::store::Store;
use zwasm_sdk::table::Table;
use zwasm_sdk::val::Val;

// (func (export "f") (result i32) (i32.const 42))
const RETURN42_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
    0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x06, 0x01, 0x04, 0x00, 0x41,
    0x2a, 0x0b,
];

// (module
//   (import "env" "h" (func (param i32) (result i32)))
//   (func (export "f") (param i32) (result i32) (local.get 0) (call 0)))
const CALLBACK_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
    0x02, 0x09, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x01, 0x68, 0x00, 0x00, 0x03, 0x02, 0x01, 0x00, 0x07,
    0x05, 0x01, 0x01, 0x66, 0x00, 0x01, 0x0a, 0x08, 0x01, 0x06, 0x00, 0x20, 0x00, 0x10, 0x00, 0x0b,
];

unsafe extern "C" fn add_one(
    args: *const zwasm_sys::wasm_val_vec_t,
    results: *mut zwasm_sys::wasm_val_vec_t,
) -> *mut zwasm_sys::wasm_trap_t {
    let arg = (*args).data;
    let res = (*results).data;
    (*res).kind = zwasm_sys::wasm_valkind_enum_WASM_I32 as u8;
    (*res).of.i32_ = (*arg).of.i32_ + 1;
    std::ptr::null_mut()
}

fn new_add_one_host_func(store: &mut Store) -> Func {
    let mut params = zwasm_sys::wasm_valtype_vec_t {
        size: 0,
        data: std::ptr::null_mut(),
    };
    let mut results = zwasm_sys::wasm_valtype_vec_t {
        size: 0,
        data: std::ptr::null_mut(),
    };
    let param_type =
        unsafe { zwasm_sys::wasm_valtype_new(zwasm_sys::wasm_valkind_enum_WASM_I32 as u8) };
    let result_type =
        unsafe { zwasm_sys::wasm_valtype_new(zwasm_sys::wasm_valkind_enum_WASM_I32 as u8) };
    unsafe {
        zwasm_sys::wasm_valtype_vec_new(&mut params, 1, &param_type);
        zwasm_sys::wasm_valtype_vec_new(&mut results, 1, &result_type);
    };
    let functype = unsafe { zwasm_sys::wasm_functype_new(&mut params, &mut results) };
    let host_fn = unsafe { Func::new_host(store, functype, Some(add_one)) }.unwrap();
    unsafe { zwasm_sys::wasm_functype_delete(functype) };
    host_fn
}

// One store owning every entity kind; the drop frees funcs before instances,
// instances before modules, and everything before the store, on one thread.
#[test]
fn store_drop_frees_everything() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let host_fn = new_add_one_host_func(&mut store);
    let module = Module::new(&mut store, CALLBACK_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[host_fn]).unwrap();
    let f = instance.get_func(&mut store, "f").unwrap();

    let mut results = [Val::I32(0)];
    f.call(&mut store, &[Val::I32(41)], &mut results).unwrap();
    assert_eq!(results, [Val::I32(42)]);

    let _memory = Memory::new(&mut store, 1, Some(2)).unwrap();
    let _global = Global::new(&mut store, Val::I64(7), true).unwrap();
    let _table = Table::new(&mut store, 1, Some(4)).unwrap();

    drop(store);
}

fn assert_send_sync<T: Send + Sync>() {}

// The engine holds no per-store state, so unlike everything else here it
// crosses threads. Nothing else in the suite would notice if that stopped
// being true, because the store-derived types are all !Send by construction.
#[test]
fn engine_is_send_and_sync() {
    assert_send_sync::<Engine>();
}

// The engine is kept alive by the store's own clone, so the caller's Engine
// values can all be dropped while stores still use it.
#[test]
fn engine_clones_share_one_engine() {
    let engine = Engine::new().unwrap();
    let clone = engine.clone();
    drop(engine);

    let mut store_a = Store::new(&clone).unwrap();
    let mut store_b = Store::new(&clone).unwrap();
    drop(clone);

    let module_a = Module::new(&mut store_a, RETURN42_WASM).unwrap();
    let module_b = Module::new(&mut store_b, RETURN42_WASM).unwrap();
    let instance_a = Instance::new(&mut store_a, &module_a, &[]).unwrap();
    let instance_b = Instance::new(&mut store_b, &module_b, &[]).unwrap();

    let mut results = [Val::I32(0)];
    let fa = instance_a.get_func(&mut store_a, "f").unwrap();
    fa.call(&mut store_a, &[], &mut results).unwrap();
    assert_eq!(results, [Val::I32(42)]);
    let fb = instance_b.get_func(&mut store_b, "f").unwrap();
    fb.call(&mut store_b, &[], &mut results).unwrap();
    assert_eq!(results, [Val::I32(42)]);
}

// Handles are plain copies naming one object: two copies of a Func reach the
// same function, and copies of a Memory observe each other's growth.
#[test]
fn handle_copies_name_one_object() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, RETURN42_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    let f = instance.get_func(&mut store, "f").unwrap();
    let f2 = f;
    let mut results = [Val::I32(0)];
    f2.call(&mut store, &[], &mut results).unwrap();
    f.call(&mut store, &[], &mut results).unwrap();
    assert_eq!(results, [Val::I32(42)]);

    let memory = Memory::new(&mut store, 1, Some(4)).unwrap();
    let memory2 = memory;
    assert_eq!(memory.grow(&mut store, 1).unwrap(), 1);
    assert_eq!(memory2.size(&store), 2);
}

// Mixing handles from two stores would mix the stores' C-side state, so it
// panics, mirroring wasmtime.
#[test]
#[should_panic(expected = "store it does not belong to")]
fn instantiating_a_foreign_module_panics() {
    let engine = Engine::new().unwrap();
    let mut store_a = Store::new(&engine).unwrap();
    let mut store_b = Store::new(&engine).unwrap();

    let module = Module::new(&mut store_a, RETURN42_WASM).unwrap();
    let _ = Instance::new(&mut store_b, &module, &[]);
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn instantiating_with_a_foreign_import_panics() {
    let engine = Engine::new().unwrap();
    let mut store_a = Store::new(&engine).unwrap();
    let mut store_b = Store::new(&engine).unwrap();

    let module = Module::new(&mut store_a, CALLBACK_WASM).unwrap();
    let host_fn = new_add_one_host_func(&mut store_b);
    let _ = Instance::new(&mut store_a, &module, &[host_fn]);
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn calling_with_a_foreign_store_panics() {
    let engine = Engine::new().unwrap();
    let mut store_a = Store::new(&engine).unwrap();
    let mut store_b = Store::new(&engine).unwrap();

    let module = Module::new(&mut store_a, RETURN42_WASM).unwrap();
    let instance = Instance::new(&mut store_a, &module, &[]).unwrap();
    let f = instance.get_func(&mut store_a, "f").unwrap();

    let mut results = [Val::I32(0)];
    let _ = f.call(&mut store_b, &[], &mut results);
}

// Global::set reports immutability and type mismatches as errors, which the C
// API rejects silently; the checks come from the global's own type.
#[test]
fn global_set_checks_mutability_and_type() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let immutable = Global::new(&mut store, Val::I32(1), false).unwrap();
    let err = immutable.set(&mut store, Val::I32(2)).err().unwrap();
    assert!(err.to_string().contains("immutable"));

    let mutable = Global::new(&mut store, Val::I32(1), true).unwrap();
    let err = mutable.set(&mut store, Val::I64(2)).err().unwrap();
    assert!(err.to_string().contains("type"));

    mutable.set(&mut store, Val::I32(5)).unwrap();
    assert_eq!(mutable.get(&store), Val::I32(5));
}

#[test]
fn memory_grow_returns_previous_size() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let memory = Memory::new(&mut store, 1, Some(3)).unwrap();

    assert_eq!(memory.size(&store), 1);
    assert_eq!(memory.grow(&mut store, 2).unwrap(), 1);
    assert_eq!(memory.size(&store), 3);
    assert!(memory.grow(&mut store, 1).is_err());
}

#[test]
fn table_grow_returns_previous_size() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let table = Table::new(&mut store, 1, Some(3)).unwrap();

    assert_eq!(table.size(&store), 1);
    assert_eq!(table.grow(&mut store, 2).unwrap(), 1);
    assert_eq!(table.size(&store), 3);
    assert!(table.grow(&mut store, 1).is_err());
}

// zwasm's wasm_memory_data returns null when the backing is empty
// ("if (bytes.len == 0) return null", src/api/instance.zig), and a zero-page
// memory is legal. slice::from_raw_parts requires a non-null pointer even for
// a zero length, so the null has to be turned into an empty slice rather than
// passed through.
#[test]
fn zero_page_memory_yields_empty_slices() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let memory = Memory::new(&mut store, 0, None).unwrap();

    assert_eq!(memory.size(&store), 0);
    assert!(memory.data(&store).is_empty());
    assert!(memory.data_mut(&mut store).is_empty());
}

// A handle used with a store it does not belong to panics for every entity
// kind, not just the two the instantiation paths cover.
#[test]
#[should_panic(expected = "store it does not belong to")]
fn memory_with_a_foreign_store_panics() {
    let engine = Engine::new().unwrap();
    let mut store_a = Store::new(&engine).unwrap();
    let store_b = Store::new(&engine).unwrap();

    let memory = Memory::new(&mut store_a, 1, None).unwrap();
    let _ = memory.size(&store_b);
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn global_with_a_foreign_store_panics() {
    let engine = Engine::new().unwrap();
    let mut store_a = Store::new(&engine).unwrap();
    let store_b = Store::new(&engine).unwrap();

    let global = Global::new(&mut store_a, Val::I32(1), true).unwrap();
    let _ = global.get(&store_b);
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn table_with_a_foreign_store_panics() {
    let engine = Engine::new().unwrap();
    let mut store_a = Store::new(&engine).unwrap();
    let store_b = Store::new(&engine).unwrap();

    let table = Table::new(&mut store_a, 1, None).unwrap();
    let _ = table.size(&store_b);
}
