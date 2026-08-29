use zwasm_sys::{self as sys};

use crate::{
    error::{non_null, Error},
    store::Store,
    val::Val,
};

/// A global variable, wrapping `wasm_global_t`.
///
/// A handle into a [`Store`]; the store owns the C global and frees it on its own
/// drop, so the handle is `Copy` and carries no destructor.
#[derive(Debug, Clone, Copy)]
pub struct Global {
    pub(crate) ptr: *mut sys::wasm_global_t,
    pub(crate) store_id: u64,
}

impl Global {
    /// Creates a global holding `initial`.
    ///
    /// The value type is taken from `initial`. `mutable` decides whether
    /// [`Global::set`] is allowed.
    pub fn new(store: &mut Store, initial: Val, mutable: bool) -> Result<Self, Error> {
        let valtype = non_null(
            unsafe { sys::wasm_valtype_new(initial.kind()) },
            "failed to create value type",
        )?;
        let mutability = if mutable {
            sys::wasm_mutability_enum_WASM_VAR
        } else {
            sys::wasm_mutability_enum_WASM_CONST
        };

        // wasm_globaltype_new takes ownership of valtype, so valtype is only ours
        // to release while this call has not succeeded.
        let globaltype = unsafe { sys::wasm_globaltype_new(valtype, mutability as u8) };
        if globaltype.is_null() {
            unsafe { sys::wasm_valtype_delete(valtype) };
            return Err(Error::Message("failed to create global type".to_string()));
        }

        let initial_val: sys::wasm_val_t = initial.into();
        let ptr = unsafe { sys::wasm_global_new(store.ptr, globaltype, &initial_val) };
        unsafe { sys::wasm_globaltype_delete(globaltype) };

        let ptr = non_null(ptr, "failed to create global")?;
        store.globals.push(ptr);

        Ok(Global {
            ptr,
            store_id: store.id,
        })
    }

    /// Reads the current value.
    ///
    /// This takes a shared borrow where wasmtime's `Global::get` takes an
    /// exclusive one, which holds while [`Val`] covers only the numeric types:
    /// the C side just reads the value cell. A reference-typed `Val` would make
    /// this allocate an owned `wasm_ref_t`, and the borrow would have to
    /// tighten to match.
    pub fn get(&self, store: &Store) -> Val {
        store.check(self.store_id);
        let mut out: sys::wasm_val_t = unsafe { std::mem::zeroed() };
        unsafe { sys::wasm_global_get(self.ptr, &mut out) };
        Val::from(out)
    }

    /// Writes `value`.
    ///
    /// Fails on an immutable global and on a value of the wrong type. The C API
    /// rejects both silently, so the checks are made here against the global's
    /// own type.
    pub fn set(&self, store: &mut Store, value: Val) -> Result<(), Error> {
        store.check(self.store_id);
        let global_type = unsafe { sys::wasm_global_type(self.ptr) };
        let mutability = unsafe { sys::wasm_globaltype_mutability(global_type) };
        let content = unsafe { sys::wasm_globaltype_content(global_type) };
        let kind = unsafe { sys::wasm_valtype_kind(content) };
        unsafe { sys::wasm_globaltype_delete(global_type) };

        if mutability != sys::wasm_mutability_enum_WASM_VAR as u8 {
            return Err(Error::Message(
                "cannot set the value of an immutable global".to_string(),
            ));
        }

        if kind != value.kind() {
            return Err(Error::Message(
                "value type does not match the global's type".to_string(),
            ));
        }

        let val: sys::wasm_val_t = value.into();
        unsafe { sys::wasm_global_set(self.ptr, &val) };
        Ok(())
    }
}
