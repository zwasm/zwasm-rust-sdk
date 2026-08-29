use std::sync::atomic::AtomicU64;

use zwasm_sys as sys;

use crate::{
    engine::Engine,
    error::{non_null, Error},
    wasi::WasiConfig,
};

static NEXT_STORE_ID: AtomicU64 = AtomicU64::new(0);

/// Runtime state for one thread, wrapping `wasm_store_t`.
///
/// The store owns everything created through it: modules, instances, functions,
/// memories, globals and tables. The other types are `Copy` handles naming an
/// object inside a store, so using one means passing the store back in, and the
/// borrow checker keeps every use inside the store's lifetime:
///
/// ```compile_fail
/// # use zwasm_sdk::{engine::Engine, store::Store, module::Module, instance::Instance, val::Val};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let wasm: &[u8] = &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
/// let engine = Engine::new()?;
/// let mut store = Store::new(&engine)?;
/// let module = Module::new(&mut store, wasm)?;
/// let instance = Instance::new(&mut store, &module, &[])?;
/// drop(store);
/// instance.get_func(&mut store, "f"); // error: borrow of moved value
/// # Ok(())
/// # }
/// ```
///
/// This is wasmtime's ownership model. zwasm resolves every deallocation through
/// store and engine back-pointers, so the store deletes its children first and
/// itself last, and keeps its [`Engine`] alive until after that.
///
/// A store is deliberately neither `Send` nor `Sync`, because the C side is
/// single threaded per store.
///
/// There is no `Default`, because a store needs an [`Engine`], and engines are
/// meant to be created once and shared rather than made as a side effect.
pub struct Store {
    pub(crate) ptr: *mut sys::wasm_store_t,
    pub(crate) id: u64,
    pub(crate) funcs: Vec<*mut sys::wasm_func_t>,
    pub(crate) instances: Vec<*mut sys::wasm_instance_t>,
    pub(crate) modules: Vec<*mut sys::wasm_module_t>,
    pub(crate) memories: Vec<*mut sys::wasm_memory_t>,
    pub(crate) globals: Vec<*mut sys::wasm_global_t>,
    pub(crate) tables: Vec<*mut sys::wasm_table_t>,
    _engine: Engine,
}

impl Store {
    /// Creates a store bound to `engine`, which has to outlive it.
    pub fn new(engine: &Engine) -> Result<Self, Error> {
        let ptr = non_null(
            unsafe { sys::wasm_store_new(engine.ptr()) },
            "failed to create store",
        )?;
        Ok(Store {
            ptr,
            id: NEXT_STORE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            funcs: Vec::new(),
            instances: Vec::new(),
            modules: Vec::new(),
            memories: Vec::new(),
            globals: Vec::new(),
            tables: Vec::new(),
            _engine: engine.clone(),
        })
    }

    pub(crate) fn check(&self, store_id: u64) {
        assert_eq!(
            self.id, store_id,
            "object used with a store it does not belong to"
        );
    }

    /// Installs a WASI host so that imports of `wasi_snapshot_preview1.*` resolve
    /// against it.
    ///
    /// The config is taken by value because the C side takes ownership of it.
    /// Calling twice replaces the host an instantiation from here on will bind.
    ///
    /// # Replacing a host that instances already hold
    ///
    /// An instance binds the host it was created under, by address, and keeps
    /// that one for the rest of its life. Replacing the host therefore changes
    /// nothing for the instances that already exist — it only decides what the
    /// next instantiation binds.
    ///
    /// The replaced config is not freed. zwasm retires it onto the store and
    /// frees it with the store, because those instances still hold its address,
    /// and the directories it preopened stay open until then. A long-lived store
    /// that preopens per run should take a new store per configuration rather
    /// than re-setting this one, or the descriptors accumulate.
    ///
    /// One thing degrades. After a replacement, an earlier instance's clean
    /// `proc_exit` no longer reports the status it asked for: the guest writes
    /// it to the host it bound, while the reader looks at the store's current
    /// one. Measured on all three engines, such a call arrives as
    /// [`Error::Trap`] carrying [`TrapKind::WasiExit`](crate::error::TrapKind::WasiExit) rather than as
    /// [`Error::WasiExit`] — so the fact that it was a WASI exit survives, and
    /// only the status is lost. Tracked upstream as
    /// [zwasm#345](https://github.com/zwasm/zwasm/issues/345); when that lands,
    /// the status comes back and this paragraph goes.
    pub fn set_wasi(&mut self, config: WasiConfig) {
        let config = std::mem::ManuallyDrop::new(config);
        unsafe { sys::zwasm_store_set_wasi(self.ptr, config.ptr) }
    }

