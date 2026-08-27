use zwasm_sdk::engine::Engine;
use zwasm_sdk::error::Error;
use zwasm_sdk::func::Func;
use zwasm_sdk::instance::Instance;
use zwasm_sdk::module::Module;
use zwasm_sdk::store::Store;
use zwasm_sdk::val::Val;

// (func (export "f") (result i32) (i32.const 42))
const RETURN42_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
    0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x06, 0x01, 0x04, 0x00, 0x41,
    0x2a, 0x0b,
];

// (func (export "add") (param i32 i32) (result i32) (i32.add (local.get 0) (local.get 1)))
const ADD_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01,
    0x7f, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00, 0x0a, 0x09,
    0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
];

// Minimal valid wasm: magic + version only
const MINIMAL_WASM: &[u8] = &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

#[test]
fn test_engine_new() {
    let engine = Engine::new();
    assert!(engine.is_ok());
}

#[test]
fn test_engine_default() {
    let _engine = Engine::default();
}

#[test]
fn test_store_new() {
    let engine = Engine::new().unwrap();
    let store = Store::new(&engine);
    assert!(store.is_ok());
}

#[test]
fn test_module_new() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, RETURN42_WASM);
    assert!(module.is_ok());
}

#[test]
fn test_module_invalid_wasm() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, &[0x00, 0x00, 0x00, 0x00]);
    assert!(module.is_err());
}

#[test]
fn test_module_minimal() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, MINIMAL_WASM);
    assert!(module.is_ok());
}

#[test]
fn test_instance_new() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, RETURN42_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]);
    assert!(instance.is_ok());
}

#[test]
fn test_invoke_no_args() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, RETURN42_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let func = instance.get_func(&mut store, "f").unwrap();

    let mut results = [Val::I32(0)];
    func.call(&mut store, &[], &mut results).unwrap();
    assert_eq!(results, [Val::I32(42)]);
}

#[test]
fn test_invoke_with_args() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, ADD_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let func = instance.get_func(&mut store, "add").unwrap();

    let mut results = [Val::I32(0)];
    func.call(&mut store, &[Val::I32(10), Val::I32(32)], &mut results)
        .unwrap();
    assert_eq!(results, [Val::I32(42)]);
}

#[test]
fn test_invoke_add_zero() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, ADD_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let func = instance.get_func(&mut store, "add").unwrap();

    let mut results = [Val::I32(1)];
    func.call(&mut store, &[Val::I32(0), Val::I32(0)], &mut results)
        .unwrap();
    assert_eq!(results, [Val::I32(0)]);
}

#[test]
fn test_arities() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, ADD_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let func = instance.get_func(&mut store, "add").unwrap();

    assert_eq!(func.param_arity(&store), 2);
    assert_eq!(func.result_arity(&store), 1);
}

// The results slice has to be sized by the caller, so a wrong length is an
// error before anything runs, matching wasmtime's contract.
#[test]
fn test_call_wrong_results_len_is_an_error() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, RETURN42_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let func = instance.get_func(&mut store, "f").unwrap();

    let err = func.call(&mut store, &[], &mut []).err().unwrap();
    assert!(err.to_string().contains("results"));
}

#[test]
fn test_call_wrong_params_len_is_an_error() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, ADD_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let func = instance.get_func(&mut store, "add").unwrap();

    let mut results = [Val::I32(0)];
    let err = func
        .call(&mut store, &[Val::I32(1)], &mut results)
        .err()
        .unwrap();
    assert!(err.to_string().contains("parameters"));
}

#[test]
fn test_get_func_not_found() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, RETURN42_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    assert!(instance.get_func(&mut store, "nonexistent").is_none());
}

// (module (memory (export "m") 1))
const MEMORY_EXPORT_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x05, 0x03, 0x01, 0x00, 0x01, 0x07, 0x05, 0x01,
    0x01, 0x6d, 0x02, 0x00,
];

// The name resolves, but to a memory. wasm_extern_as_func yields null there and
// wasm_func_copy passes null through, so this has to be None rather than a
// crash, matching wasmtime's get_func.
#[test]
fn test_get_func_not_a_function() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, MEMORY_EXPORT_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    assert!(instance.get_func(&mut store, "m").is_none());
}

// (module
//   (func (export "a") (result i32) (i32.const 1))
//   (func (export "b") (result i32) (i32.const 2)))
const TWO_EXPORTS_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
    0x03, 0x02, 0x00, 0x00, 0x07, 0x09, 0x02, 0x01, 0x61, 0x00, 0x00, 0x01, 0x62, 0x00, 0x01, 0x0a,
    0x0b, 0x02, 0x04, 0x00, 0x41, 0x01, 0x0b, 0x04, 0x00, 0x41, 0x02, 0x0b,
];

