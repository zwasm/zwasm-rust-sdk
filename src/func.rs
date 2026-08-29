use zwasm_sys as sys;

use crate::{
    error::{non_null, trap_into_result, Error},
    store::Store,
    val::Val,
};

/// A callable function, wrapping `wasm_func_t`.
///
/// A handle into a [`Store`]; the store owns the C function and frees it on its
/// own drop, so the handle is `Copy` and carries no destructor. Obtained from an
/// [`Instance`](crate::instance::Instance) export, or created from a Rust
/// callback with [`Func::new_host`].
#[derive(Debug, Clone, Copy)]
pub struct Func {
    pub(crate) ptr: *mut sys::wasm_func_t,
    pub(crate) store_id: u64,
}

impl Func {
    /// Creates a host function the guest can call.
    ///
    /// The result is meant to be passed to
    /// [`Instance::new`](crate::instance::Instance::new) as an import, so that a
    /// guest reaches the callback through it. [`Func::call`] also works: zwasm
    /// invokes the callback with no instance in between, as wasmtime does.
    ///
    /// Which way it is reached decides what a trap from the callback carries.
    /// Called directly, the trap reaches the caller intact, message and all.
    /// Reached through a guest import, zwasm consumes it and substitutes a
    /// generic one — measured, both paths report
    /// [`TrapKind::BindingError`](crate::error::TrapKind::BindingError), and
    /// only the direct path keeps the callback's own message. Carrying the
    /// callback's detail across the guest boundary would need a field zwasm
    /// does not have (its ADR-0218), so this is a standing difference rather
    /// than something waiting on a fix.
    ///
    /// # Safety
    ///
    /// `functype` must point to a live `wasm_functype_t`. Ownership stays with the
    /// caller, who must release it with `wasm_functype_delete` once this call
    /// returns; the arity is copied here.
    ///
    /// `callback` must accept the argument and result arities that `functype`
    /// declares, and must write every result before returning null. Returning a
    /// non-null trap transfers ownership of that trap to the runtime.
    pub unsafe fn new_host(
        store: &mut Store,
        functype: *const sys::wasm_functype_t,
        callback: sys::wasm_func_callback_t,
    ) -> Result<Self, Error> {
        let func = unsafe { sys::wasm_func_new(store.ptr, functype, callback) };
        let func = non_null(func, "failed to create host function")?;
        store.funcs.push(func);

        Ok(Func {
            ptr: func,
            store_id: store.id,
        })
    }

    pub fn param_arity(&self, store: &Store) -> usize {
        store.check(self.store_id);
        unsafe { sys::wasm_func_param_arity(self.ptr) }
    }

    pub fn result_arity(&self, store: &Store) -> usize {
        store.check(self.store_id);
        unsafe { sys::wasm_func_result_arity(self.ptr) }
    }

    /// Calls the function, writing its results into `results`.
    ///
    /// `params` and `results` have to match the function's declared arities; a
    /// wrong length is reported as an error before anything runs. Size `results`
    /// with [`Func::result_arity`]. Parameter types are not checked here — a
    /// mismatch traps.
    ///
    /// `results` is written only when this returns `Ok`. On any error it keeps
    /// whatever it held, which is the caller's own data rather than anything
    /// from the guest; do not read it after an `Err`.
    ///
    /// A guest trap is returned as [`Error::Trap`] carrying the trap message.
    ///
    /// A function from [`Func::new_host`] can be called this way too, which
    /// runs its callback directly with no instance in between. See there for
    /// what a trap from the callback carries on each path.
    ///
    /// # Errors
    ///
    /// Fails when the arities do not match.
    ///
    /// # Panics
    ///
    /// Panics when `self` belongs to a different store.
    pub fn call(
        &self,
        store: &mut Store,
        params: &[Val],
        results: &mut [Val],
    ) -> Result<(), Error> {
        store.check(self.store_id);
        let nparams = unsafe { sys::wasm_func_param_arity(self.ptr) };
        if params.len() != nparams {
            return Err(Error::Message(format!(
                "expected {nparams} parameters, got {}",
                params.len()
            )));
        }

        let nresults = unsafe { sys::wasm_func_result_arity(self.ptr) };
        if results.len() != nresults {
            return Err(Error::Message(format!(
                "expected {nresults} results, got {}",
                results.len()
            )));
        }

        let params_vals: Vec<sys::wasm_val_t> = params.iter().map(|a| a.clone().into()).collect();
        let params_vec = sys::wasm_val_vec_t {
            size: params_vals.len(),
            data: params_vals.as_ptr() as *mut _,
        };

        let mut results_vals = vec![unsafe { std::mem::zeroed::<sys::wasm_val_t>() }; nresults];
        let mut results_vec = sys::wasm_val_vec_t {
            size: nresults,
            data: results_vals.as_mut_ptr(),
        };

        let trap = unsafe { sys::wasm_func_call(self.ptr, &params_vec, &mut results_vec) };
        trap_into_result(trap, store)?;

        for (slot, val) in results.iter_mut().zip(results_vals) {
            *slot = val.into();
        }

        Ok(())
    }
}