    /// Detaches the WASI host, so a later instantiation resolves no
    /// `wasi_snapshot_preview1.*` import.
    ///
    /// Detaching is a replacement like any other: instances that already exist
    /// keep the host they bound, the config is freed with the store rather than
    /// here, and the exit-status caveat on [`Store::set_wasi`] applies.
    pub fn unset_wasi(&mut self) {
        unsafe { sys::zwasm_store_set_wasi(self.ptr, std::ptr::null_mut()) }
    }
}

/// Written out rather than derived. The store's six ownership registries are
/// `Vec<*mut _>`, so a derived `Debug` would put dozens of raw addresses on one
/// line — and what a reader wants from them is how much the store holds, not
/// which pointers it holds. `id` stands in for `ptr`: it identifies the store
/// and is what a cross-store use is checked against, so printing both would say
/// the same thing twice.
impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("id", &self.id)
            .field("funcs", &self.funcs.len())
            .field("instances", &self.instances.len())
            .field("modules", &self.modules.len())
            .field("memories", &self.memories.len())
            .field("globals", &self.globals.len())
            .field("tables", &self.tables.len())
            .finish()
    }
}

impl Drop for Store {
    /// Frees the store's objects children-first, then the store itself.
    ///
    /// The order is not cosmetic. zwasm's `wasm_func_delete` — and the memory,
    /// global and table deletes — reach their allocator through
    /// `handle.instance` when the handle came from an instance, so those have
    /// to go before the instances do. An instance in turn executes out of its
    /// module's byte copy, so modules go last of the children. Everything
    /// resolves through the store, which is why `wasm_store_delete` is last of
    /// all, and the store's `Engine` clone outlives even that because fields
    /// drop only after this function returns.
    ///
    /// Host functions passed to [`Instance::new`](crate::instance::Instance::new)
    /// as imports sit in the same `funcs` list and so are freed before the
    /// instances that imported them. That is safe: `wasm_instance_delete` runs
    /// the host-info finalizer, unregisters the instance and parks its runtime,
    /// and the parked runtime's later teardown frees only its own storage —
    /// neither reads the import bindings or the callback payloads those funcs
    /// own.
    ///
    /// This is also the only place anything is freed: nothing hands back an
    /// individual object, so a handle can never name something already gone
    /// while its store is alive. `Copy` handles are sound because of that.
    ///
    /// Deleting each object here and then the store assumes the C store does
    /// not own them too, or every one would be freed twice. It does not: a
    /// zwasm `Store` tracks its engine, its WASI host, its live instances and
    /// their parked runtimes, and nothing else — funcs, memories, globals,
    /// tables and modules are the caller's to release. Instances are the one
    /// overlap, and `wasm_instance_delete` unregisters each from the store's
    /// live list, so the cascade in `wasm_store_delete` does not reach them a
    /// second time.
    fn drop(&mut self) {
        unsafe {
            for &f in self.funcs.iter() {
                sys::wasm_func_delete(f);
            }
            for &m in self.memories.iter() {
                sys::wasm_memory_delete(m);
            }
            for &g in self.globals.iter() {
                sys::wasm_global_delete(g);
            }
            for &t in self.tables.iter() {
                sys::wasm_table_delete(t);
            }
            for &i in self.instances.iter() {
                sys::wasm_instance_delete(i);
            }
            for &m in self.modules.iter() {
                sys::wasm_module_delete(m);
            }
            sys::wasm_store_delete(self.ptr);
        }
    }
}
