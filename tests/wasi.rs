use zwasm_sdk::engine::Engine;
use zwasm_sdk::error::{Error, TrapKind};
use zwasm_sdk::instance::Instance;
use zwasm_sdk::module::Module;
use zwasm_sdk::store::Store;
use zwasm_sdk::wasi::WasiConfig;

// (module
//   (import "wasi_snapshot_preview1" "fd_write"
//     (func (param i32 i32 i32 i32) (result i32))))
const WASI_IMPORT_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic + version
    // type section: (i32 i32 i32 i32) -> (i32)
    0x01, 0x09, 0x01, 0x60, 0x04, 0x7f, 0x7f, 0x7f, 0x7f, 0x01, 0x7f,
    // import section: "wasi_snapshot_preview1" "fd_write" func 0
    0x02, 0x23, 0x01, //
    0x16, 0x77, 0x61, 0x73, 0x69, 0x5f, 0x73, 0x6e, 0x61, 0x70, 0x73, 0x68, 0x6f, 0x74, 0x5f, 0x70,
    0x72, 0x65, 0x76, 0x69, 0x65, 0x77, 0x31, //
    0x08, 0x66, 0x64, 0x5f, 0x77, 0x72, 0x69, 0x74, 0x65, //
    0x00, 0x00,
];

#[test]
fn test_wasi_config_new() {
    let config = WasiConfig::new();
    assert!(config.is_ok());
}

#[test]
fn test_wasi_config_default() {
    let _config = WasiConfig::default();
}

#[test]
fn test_wasi_config_inherit_stdio() {
    let mut config = WasiConfig::new().unwrap();
    config.inherit_stdio();
}

#[test]
fn test_wasi_config_inherit_env() {
    let mut config = WasiConfig::new().unwrap();
    assert!(config.inherit_env().is_ok());
}

#[test]
fn test_wasi_config_set_args() {
    let mut config = WasiConfig::new().unwrap();
    assert!(config.set_args(&["prog", "--flag"]).is_ok());
}

#[test]
fn test_wasi_config_set_args_empty() {
    let mut config = WasiConfig::new().unwrap();
    assert!(config.set_args(&[]).is_ok());
}

#[test]
fn test_wasi_config_set_args_interior_null() {
    let mut config = WasiConfig::new().unwrap();
    assert!(config.set_args(&["ok", "bad\0arg"]).is_err());
}

#[test]
fn test_wasi_config_set_envs() {
    let mut config = WasiConfig::new().unwrap();
    assert!(config
        .set_envs(&[("KEY1", "VALUE1"), ("KEY2", "VALUE2")])
        .is_ok());
}

#[test]
fn test_wasi_config_set_envs_interior_null() {
    let mut config = WasiConfig::new().unwrap();
    assert!(config.set_envs(&[("KEY", "bad\0value")]).is_err());
}

#[test]
fn test_wasi_config_preopen_dir() {
    let dir = std::env::temp_dir();
    let mut config = WasiConfig::new().unwrap();
    assert!(config.preopen_dir(dir.to_str().unwrap(), "/").is_ok());
}

#[test]
fn test_wasi_config_preopen_dir_interior_null() {
    let mut config = WasiConfig::new().unwrap();
    assert!(config.preopen_dir("/tmp\0bad", "/").is_err());
}

#[test]
fn test_module_with_wasi_import() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, WASI_IMPORT_WASM);
    assert!(module.is_ok());
}

#[test]
fn test_instantiate_with_wasi_succeeds() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let mut config = WasiConfig::new().unwrap();
    config.set_args(&["prog"]).unwrap();
    config.inherit_stdio();
    store.set_wasi(config);

    let module = Module::new(&mut store, WASI_IMPORT_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]);
    assert!(instance.is_ok());
}

