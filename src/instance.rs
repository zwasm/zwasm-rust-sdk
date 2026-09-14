use zwasm_sys as sys;

use crate::{
    error::{non_null, trap_into_result, Error},
    func::Func,
    module::Module,
    store::Store,
};

/// Which engine runs one instance, mirroring `ZWASM_ENGINE_*` in zwasm's
/// `include/zwasm.h`.
///
/// Not [`Engine`](crate::engine::Engine), which is `wasm_engine_t` — the
/// compilation environment a [`Store`] is built on. This names the executor
/// behind a single `Instance`.
///
/// [`Auto`](Self::Auto) is a request; [`Instance::engine`] is the answer, and
/// it never reports `Auto`.
///
/// Non-exhaustive because the kinds are C defines zwasm can append to: a kind
/// added upstream should not be a breaking change here.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    /// `ZWASM_ENGINE_AUTO`, what stock `wasm_instance_new` passes.
    ///
    /// Compiles the module with the JIT, and instantiates the interpreter only
    /// for a module the JIT *declines* — an import it cannot satisfy, or a body
    /// it cannot compile. A module the JIT judges *invalid* is not retried:
    /// instantiation fails with
    /// [`TrapKind::InvalidModule`](crate::error::TrapKind::InvalidModule).
    Auto,
    /// `ZWASM_ENGINE_JIT`. Forces the native JIT, with no silent downgrade: a
    /// declined module fails instantiation rather than falling back.
    Jit,
    /// `ZWASM_ENGINE_INTERP`. Forces the interpreter.
    ///
    /// Alone among the three it rejects a module importing
    /// `wasi_snapshot_preview1` when the store has no WASI host configured. It
    /// also cannot import from an instance the JIT backs, which zwasm states as
    /// unsupported rather than as a defect (ADR-0228).
    Interp,
    /// A kind this crate does not know about, carrying the raw value.
    ///
    /// Reached when the linked zwasm reports a kind added after this crate's
    /// conversion was written — a bumped submodule, say. Carrying the value
    /// keeps that from being a panic.
    Unknown(i32),
}

impl EngineKind {
    /// The `engine_kind` byte `zwasm_instance_new_ex` takes.
    ///
    /// `Unknown` is passed through rather than rejected, so a kind read back
    /// from [`Instance::engine`] can be handed to [`Instance::new_with_engine`]
    /// again. zwasm decides what it means.
    pub(crate) fn as_raw(self) -> u8 {
        match self {
            EngineKind::Auto => 0,
            EngineKind::Jit => 1,
            EngineKind::Interp => 2,
            EngineKind::Unknown(other) => other as u8,
        }
    }
}

impl From<i32> for EngineKind {
    fn from(kind: i32) -> Self {
        match kind {
            0 => EngineKind::Auto,
            1 => EngineKind::Jit,
            2 => EngineKind::Interp,
            other => EngineKind::Unknown(other),
        }
    }
}

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
        Self::new_with_engine(store, module, imports, EngineKind::Auto)
    }

    /// Instantiates `module` on a chosen engine, rather than leaving the choice
    /// to zwasm.
    ///
    /// [`Instance::new`] is this with [`EngineKind::Auto`]; everything that doc
    /// says about imports, WASI and start-function traps holds here too.
    ///
    /// # Errors
    ///
    /// A module the chosen engine *declines* fails with [`Error::Message`]
    /// naming the engine that was asked for. zwasm reports a decline as a null
    /// instance with no trap and no reason, so the engine is the only thing
    /// this can add (zwasm/zwasm#353). A module the JIT judges *invalid* fails
    /// with [`TrapKind::InvalidModule`](crate::error::TrapKind::InvalidModule)
    /// instead, on [`Auto`](EngineKind::Auto) and [`Jit`](EngineKind::Jit)
    /// alike.
    ///
    /// # Panics
    ///
    /// Panics when `module` or any import belongs to a different store, as
    /// [`new`](Self::new) does.
    pub fn new_with_engine(
        store: &mut Store,
        module: &Module,
        imports: &[Func],
        engine: EngineKind,
    ) -> Result<Self, Error> {
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
        let ptr = unsafe {
            sys::zwasm_instance_new_ex(
                store.ptr,
                module.ptr,
                &import_extern_vec,
                &mut trap,
                engine.as_raw(),
            )
        };

        trap_into_result(trap, store)?;
        let ptr = non_null(
            ptr,
            &format!("failed to create instance on the {engine:?} engine"),
        )?;
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

        Some(Func {
            ptr,
            store_id: store.id,
        })
    }

    /// The engine that actually ran this instance — [`EngineKind::Jit`] or
    /// [`EngineKind::Interp`], never [`EngineKind::Auto`].
    ///
    /// `Auto` is what you asked for; this is what you got. An instance `Auto`
    /// handed to the interpreter because the JIT declined its module reports
    /// `Interp` here. Mirrors `zwasm_instance_engine` (zwasm's ADR-0200 D3).
    ///
    /// # Panics
    ///
    /// Panics when `self` belongs to a different store.
    pub fn engine(&self, store: &Store) -> EngineKind {
        store.check(self.store_id);
        let mut kind: i32 = 0;
        // `zwasm_instance_engine` returns false only for a null instance, and an
        // `Instance` only ever holds a pointer that passed `non_null`. Returning
        // some kind here instead would mean inventing one the header says it
        // never gives.
        assert!(
            unsafe { sys::zwasm_instance_engine(self.ptr, &mut kind) },
            "zwasm_instance_engine refused a non-null instance"
        );
        EngineKind::from(kind)
    }
}
