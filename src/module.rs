use zwasm_sys as sys;

use crate::{
    error::{non_null, Error},
    store::Store,
};

/// A compiled and validated module, wrapping `wasm_module_t`.
///
/// A handle into a [`Store`]; the store owns the C module and frees it on its own
/// drop, so the handle is `Copy` and carries no destructor. A module holds no
/// runtime state, so one module can back several
/// [`Instance`](crate::instance::Instance)s.
///
/// Unlike wasmtime, where a module belongs to an engine and can be instantiated
/// in any store, the wasm-c-api ties a module to the store it was created in.
#[derive(Debug, Clone, Copy)]
pub struct Module {
    pub(crate) ptr: *mut sys::wasm_module_t,
    pub(crate) store_id: u64,
}

impl Module {
    /// Decodes and validates `wasm_bytes`.
    ///
    /// The bytes are copied, so they do not have to outlive the module. Returns an
    /// error when the input is not a valid module; the C API reports no reason, so
    /// the message is generic.
    pub fn new(store: &mut Store, wasm_bytes: &[u8]) -> Result<Self, Error> {
        let binary = sys::wasm_byte_vec_t {
            size: wasm_bytes.len(),
            data: wasm_bytes.as_ptr() as *mut _,
        };
        let ptr = non_null(
            unsafe { sys::wasm_module_new(store.ptr, &binary) },
            "failed to create module",
        )?;
        store.modules.push(ptr);
        Ok(Module {
            ptr,
            store_id: store.id,
        })
    }
}