// Each Func from get_func owns its handle, so two of them from one instance
// are independent. They used to borrow out of an exports vector that the Func
// kept alive instead, which is the arrangement wasm_func_copy replaced.
#[test]
fn test_two_funcs_stay_valid_together() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, TWO_EXPORTS_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    let a = instance.get_func(&mut store, "a").unwrap();
    let b = instance.get_func(&mut store, "b").unwrap();

    // Interleaved, so each handle is used after the other one was created.
    let mut results = [Val::I32(0)];
    a.call(&mut store, &[], &mut results).unwrap();
    assert_eq!(results, [Val::I32(1)]);
    b.call(&mut store, &[], &mut results).unwrap();
    assert_eq!(results, [Val::I32(2)]);
    a.call(&mut store, &[], &mut results).unwrap();
    assert_eq!(results, [Val::I32(1)]);
}

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
    // Create functype: (i32) -> (i32)
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

#[test]
fn test_host_function() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let host_fn = new_add_one_host_func(&mut store);

    let module = Module::new(&mut store, CALLBACK_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[host_fn]).unwrap();
    let f = instance.get_func(&mut store, "f").unwrap();

    let mut results = [Val::I32(0)];
    f.call(&mut store, &[Val::I32(41)], &mut results).unwrap();
    assert_eq!(results, [Val::I32(42)]);
}

// (module (func (export "f") unreachable))
const UNREACHABLE_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03, 0x02,
    0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x05, 0x01, 0x03, 0x00, 0x00, 0x0b,
];

// (module (func (export "f") (result i32) (i32.div_s (i32.const 1) (i32.const 0))))
const DIV_BY_ZERO_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
    0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x09, 0x01, 0x07, 0x00, 0x41,
    0x01, 0x41, 0x00, 0x6d, 0x0b,
];

fn trap_error(wasm: &[u8]) -> Error {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, wasm).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let func = instance.get_func(&mut store, "f").unwrap();

    let mut results = vec![Val::I32(0); func.result_arity(&store)];
    let err = func.call(&mut store, &[], &mut results).err().unwrap();
    match err {
        Error::Trap(_) => err,
        other => panic!("expected a trap, got {other:?}"),
    }
}

#[test]
fn test_trap_on_unreachable() {
    assert_eq!(trap_error(UNREACHABLE_WASM).to_string(), "unreachable");
}

// The trap message is copied out of a `wasm_message_t` whose `size` is the raw
// byte count. Treating it as null terminated and reading `size - 1` dropped the
// last character of every trap, which showed up as "integer divide by zer".
#[test]
fn test_trap_message_is_not_truncated() {
    assert_eq!(
        trap_error(DIV_BY_ZERO_WASM).to_string(),
        "integer divide by zero"
    );
}

// (module (func) (export "" (func 0)))
const EMPTY_NAME_EXPORT_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03, 0x02,
    0x01, 0x00, 0x07, 0x04, 0x01, 0x00, 0x00, 0x00, 0x0a, 0x04, 0x01, 0x02, 0x00, 0x0b,
];

// An export named with the empty string is legal wasm, and zwasm hands its name
// back as {size: 0, data: null} (vecNew, src/api/vec.zig). Comparing that
// through slice::from_raw_parts is undefined, and it was reached while scanning
// for *any* name, so one such export poisoned every lookup in the module.
#[test]
fn empty_named_export_does_not_poison_lookups() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, EMPTY_NAME_EXPORT_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    assert!(instance.get_func(&mut store, "absent").is_none());

    // And the empty name itself resolves, rather than being unreachable.
    let f = instance
        .get_func(&mut store, "")
        .expect("empty-named export");
    f.call(&mut store, &[], &mut []).unwrap();
}

// zwasm's wasm_func_call returns null — "no trap", i.e. success — for a func
// with no instance behind it ("handle.instance orelse return null",
// src/api/instance.zig), so calling a host function directly used to report
// success with untouched results and the callback never running. A host
// function is only callable by a guest, through an import.
#[test]
fn calling_a_host_function_directly_is_an_error() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let host_fn = new_add_one_host_func(&mut store);

    let mut results = [Val::I32(0)];
    let err = host_fn
        .call(&mut store, &[Val::I32(41)], &mut results)
        .err()
        .unwrap();
    assert!(err.to_string().contains("cannot be called directly"));
    assert_eq!(results, [Val::I32(0)], "results must be left untouched");

    // The same function still works as an import.
    let module = Module::new(&mut store, CALLBACK_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[host_fn]).unwrap();
    let f = instance.get_func(&mut store, "f").unwrap();
    f.call(&mut store, &[Val::I32(41)], &mut results).unwrap();
    assert_eq!(results, [Val::I32(42)]);
}

// call() writes results only on success. A trap returns before the write-back,
// so the slice keeps the caller's own values — documented rather than papered
// over, because the guest produced nothing to put there.
#[test]
fn a_trapping_call_leaves_results_alone() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, DIV_BY_ZERO_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let func = instance.get_func(&mut store, "f").unwrap();

    let mut results = [Val::I32(7)];
    let err = func.call(&mut store, &[], &mut results).err().unwrap();
    assert!(matches!(err, Error::Trap(_)));
    assert_eq!(results, [Val::I32(7)]);
}
