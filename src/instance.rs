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
/// Not [`Engine`](crate::engine::Engine) — that is `wasm_engine_t`, the
/// environment a [`Store`] is built on. This names one instance's executor.
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
    /// for a module the JIT *declines*. One it judges *invalid* is not retried:
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
    /// keeps that from being a panic, and lets the kind be handed back to
    /// [`Instance::new_with_engine`], where that zwasm does know it.
    ///
    /// Asking for one zwasm does *not* know is not an error there: it maps any
    /// unrecognized byte to [`Auto`](Self::Auto) rather than refusing, which
    /// its C header does not say (zwasm/zwasm#459). So a kind this crate
    /// invented, rather than read back, is a silent `Auto`.
    Unknown(i32),
}

impl EngineKind {
    /// The `engine_kind` byte `zwasm_instance_new_ex` takes, or `None` for an
    /// [`Unknown`](Self::Unknown) that does not fit one — truncating it would
    /// ask for a different engine, and nothing downstream would notice.
    pub(crate) fn as_raw(self) -> Option<u8> {
        match self {
            EngineKind::Auto => Some(0),
            EngineKind::Jit => Some(1),
            EngineKind::Interp => Some(2),
            EngineKind::Unknown(other) => u8::try_from(other).ok(),
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
    /// Nothing bounds what else it does. It runs before there is an instance to
    /// arm, and the C ABI this crate binds takes no limits on instantiation
    /// (zwasm/zwasm#465) — zwasm can arm them ahead of it, but only through its
    /// Zig API. So a start function that does not return hangs the host, and
    /// one that grows memory keeps those pages.
    /// [`set_fuel`](Self::set_fuel) and
    /// [`set_memory_pages_limit`](Self::set_memory_pages_limit) cover what
    /// happens after this returns, not this.
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
    /// An [`EngineKind::Unknown`] too large for the byte the C entry point
    /// takes fails with [`Error::Message`] before zwasm is called at all.
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
        let engine_kind = engine.as_raw().ok_or_else(|| {
            Error::Message(format!("{engine:?} is not a selector zwasm can be given"))
        })?;
        let mut trap: *mut sys::wasm_trap_t = std::ptr::null_mut();
        let ptr = unsafe {
            sys::zwasm_instance_new_ex(
                store.ptr,
                module.ptr,
                &import_extern_vec,
                &mut trap,
                engine_kind,
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

    /// Arms the fuel budget, so the guest traps with
    /// [`TrapKind::OutOfFuel`](crate::error::TrapKind::OutOfFuel) when it runs
    /// out instead of running to completion.
    ///
    /// Re-arms rather than adds: a second call replaces whatever is left, and
    /// an instance that already exhausted its budget runs again once re-armed.
    ///
    /// The budget is per instance. wasmtime meters a whole `Store`, so its
    /// `Store::set_fuel` and this one are not the same scope.
    ///
    /// # Units
    ///
    /// A unit is engine-specific: the interpreter counts instructions executed,
    /// the JIT counts poll-site crossings — one per loop back-edge, plus one on
    /// entering a function that is polled at all. An *n*-iteration loop costs
    /// *n* + 1 on the JIT and roughly an order of magnitude more on the
    /// interpreter, so a budget means nothing portable unless the engine is
    /// pinned with [`new_with_engine`](Self::new_with_engine).
    ///
    /// # What a budget does not stop
    ///
    /// The x86_64 JIT backend emits no poll for a function that never touches
    /// its runtime pointer, so a trivial one — a body that only pushes a
    /// constant, say — runs to completion on an exhausted budget rather than
    /// trapping. The arm64 backend polls on every function entry, so the same
    /// call traps there: this is a property of the backend, not of the engine
    /// (zwasm/zwasm#466). A loop is polled on both, so it bounds how *little*
    /// can be charged, not how long a guest can run.
    ///
    /// The start function is outside any budget, because it has already run by
    /// the time there is an instance to arm — see [`new`](Self::new).
    ///
    /// # Panics
    ///
    /// Panics when `self` belongs to a different store.
    pub fn set_fuel(&self, store: &mut Store, fuel: u64) {
        store.check(self.store_id);
        unsafe { sys::zwasm_instance_set_fuel(self.ptr, fuel) }
    }

    /// Removes the budget, so the guest runs unmetered again.
    ///
    /// [`fuel_remaining`](Self::fuel_remaining) reports `None` afterwards,
    /// whatever was left.
    ///
    /// # Panics
    ///
    /// Panics when `self` belongs to a different store.
    pub fn disable_fuel(&self, store: &mut Store) {
        store.check(self.store_id);
        unsafe { sys::zwasm_instance_disable_fuel(self.ptr) }
    }

    /// The fuel left on this instance, or `None` when no budget is armed.
    ///
    /// `None` is not zero. An instance that ran out reports `Some(0)` and stays
    /// metered until [`set_fuel`](Self::set_fuel) re-arms it or
    /// [`disable_fuel`](Self::disable_fuel) removes it; one that was never
    /// armed, or was disabled, reports `None`.
    ///
    /// # Panics
    ///
    /// Panics when `self` belongs to a different store.
    pub fn fuel_remaining(&self, store: &Store) -> Option<u64> {
        store.check(self.store_id);
        let mut fuel: u64 = 0;
        unsafe { sys::zwasm_instance_fuel_remaining(self.ptr, &mut fuel) }.then_some(fuel)
    }

    /// Caps how far the guest can grow memory 0, in wasm pages of 64 KiB — a
    /// ceiling meant as 64 MiB is `1024`, not the byte count.
    ///
    /// zwasm offers no way to read the cap back, so a caller that needs to
    /// know has to remember.
    ///
    /// # It does not trap
    ///
    /// A `memory.grow` past the cap returns the spec's own grow failure, `-1`,
    /// and leaves the memory at its current size — nothing reaches the host.
    /// In particular this is unrelated to
    /// [`TrapKind::OutOfMemory`](crate::error::TrapKind::OutOfMemory), which
    /// zwasm raises for an allocator failure or the GC heap's own ceiling.
    ///
    /// Setting a cap below the size already allocated shrinks nothing and
    /// refuses every later grow, including one of zero pages — which otherwise
    /// succeeds at any cap the size has not passed. A start function that grew
    /// memory leaves exactly that state, and cannot be capped ahead of time —
    /// see [`new`](Self::new).
    ///
    /// # Not the module's declared maximum
    ///
    /// A memory type can declare a maximum of its own, which belongs to the
    /// module and is what [`Memory::grow`](crate::memory::Memory::grow)
    /// checks. This is a host ceiling on one instance, and may sit below it.
    ///
    /// # Panics
    ///
    /// Panics when `self` belongs to a different store.
    pub fn set_memory_pages_limit(&self, store: &mut Store, max_pages: u64) {
        store.check(self.store_id);
        unsafe { sys::zwasm_instance_set_memory_pages_limit(self.ptr, max_pages) }
    }

    /// Removes the cap, so the guest can grow to whatever the module's own
    /// maximum allows again.
    ///
    /// # Panics
    ///
    /// Panics when `self` belongs to a different store.
    pub fn clear_memory_pages_limit(&self, store: &mut Store) {
        store.check(self.store_id);
        unsafe { sys::zwasm_instance_clear_memory_pages_limit(self.ptr) }
    }

    /// The engine that actually ran this instance — [`EngineKind::Jit`] or
    /// [`EngineKind::Interp`], never [`EngineKind::Auto`].
    ///
    /// `Auto` is what you asked for; this is what you got — an instance `Auto`
    /// handed to the interpreter because the JIT declined its module reports
    /// `Interp`. Mirrors `zwasm_instance_engine` (zwasm's ADR-0200 D3).
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
