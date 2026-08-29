use zwasm_sys::{self as sys};

use crate::{
    error::{non_null, trap_into_result, Error},
    func::Func,
    module::Module,
    store::Store,
};

/// An instantiated module, wrapping `wasm_instance_t`.
///
/// A handle into a [`Store`]; the store owns the C instance and frees it on its
/// own drop, so the handle is `Copy` and carries no destructor.
#[derive(Debug, Clone, Copy)]
pub struct Instance {
    pub(crate) ptr: *mut sys::wasm_instance_t,
    module: *mut sys::wasm_module_t,
    store_id: u64,
}

impl Instance {
    /// Instantiates `module`, running its start function if it has one.
    ///
    /// `imports` has to line up with the module's import section, in declaration
    /// order. Only function imports are supported; a module importing a memory,
    /// global or table cannot be instantiated through this API yet.
    ///
    /// Imports of `wasi_snapshot_preview1.*` are resolved by the host installed
    /// with [`Store::set_wasi`](crate::store::Store::set_wasi), not through this
    /// argument.
    ///
    /// A trap in the start function is returned as [`Error::Trap`].
    ///
    /// # Panics
    ///
    /// Panics when `module` or any import belongs to a different store,
    /// mirroring wasmtime. Passing them through would mix two stores' state on
    /// the C side.
    pub fn new(store: &mut Store, module: &Module, imports: &[Func]) -> Result<Self, Error> {
        store.check(module.store_id);
        for &f in imports {
            store.check(f.store_id);
        }
        let import_externs: Vec<*mut sys::wasm_extern_t> = imports
            .iter()
            .map(|f| unsafe { sys::wasm_func_as_extern(f.ptr) })
            .collect();
        let import_extern_vec = sys::wasm_extern_vec_t {
            size: import_externs.len(),
            data: import_externs.as_ptr() as *mut _,
        };
        let mut trap: *mut sys::wasm_trap_t = std::ptr::null_mut();
        let ptr =
            unsafe { sys::wasm_instance_new(store.ptr, module.ptr, &import_extern_vec, &mut trap) };

        trap_into_result(trap)?;
        let ptr = non_null(ptr, "failed to create instance")?;
        store.instances.push(ptr);

        Ok(Instance {
            ptr,
            module: module.ptr,
            store_id: store.id,
        })
    }

    /// Looks an exported function up by name, like wasmtime's
    /// `Instance::get_func`.
    ///
    /// Returns `None` when nothing is exported under `name`, or when the export
    /// is not a function. The names come from the module's export section,
    /// matched by position, because `wasm_instance_exports` returns values
    /// without names.
    ///
    /// Each call allocates a fresh C handle that the store owns until it drops,
    /// so looking the same export up in a loop grows the store. Resolve once
    /// and keep the [`Func`] — it is `Copy`.
    ///
    /// # Panics
    ///
    /// Panics when `self` belongs to a different store.
    pub fn get_func(&self, store: &mut Store, name: &str) -> Option<Func> {
        store.check(self.store_id);
        let mut module_exports = sys::wasm_exporttype_vec_t {
            size: 0,
            data: std::ptr::null_mut(),
        };
        unsafe { sys::wasm_module_exports(self.module, &mut module_exports) };
        let found_index = (0..module_exports.size).position(|i| {
            let exporttype = unsafe { *module_exports.data.add(i) };
            let name_ptr = unsafe { sys::wasm_exporttype_name(exporttype) };
            // The name belongs to the exporttype, which lives until the vector is
            // deleted below.
            let name_vec = unsafe { &*name_ptr };
            // An empty export name comes back as {size: 0, data: null} (zwasm
            // vecNew, src/api/vec.zig), and from_raw_parts needs a non-null pointer
            // even for a zero length.
            let name_bytes: &[u8] = if name_vec.size == 0 || name_vec.data.is_null() {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(name_vec.data as *const u8, name_vec.size) }
            };
            name_bytes == name.as_bytes()
        });
        unsafe { sys::wasm_exporttype_vec_delete(&mut module_exports) };
        let index = found_index?;

        // The index found above is reused against the instance's exports,
        // which holds because zwasm decodes both vectors from the same
        // `sections.decodeExports` and populates the instance one all-or-nothing
        // (`src/api/instance.zig`). The bounds check below is what keeps a
        // divergence from being read out of range rather than trusted.
        let mut instance_exports = sys::wasm_extern_vec_t {
            size: 0,
            data: std::ptr::null_mut(),
        };
        unsafe { sys::wasm_instance_exports(self.ptr, &mut instance_exports) };

        if index >= instance_exports.size {
            unsafe { sys::wasm_extern_vec_delete(&mut instance_exports) };
            return None;
        }

        let ext = unsafe { *instance_exports.data.add(index) };
        // wasm_extern_as_func borrows out of the vector, so the handle has to be
        // copied before the vector goes. A non-function export makes it null, and
        // wasm_func_copy passes null through (zwasm cloneEntity,
        // src/api/ref_base.zig:249), so the check below covers both cases.
        let ptr = unsafe { sys::wasm_func_copy(sys::wasm_extern_as_func(ext)) };
        unsafe { sys::wasm_extern_vec_delete(&mut instance_exports) };

        if ptr.is_null() {
            return None;
        }
        store.funcs.push(ptr);

        Some(Func::from_export(ptr, store.id))
    }
}
