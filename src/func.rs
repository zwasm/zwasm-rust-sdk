use std::os::raw::c_void;

use zwasm_sys as sys;

use crate::{
    error::{error_to_trap, non_null, trap_into_result, Error},
    store::Store,
    val::{Val, ValType},
};

/// A callable function, wrapping `wasm_func_t`.
///
/// A handle into a [`Store`]; the store owns the C function and frees it on its
/// own drop, so the handle is `Copy` and carries no destructor. Obtained from an
/// [`Instance`](crate::instance::Instance) export, built from a Rust closure
/// with [`Func::new`], or from a raw C callback with [`Func::new_host`].
#[derive(Debug, Clone, Copy)]
pub struct Func {
    pub(crate) ptr: *mut sys::wasm_func_t,
    pub(crate) store_id: u64,
}

impl Func {
    /// Creates a host function from a Rust closure, like wasmtime's `Func::new`.
    ///
    /// The result is meant to be passed to
    /// [`Instance::new`](crate::instance::Instance::new) as an import.
    /// [`Func::call`] also works, which runs the closure with no instance in
    /// between.
    ///
    /// The closure is `Fn`, not `FnMut`: a guest can reach the same import
    /// re-entrantly, so state has to go through a [`Cell`](std::cell::Cell) or
    /// [`RefCell`](std::cell::RefCell) rather than through `&mut`. It is
    /// neither `Send` nor `Sync` either, which is what lets it capture an
    /// [`Rc`](std::rc::Rc): nothing in this crate can move a store to another
    /// thread, so nothing can move the closure to one.
    ///
    /// The store owns the closure and drops it with itself. [`Func`] is a
    /// `Copy` handle with no destructor, so letting one go frees nothing.
    ///
    /// # No access to the store
    ///
    /// The closure sees its arguments and nothing else — it cannot read the
    /// caller's memory, which is what wasmtime's `Caller` is for. That is not a
    /// simplification: zwasm's callback receives no instance, so there is
    /// nothing to resolve an export against, and `env` is fixed when the
    /// function is built while one function can be imported by many instances.
    /// Asked upstream as zwasm/zwasm#486; a constructor that passes a caller
    /// can be added beside this one without disturbing it.
    ///
    /// Calling back into the store is out for the same reason — there is no
    /// `&mut Store` to be had inside a call that already holds one.
    ///
    /// # Unwritten results
    ///
    /// Each slot starts at a zero of its declared type, so a closure that
    /// returns `Ok(())` without filling one hands the guest that zero. This is
    /// where the safe path parts from [`new_host`](Self::new_host), whose
    /// contract is that every result is written before returning; there, an
    /// unwritten slot holds whatever the runtime left in it. A zero is a
    /// defined answer rather than a good one, and telling "not written" from
    /// "deliberately zero" would need a sentinel no wasm value type has.
    ///
    /// # Errors
    ///
    /// Fails when a value type, the function type, or the function itself
    /// cannot be allocated.
    ///
    /// An `Err` from the closure becomes a trap, and so does a panic — a panic
    /// crossing an `extern "C"` boundary would otherwise abort the process. So
    /// does a result whose type does not match what `results` declared, which
    /// is refused rather than passed on to the guest.
    ///
    /// Which way the function was reached decides what that trap carries, the
    /// same way it does for [`new_host`](Self::new_host): called directly the
    /// message survives, reached through a guest import zwasm substitutes a
    /// generic one.
    pub fn new(
        store: &mut Store,
        params: &[ValType],
        results: &[ValType],
        f: impl Fn(&[Val], &mut [Val]) -> Result<(), Error> + 'static,
    ) -> Result<Self, Error> {
        // Built before the box, so a failure here has nothing to unwind.
        let functype = new_functype(params, results)?;

        let env = Box::into_raw(Box::new(HostEnv {
            f: Box::new(f),
            store: store.ptr,
            results: results.to_vec(),
        })) as *mut c_void;

        let func = unsafe {
            sys::wasm_func_new_with_env(store.ptr, functype, Some(trampoline), env, Some(finalize))
        };
        unsafe { sys::wasm_functype_delete(functype) };

        if func.is_null() {
            // The box is ours again, along with whatever the closure captured.
            // Every `return null` in zwasm's `funcNewImpl`
            // (`src/api/extern_new.zig`) frees its own allocations and stores
            // the finalizer without ever calling it, so nothing else will.
            drop(unsafe { Box::from_raw(env as *mut HostEnv) });
            return Err(Error::Message("failed to create host function".to_string()));
        }
        store.funcs.push(func);

        Ok(Func {
            ptr: func,
            store_id: store.id,
        })
    }

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
    /// A host function — from [`Func::new`] or [`new_host`](Self::new_host) —
    /// can be called this way too, which runs its body directly with no
    /// instance in between. See either for what a trap from it carries on each
    /// path.
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

