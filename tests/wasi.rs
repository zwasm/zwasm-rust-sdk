use zwasm_sdk::engine::Engine;
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
    store.set_wasi(config).unwrap();

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
    store.set_wasi(first).unwrap();

    let mut second = WasiConfig::new().unwrap();
    second.set_args(&["second"]).unwrap();
    store.set_wasi(second).unwrap();

    let module = Module::new(&mut store, WASI_IMPORT_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[]);
    assert!(instance.is_ok());
}

#[test]
fn test_unset_wasi() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let config = WasiConfig::new().unwrap();
    store.set_wasi(config).unwrap();
    store.unset_wasi().unwrap();
}

// zwasm captures a raw pointer to the store's WASI host in each import binding
// at instantiation time (`.ctx = wasi_host_ptr`, src/api/instance.zig), while
// zwasm_store_set_wasi frees the old host immediately. Replacing the host after
// an instance exists therefore leaves that instance pointing at freed memory,
// so the store refuses it rather than letting safe code reach the crash.
#[test]
fn changing_the_wasi_host_after_instantiating_is_refused() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let mut first = WasiConfig::new().unwrap();
    first.set_args(&["prog"]).unwrap();
    store.set_wasi(first).unwrap();

    let module = Module::new(&mut store, WASI_IMPORT_WASM).unwrap();
    let _instance = Instance::new(&mut store, &module, &[]).unwrap();

    let second = WasiConfig::new().unwrap();
    let err = store.set_wasi(second).err().unwrap();
    assert!(err.to_string().contains("after instantiating"));

    let err = store.unset_wasi().err().unwrap();
    assert!(err.to_string().contains("after instantiating"));
}

// The refusal consumes the config rather than handing it back in the error.
// Pinned because it is a deliberate choice, not an oversight: reaching the
// refusal means the calls are ordered wrongly, and a fresh store needs a fresh
// config anyway. Freeing exactly once on this path is what the test proves —
// a leak or a double free here would show up under a sanitizer.
#[test]
fn a_refused_wasi_config_is_released() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let module = Module::new(&mut store, WASI_IMPORT_WASM).unwrap();
    let mut wasi = WasiConfig::new().unwrap();
    wasi.set_args(&["prog"]).unwrap();
    store.set_wasi(wasi).unwrap();
    let _instance = Instance::new(&mut store, &module, &[]).unwrap();

    for _ in 0..8 {
        let mut rejected = WasiConfig::new().unwrap();
        rejected.set_args(&["never installed"]).unwrap();
        assert!(store.set_wasi(rejected).is_err());
    }
}