#[test]
fn test_set_wasi_twice_replaces() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let mut first = WasiConfig::new().unwrap();
    first.set_args(&["first"]).unwrap();
    store.set_wasi(first);

    let mut second = WasiConfig::new().unwrap();
    second.set_args(&["second"]).unwrap();
    store.set_wasi(second);

    let module = Module::new(&mut store, WASI_IMPORT_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]);
    assert!(instance.is_ok());
}

#[test]
fn test_unset_wasi() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let config = WasiConfig::new().unwrap();
    store.set_wasi(config);
    store.unset_wasi();
}

// Replacing the host used to be refused: zwasm freed the old one while live
// instances still held its address, so calling into an instance afterwards
// reached released memory. zwasm/zwasm#314 retires the old host onto the store
// instead, and this is the call that used to be the crash.
//
// It also pins what the replacement costs. The guest writes its exit status to
// the host it bound at instantiation, while the reader looks at the store's
// current one, so the status is invisible afterwards: the call arrives as a
// trap that still carries TrapKind::WasiExit rather than as Error::WasiExit.
// That is zwasm/zwasm#345, and this asserts today's behaviour rather than the
// wanted one — when #345 lands, this test is what notices.
#[test]
fn an_instance_outlives_the_wasi_host_it_bound() {
    let engine = Engine::new().unwrap();
    let mut store = store_with_wasi(&engine);

    let module = Module::new(&mut store, &exit_wasm(0)).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    let mut second = WasiConfig::new().unwrap();
    second.set_args(&["replacement"]).unwrap();
    store.set_wasi(second);

    let start = instance
        .get_func(&mut store, "_start")
        .expect("no _start export");
    let err = start.call(&mut store, &[], &mut []).unwrap_err();

    assert_eq!(
        err.trap_kind(),
        Some(TrapKind::WasiExit),
        "the exit should still be recognisable as one: {err:?}"
    );
    assert!(
        matches!(err, Error::Trap { .. }),
        "zwasm/zwasm#345 appears to be fixed — the status is readable again, so \
         this test and the caveat on Store::set_wasi should become WasiExit: {err:?}"
    );
}

// Replacing repeatedly retires each old config onto the store rather than
// freeing it, so this is where a double free or a leak of the retired configs
// would show up under a sanitizer.
#[test]
fn replaced_wasi_configs_survive_until_the_store_dies() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let module = Module::new(&mut store, WASI_IMPORT_WASM).unwrap();
    let mut wasi = WasiConfig::new().unwrap();
    wasi.set_args(&["prog"]).unwrap();
    store.set_wasi(wasi);
    let _instance = Instance::new(&mut store, &module, &[]).unwrap();

    for n in 0..8 {
        let mut next = WasiConfig::new().unwrap();
        next.set_args(&[&format!("replacement {n}")]).unwrap();
        store.set_wasi(next);
    }
}

// (module
//   (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
//   (func (export "_start") (call $exit (i32.const 0))))
const WASI_EXIT_WASM: [u8; 95] = [
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x60, 0x01, 0x7f, 0x00, 0x60,
    0x00, 0x00, 0x02, 0x24, 0x01, 0x16, 0x77, 0x61, 0x73, 0x69, 0x5f, 0x73, 0x6e, 0x61, 0x70, 0x73,
    0x68, 0x6f, 0x74, 0x5f, 0x70, 0x72, 0x65, 0x76, 0x69, 0x65, 0x77, 0x31, 0x09, 0x70, 0x72, 0x6f,
    0x63, 0x5f, 0x65, 0x78, 0x69, 0x74, 0x00, 0x00, 0x03, 0x02, 0x01, 0x01, 0x07, 0x0a, 0x01, 0x06,
    0x5f, 0x73, 0x74, 0x61, 0x72, 0x74, 0x00, 0x01, 0x0a, 0x08, 0x01, 0x06, 0x00, 0x41, 0x00, 0x10,
    0x00, 0x0b, 0x00, 0x0b, 0x04, 0x6e, 0x61, 0x6d, 0x65, 0x01, 0x04, 0x01, 0x00, 0x01, 0x65,
];