        let params_vals: Vec<sys::wasm_val_t> = params.iter().map(|&v| v.into()).collect();
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

/// What a host function's body is, once boxed.
type HostCallback = dyn Fn(&[Val], &mut [Val]) -> Result<(), Error>;

/// The payload [`Func::new`] hands to C, recovered by the trampoline and freed
/// by the finalizer.
struct HostEnv {
    f: Box<HostCallback>,
    /// Needed by `wasm_trap_new`. Valid for as long as the callback can run:
    /// the store owns the func, so it outlives every call through it.
    store: *mut sys::wasm_store_t,
    /// Needed to size and type the result slots, which the C side does not
    /// describe to the callback.
    results: Vec<ValType>,
}

/// Calls the boxed closure on zwasm's behalf.
///
/// Everything is inside `catch_unwind`: a panic crossing an `extern "C"`
/// boundary aborts the process, and one guest call must not be able to do that.
/// A panic becomes a trap, like any other failure the closure reports.
unsafe extern "C" fn trampoline(
    env: *mut c_void,
    args: *const sys::wasm_val_vec_t,
    results: *mut sys::wasm_val_vec_t,
) -> *mut sys::wasm_trap_t {
    // The box `Func::new` leaked, alive until the store frees this func. The
    // store pointer is valid for the same reason: the store owns the func, so
    // it outlives every call made through it.
    let env = unsafe { &*(env as *const HostEnv) };

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let args = unsafe { read_vals(args) };

        // Built from the declared types, not from what arrived: zwasm fills
        // every result slot with `{kind: i32, of: 0}` before calling
        // (`src/api/instance.zig`), so reading them back would hand an `f64`
        // result the wrong kind.
        let mut out: Vec<Val> = env.results.iter().map(|ty| ty.zero()).collect();

        (env.f)(&args, &mut out)?;

        // Checked after the closure ran, because this is about what it wrote.
        // The count comes from the same place the slots do, so a mismatch means
        // zwasm disagreeing with the functype it was given — refused rather
        // than truncated, for the same reason a wrong type is.
        let slots = unsafe { (*results).size };
        if out.len() != slots {
            return Err(Error::Message(format!(
                "host function wrote {} results into {slots} slots",
                out.len()
            )));
        }
        for (i, (val, ty)) in out.iter().zip(&env.results).enumerate() {
            if val.kind() != ty.kind() {
                return Err(Error::Message(format!(
                    "host function result {i}: expected {ty:?}, got {val:?}"
                )));
            }
        }

        unsafe { write_vals(results, &out) };
        Ok(())
    }));

    match outcome {
        Ok(Ok(())) => std::ptr::null_mut(),
        Ok(Err(e)) => error_to_trap(env.store, &e),
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                format!("host function panicked: {s}")
            } else if let Some(s) = payload.downcast_ref::<String>() {
                format!("host function panicked: {s}")
            } else {
                "host function panicked".to_string()
            };
            error_to_trap(env.store, &Error::Message(msg))
        }
    }
}

/// Copies the arguments out of the C vector.
///
/// A zero-length vector arrives as `{size: 0, data: null}`, and
/// `from_raw_parts` needs a non-null pointer even for a zero length.
unsafe fn read_vals(ptr: *const sys::wasm_val_vec_t) -> Vec<Val> {
    let vals_vec = unsafe { &*ptr };
    if vals_vec.size == 0 || vals_vec.data.is_null() {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(vals_vec.data, vals_vec.size) }
        .iter()
        .map(|&v| Val::from(v))
        .collect()
}

/// Writes the results back into the C vector, up to the slots it has.
unsafe fn write_vals(ptr: *mut sys::wasm_val_vec_t, vals: &[Val]) {
    let vals_vec = unsafe { &mut *ptr };
    if vals_vec.size == 0 || vals_vec.data.is_null() {
        return;
    }
    let slots = unsafe { std::slice::from_raw_parts_mut(vals_vec.data, vals_vec.size) };
    for (slot, val) in slots.iter_mut().zip(vals) {
        *slot = (*val).into();
    }
}

/// Frees the payload. zwasm calls this once, when the store drops the func.
///
/// Dropping the closure here runs the `Drop` of whatever it captured, inside an
/// `extern "C"` frame — so a capture that panics on drop aborts the process.
/// Nothing can be done about that from here, and a panicking `Drop` is already
/// a program that cannot unwind cleanly.
unsafe extern "C" fn finalize(env: *mut c_void) {
    drop(Box::from_raw(env as *mut HostEnv));
}

/// Builds a `wasm_valtype_vec_t` holding one valtype per entry.
///
/// Ownership walks up: the vector takes every valtype handed to it, so a failure
/// is only ours to clean up at the step it happened on.
fn valtype_vec(types: &[ValType]) -> Result<sys::wasm_valtype_vec_t, Error> {
    let mut valtypes: Vec<*mut sys::wasm_valtype_t> = Vec::with_capacity(types.len());
    for ty in types {
        let valtype = unsafe { sys::wasm_valtype_new(ty.kind()) };
        if valtype.is_null() {
            for valtype in valtypes {
                unsafe { sys::wasm_valtype_delete(valtype) }
            }
            return Err(Error::Message("failed to create value type".to_string()));
        }
        valtypes.push(valtype);
    }
    let mut out = sys::wasm_valtype_vec_t {
        size: 0,
        data: std::ptr::null_mut(),
    };
    unsafe { sys::wasm_valtype_vec_new(&mut out, valtypes.len(), valtypes.as_ptr()) };
    if !types.is_empty() && out.data.is_null() {
        for valtype in valtypes {
            unsafe { sys::wasm_valtype_delete(valtype) }
        }
        return Err(Error::Message(
            "failed to create value type list".to_string(),
        ));
    }
    Ok(out)
}

/// Builds a `wasm_functype_t` from the two type lists.
///
/// `wasm_functype_new` takes both vectors only when it succeeds, so the null
/// path has to delete them — the same shape `wasm_globaltype_new` has in
/// `global.rs`. Deleting a vector frees its valtypes too.
fn new_functype(
    params: &[ValType],
    results: &[ValType],
) -> Result<*mut sys::wasm_functype_t, Error> {
    let mut params_vec = valtype_vec(params)?;
    let mut results_vec = match valtype_vec(results) {
        Ok(v) => v,
        Err(e) => {
            unsafe { sys::wasm_valtype_vec_delete(&mut params_vec) };
            return Err(e);
        }
    };
    let functype = unsafe { sys::wasm_functype_new(&mut params_vec, &mut results_vec) };
    if functype.is_null() {
        unsafe { sys::wasm_valtype_vec_delete(&mut params_vec) };
        unsafe { sys::wasm_valtype_vec_delete(&mut results_vec) };
        return Err(Error::Message("failed to create function type".to_string()));
    }
    Ok(functype)
}