// The `i32.const` operand `_start` hands to proc_exit, patched per case, so the
// status has to stay a single-byte signed LEB128.
const EXIT_STATUS_OFFSET: usize = 78;

// (module (func (export "_start") unreachable)) — traps without proc_exit.
const FAULT_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03, 0x02,
    0x01, 0x00, 0x07, 0x0a, 0x01, 0x06, 0x5f, 0x73, 0x74, 0x61, 0x72, 0x74, 0x00, 0x00, 0x0a, 0x05,
    0x01, 0x03, 0x00, 0x00, 0x0b,
];

// (module
//   (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
//   (func $s (call $exit (i32.const 7)))
//   (start $s)) — exits before instantiation returns.
const START_EXIT_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x60, 0x01, 0x7f, 0x00, 0x60,
    0x00, 0x00, 0x02, 0x24, 0x01, 0x16, 0x77, 0x61, 0x73, 0x69, 0x5f, 0x73, 0x6e, 0x61, 0x70, 0x73,
    0x68, 0x6f, 0x74, 0x5f, 0x70, 0x72, 0x65, 0x76, 0x69, 0x65, 0x77, 0x31, 0x09, 0x70, 0x72, 0x6f,
    0x63, 0x5f, 0x65, 0x78, 0x69, 0x74, 0x00, 0x00, 0x03, 0x02, 0x01, 0x01, 0x08, 0x01, 0x01, 0x0a,
    0x08, 0x01, 0x06, 0x00, 0x41, 0x07, 0x10, 0x00, 0x0b, 0x00, 0x0e, 0x04, 0x6e, 0x61, 0x6d, 0x65,
    0x01, 0x07, 0x02, 0x00, 0x01, 0x65, 0x01, 0x01, 0x73,
];

// (module (func $s unreachable) (start $s)) — faults during instantiation,
// without reaching proc_exit.
const START_FAULT_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03, 0x02,
    0x01, 0x00, 0x08, 0x01, 0x00, 0x0a, 0x05, 0x01, 0x03, 0x00, 0x00, 0x0b, 0x00, 0x0b, 0x04, 0x6e,
    0x61, 0x6d, 0x65, 0x01, 0x04, 0x01, 0x00, 0x01, 0x73,
];

fn exit_wasm(status: u8) -> Vec<u8> {
    assert!(
        status <= 63,
        "a status above 63 needs more than one LEB128 byte"
    );
    let mut wasm = WASI_EXIT_WASM.to_vec();
    assert_eq!(
        wasm[EXIT_STATUS_OFFSET], 0,
        "EXIT_STATUS_OFFSET no longer points at the i32.const operand"
    );
    wasm[EXIT_STATUS_OFFSET] = status;
    wasm
}

fn store_with_wasi(engine: &Engine) -> Store {
    let mut store = Store::new(engine).unwrap();
    let mut config = WasiConfig::new().unwrap();
    config.set_args(&["prog"]).unwrap();
    store.set_wasi(config);
    store
}

fn run_start(store: &mut Store, wasm: &[u8]) -> Result<(), Error> {
    let module = Module::new(store, wasm).unwrap();
    let instance = Instance::new(store, &module, &[])?;
    let func = instance
        .get_func(store, "_start")
        .expect("no _start export");
    func.call(store, &[], &mut [])
}

// A WASI command reaches proc_exit even when it succeeds: a wasi-libc `_start`
// that returns normally calls proc_exit(0). So this is the ordinary end of a
// successful run, and it still arrives as an Err.
#[test]
fn a_guest_that_exits_cleanly_reports_its_status() {
    let engine = Engine::new().unwrap();
    let mut store = store_with_wasi(&engine);

    let err = run_start(&mut store, &exit_wasm(0)).unwrap_err();
    assert!(
        matches!(err, Error::WasiExit { code: 0 }),
        "expected WasiExit {{ code: 0 }}, got {err:?}"
    );
}

#[test]
fn a_nonzero_exit_status_survives() {
    let engine = Engine::new().unwrap();
    let mut store = store_with_wasi(&engine);

    let err = run_start(&mut store, &exit_wasm(3)).unwrap_err();
    assert!(
        matches!(err, Error::WasiExit { code: 3 }),
        "expected WasiExit {{ code: 3 }}, got {err:?}"
    );
}

// A fault is a fault even where a clean exit came before it. zwasm/zwasm#341
// used to break this: the status lived on the Store and was never cleared, so
// an implementation asking "is a status readable" before "what kind of trap is
// this" reported the fault below as a clean exit with code 0 — the value that
// reads as success. That was measured here on 1bcc0edae across all three
// engines before the fix.
//
// The fix clears the status inside `wasm_func_call`, so this case no longer
// separates the two orderings — it passes either way now, and is kept as the
// behavioural assertion plus a guard against that clear regressing.
// `a_start_section_fault_after_an_exit_is_still_a_fault` is what pins the
// ordering, because instantiation is not a call and does not clear.
#[test]
fn a_fault_after_an_exit_in_the_same_store_is_still_a_fault() {
    let engine = Engine::new().unwrap();
    let mut store = store_with_wasi(&engine);

    let first = run_start(&mut store, &exit_wasm(0)).unwrap_err();
    assert!(matches!(first, Error::WasiExit { code: 0 }), "{first:?}");

    let second = run_start(&mut store, FAULT_WASM).unwrap_err();
    assert!(
        matches!(second, Error::Trap { .. }),
        "a genuine unreachable read back the earlier exit status: {second:?}"
    );
    assert_eq!(second.trap_kind(), Some(TrapKind::Unreachable));
}

// A start section runs during instantiation, so the exit surfaces from
// Instance::new rather than from a call. Both paths go through the same
// conversion, and this is the one that would be missed if only Func::call did.
#[test]
fn a_start_section_exit_surfaces_from_instantiation() {
    let engine = Engine::new().unwrap();
    let mut store = store_with_wasi(&engine);

    let module = Module::new(&mut store, START_EXIT_WASM).unwrap();
    let err =
        Instance::new(&mut store, &module, &[]).expect_err("instantiation should have failed");
    assert!(
        matches!(err, Error::WasiExit { code: 7 }),
        "expected WasiExit {{ code: 7 }}, got {err:?}"
    );
}

// The clear that zwasm/zwasm#341 added sits in `wasm_func_call`, so two calls
// no longer catch an implementation that asks "is a status readable" before
// asking "what kind of trap is this". Instantiation is not a call: it neither
// clears the status nor is covered by that fix, so a start-section exit
// followed by a start-section fault still reaches the stale status. This is
// the case that keeps the ordering pinned.
#[test]
fn a_start_section_fault_after_an_exit_is_still_a_fault() {
    let engine = Engine::new().unwrap();
    let mut store = store_with_wasi(&engine);

    let exiting = Module::new(&mut store, START_EXIT_WASM).unwrap();
    let first =
        Instance::new(&mut store, &exiting, &[]).expect_err("the start section should have exited");
    assert!(matches!(first, Error::WasiExit { code: 7 }), "{first:?}");

    let faulting = Module::new(&mut store, START_FAULT_WASM).unwrap();
    let second = Instance::new(&mut store, &faulting, &[])
        .expect_err("the start section should have trapped");
    assert!(
        matches!(second, Error::Trap { .. }),
        "a start-section unreachable read back the earlier exit status: {second:?}"
    );
}

// WasiExit carries a kind like any other trap: kind 18 always becomes
// Error::WasiExit, so returning None here would leave TrapKind::WasiExit
// unreachable through the public API.
#[test]
fn a_wasi_exit_carries_its_kind() {
    let engine = Engine::new().unwrap();
    let mut store = store_with_wasi(&engine);

    let err = run_start(&mut store, &exit_wasm(0)).unwrap_err();
    assert_eq!(err.trap_kind(), Some(TrapKind::WasiExit));
}
