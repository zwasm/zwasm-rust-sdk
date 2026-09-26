use std::cell::RefCell;
use std::os::raw::{c_char, c_void};
use std::rc::Rc;

use zwasm_sys as sys;

use crate::error::{non_null, Error, TrapKind};

/// One hook slot: `None` when nothing is installed.
///
/// The `RefCell` is not a choice. `Engine` is `Clone` over an `Rc`, so a setter
/// only ever has `&self` and can never produce a `&mut EngineInner`.
///
/// Spelled as an alias, and the five below with it, because
/// `RefCell<Option<Box<dyn Fn(..)>>>` written out trips `clippy::type_complexity`
/// on every field — the same reason `func.rs` names its `HostCallback`.
type Slot<T> = RefCell<Option<Box<T>>>;

type CompileHook = dyn Fn(usize, bool);
type InstantiateHook = dyn Fn(Option<u64>);
type TrapHook = dyn Fn(Option<u64>, TrapKind, &str);
type FuelExhaustedHook = dyn Fn(Option<u64>);
type MemoryGrowthHook = dyn Fn(Option<u64>, u32, u64, u64);

#[derive(Default)]
struct Hooks {
    compile: Slot<CompileHook>,
    instantiate: Slot<InstantiateHook>,
    trap: Slot<TrapHook>,
    fuel_exhausted: Slot<FuelExhaustedHook>,
    memory_growth: Slot<MemoryGrowthHook>,
}

/// What the `Rc` in every [`Engine`] handle points at.
///
/// Its address is what the C side gets as each hook's `user_data`: it sits in
/// an `Rc`, so it does not move, and the closures are swapped inside the
/// `RefCell`s rather than behind a raw pointer. The alternative — `Box::into_raw`
/// into `user_data`, which is what `func.rs` does — does not fit here, because
/// these setters take no finalizer the way `wasm_func_new_with_env` does: the
/// engine would have to remember the pointers and free them itself, and
/// replacing a hook would become an ordering problem.
struct EngineInner {
    ptr: *mut sys::wasm_engine_t,
    hooks: Hooks,
}

impl Drop for EngineInner {
    /// Deletes the C engine, and only then drops the hooks.
    ///
    /// That order is guaranteed rather than lucky: a `Drop::drop` body runs to
    /// completion before the fields' own drop glue does. So by the time a
    /// closure is dropped the engine that could have called it is gone, and
    /// clearing the slots here first would be dead work — do not add it.
    fn drop(&mut self) {
        unsafe {
            sys::wasm_engine_delete(self.ptr);
        }
    }
}

/// A compilation and runtime environment, wrapping `wasm_engine_t`.
///
/// One engine can back any number of [`Store`](crate::store::Store)s.
///
/// # One thread per process
///
/// An `Engine` is neither `Send` nor `Sync`, and the reason is stronger than it
/// looks. zwasm's engine is single-threaded per *process*, not per store: its
/// stores share process-global state — among it the table the exception
/// unwinder consults to find which instance owns a frame — so a second thread
/// deleting a store can free memory a call on the first is still reading, even
/// though the two share no handle. `include/zwasm.h` states this.
///
/// Keeping the handle off other threads is therefore necessary but not
/// sufficient: two engines created independently on two threads are no safer
/// than one shared between them. Nothing in this crate prevents that yet.
///
/// ```compile_fail
/// # use zwasm_sdk::Engine;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let engine = Engine::new()?;
/// std::thread::spawn(move || drop(engine)); // error: `Engine` cannot be sent between threads
/// # Ok(())
/// # }
/// ```
///
/// `Clone` is shallow: clones share one `wasm_engine_t`, and the C engine is
/// deleted only when the last of them is gone. Every store keeps a clone, so an
/// `Engine` value can be dropped while its stores are still in use. zwasm
/// resolves allocation through the store's engine back-pointer, which makes the
/// engine outliving its stores a requirement of the C API, not a convenience.
///
/// # Observability hooks
///
/// Five engine events can be listened to — a module offered for compilation, an
/// instantiation, a trap, a fuel budget running out, a memory growing — with
/// `set_*` to install a closure and `clear_*` to take it off again. These are
/// the rules all ten share; what each event does and does not promise is on its
/// own method.
///
/// Install them before the engine is used. The engine reads a slot with a plain
/// load and takes no lock, and clones share one C engine, so a hook set through
/// any handle is set for all of them and replacing one replaces it everywhere.
///
/// **A hook must not call back into the engine.** It fires in the middle of the
/// operation it reports, so reaching this engine, its stores or its instances
/// from inside one is undefined — record what is needed and return. That
/// includes the hook's own slot: a closure that tries to `clear_*` itself to
/// fire only once is *silently ignored* and fires again, because the slot is
/// borrowed for the length of the call. Count with a [`Cell`](std::cell::Cell)
/// instead.
///
/// **A panic inside a hook is swallowed.** The C callback returns `void`, so
/// unlike [`Func::new`](crate::func::Func::new) — where a panic becomes a trap
/// — there is nowhere for one to go. Aborting was the alternative, and taking
/// the process down for a bug in a metrics closure is not proportionate. A
/// collector that needs to know it failed has to catch inside its own closure.
///
/// **A hook reports an event, not a duration.** zwasm's core carries no clock,
/// so nothing says when something happened or how long it took; time it here.
///
/// **A closure that captures the engine leaks it.** The closures are owned by
/// the same allocation every [`Engine`] handle points at, so an `Engine` clone
/// captured by one points back at what owns it. Dropping every handle does not
/// break that cycle: the C engine is never deleted, and neither is anything
/// else the closure captured. Nothing needs to fire for this to happen —
/// installing the hook is enough — and this crate cannot prevent it, because
/// the strong reference is on the caller's side of the boundary.
///
/// Capture what the hook actually needs rather than the engine. For counting,
/// that is a [`Rc`]`<`[`Cell`](std::cell::Cell)`<_>>` or a sender. For telling
/// engines apart in metrics — the one reason to reach for an `Engine` here —
/// capture a label the embedder chose instead, which is cheaper than an engine
/// and cannot leak one. There is nothing else a hook may do with an engine:
/// reaching this one from inside a hook is undefined, so a captured handle is
/// unusable as well as costly.
///
/// There is a way out while a handle still exists: `clear_*` drops the closure
/// and the engine it captured with it, and so does replacing the hook with one
/// that captures nothing. What has no way back is dropping the last handle
/// first — nothing is then left to call.
///
/// An instance is named by an id rather than a handle, because an id is
/// monotonic for the engine's life and never reused where an address can be
/// handed out again after a delete. The C ABI's 0 for "no instance" arrives as
/// `None`. There is no way to turn an id back into an
/// [`Instance`](crate::instance::Instance) — zwasm offers none — so a hook that
/// needs more than the id has to have recorded it when the instance was made.
///
/// The closures need be neither `Send` nor `Sync`, for the reason
/// [`Func::new`](crate::func::Func::new)'s does not: nothing in this crate can
/// move a store to another thread, so requiring the bounds would only forbid
/// capturing an [`Rc`] and force an `Arc<Mutex<_>>` that buys
/// nothing.
#[derive(Clone)]
pub struct Engine {
    inner: Rc<EngineInner>,
}

impl Engine {
    /// Creates an engine.
    ///
    /// Fails only when the C side cannot allocate.
    pub fn new() -> Result<Self, Error> {
        let ptr = non_null(unsafe { sys::wasm_engine_new() }, "failed to create engine")?;
        Ok(Engine {
            inner: Rc::new(EngineInner {
                ptr,
                hooks: Hooks::default(),
            }),
        })
    }

    pub(crate) fn ptr(&self) -> *mut sys::wasm_engine_t {
        self.inner.ptr
    }

    /// Reports each module offered to the engine: how many bytes, and whether
    /// the engine took them.
    ///
    /// Raised by [`Module::new`](crate::module::Module::new), which is
    /// `wasm_module_new`, and by `wasm_module_validate`.
    ///
    /// # `false` is "not taken", not "malformed"
    ///
    /// The engine does not distinguish two cases here: bytes that parsing or
    /// validation rejected, and a resource failure while validating them. The
    /// same bytes offered again with memory available can be accepted, so a
    /// `false` is not proof that a module is bad.
    ///
    /// `true` is not a promise that a module exists either. An accepted
    /// `wasm_module_new` can still return NULL if the allocation for its copy
    /// fails, and `wasm_module_validate` never creates one.
    ///
    /// An empty byte vector is an offer of zero bytes rather than the absence
    /// of an offer: it is reported, with a length of 0, and rejected.
    ///
    /// See [`Engine`] for the rules every hook shares.
    pub fn set_compile_hook(&self, f: impl Fn(usize, bool) + 'static) {
        // The borrow is taken first and held across both writes, so the slot
        // and the C side never disagree about whether a hook is installed.
        // It is also what makes replacing one safe against a displaced
        // closure whose captures have destructors: the assignment below drops
        // the old closure while this borrow is held, so a `Drop` that reaches
        // back into the engine finds the slot taken and is ignored, rather
        // than clearing the closure that just replaced it.
        let Ok(mut slot) = self.inner.hooks.compile.try_borrow_mut() else {
            return;
        };
        *slot = Some(Box::new(f));
        unsafe {
            sys::zwasm_engine_set_compile_hook(
                self.inner.ptr,
                Some(compile_trampoline),
                Rc::as_ptr(&self.inner) as *mut c_void,
            );
        }
    }

    /// Takes the compile hook off, dropping the closure that was installed.
    ///
    /// Clearing a slot nothing is installed in does nothing.
    pub fn clear_compile_hook(&self) {
        // Both sides change together or neither does. Without the borrow
        // first, a hook clearing itself would unregister the C callback and
        // then fail to empty the slot, leaving the two disagreeing.
        let Ok(mut slot) = self.inner.hooks.compile.try_borrow_mut() else {
            return;
        };
        unsafe {
            sys::zwasm_engine_set_compile_hook(self.inner.ptr, None, std::ptr::null_mut());
        }
        *slot = None;
    }

    /// Reports each instantiation that succeeded, by the id it was given.
    ///
    /// A failed one raises nothing here. When it fails by trapping — a start
    /// function that traps, say — the trap hook reports that instead, so an
    /// instantiation can be visible there and never here.
    ///
    /// # The ids are not dense
    ///
    /// They are monotonic for the engine's life and never reused, but a failed
    /// instantiation consumes one too. So gaps are normal, and the highest id
    /// seen is not a count of instances — measured.
    ///
    /// See [`Engine`] for the rules every hook shares.
    pub fn set_instantiate_hook(&self, f: impl Fn(Option<u64>) + 'static) {
        // The borrow is taken first and held across both writes, so the slot
        // and the C side never disagree about whether a hook is installed.
        // It is also what makes replacing one safe against a displaced
        // closure whose captures have destructors: the assignment below drops
        // the old closure while this borrow is held, so a `Drop` that reaches
        // back into the engine finds the slot taken and is ignored, rather
        // than clearing the closure that just replaced it.
        let Ok(mut slot) = self.inner.hooks.instantiate.try_borrow_mut() else {
            return;
        };
        *slot = Some(Box::new(f));
        unsafe {
            sys::zwasm_engine_set_instantiate_hook(
                self.inner.ptr,
                Some(instantiate_trampoline),
                Rc::as_ptr(&self.inner) as *mut c_void,
            );
        }
    }

    /// Takes the instantiate hook off, dropping the closure that was installed.
    ///
    /// Clearing a slot nothing is installed in does nothing.
    pub fn clear_instantiate_hook(&self) {
        // Both sides change together or neither does. Without the borrow
        // first, a hook clearing itself would unregister the C callback and
        // then fail to empty the slot, leaving the two disagreeing.
        let Ok(mut slot) = self.inner.hooks.instantiate.try_borrow_mut() else {
            return;
        };
        unsafe {
            sys::zwasm_engine_set_instantiate_hook(self.inner.ptr, None, std::ptr::null_mut());
        }
        *slot = None;
    }

    /// Reports each trap the engine raised: which instance, what kind, and the
    /// message.
    ///
    /// The kind is the same value
    /// [`Error::trap_kind`](crate::error::Error::trap_kind) would carry, and
    /// the message the same text — with one difference. `wasm_trap_message`
    /// counts a NUL terminator in its size and this does not, so the `&str`
    /// here needs no trailing byte stripped off it.
    ///
    /// The `&str` is borrowed for the length of the call. Copy it to keep it.
    ///
    /// # Only traps the engine raised
    ///
    /// A trap the embedder minted with `wasm_trap_new` is its own and is not
    /// reported. That covers more than it sounds like: an `Err` returned from a
    /// [`Func::new`](crate::func::Func::new) closure becomes such a trap, and
    /// so does a panic out of one, so neither reaches this hook — measured.
    ///
    /// # An id here does not mean the instance exists
    ///
    /// A start function that traps is reported with the id of the instance
    /// being born, and that instantiation then fails, so the instantiate hook
    /// never emits that id. A collector keyed on instantiation has to tolerate
    /// an id it has not seen — measured.
    ///
    /// See [`Engine`] for the rules every hook shares.
    pub fn set_trap_hook(&self, f: impl Fn(Option<u64>, TrapKind, &str) + 'static) {
        // The borrow is taken first and held across both writes, so the slot
        // and the C side never disagree about whether a hook is installed.
        // It is also what makes replacing one safe against a displaced
        // closure whose captures have destructors: the assignment below drops
        // the old closure while this borrow is held, so a `Drop` that reaches
        // back into the engine finds the slot taken and is ignored, rather
        // than clearing the closure that just replaced it.
        let Ok(mut slot) = self.inner.hooks.trap.try_borrow_mut() else {
            return;
        };
        *slot = Some(Box::new(f));
        unsafe {
            sys::zwasm_engine_set_trap_hook(
                self.inner.ptr,
                Some(trap_trampoline),
                Rc::as_ptr(&self.inner) as *mut c_void,
            );
        }
    }

    /// Takes the trap hook off, dropping the closure that was installed.
    ///
    /// Clearing a slot nothing is installed in does nothing.
    pub fn clear_trap_hook(&self) {
        // Both sides change together or neither does. Without the borrow
        // first, a hook clearing itself would unregister the C callback and
        // then fail to empty the slot, leaving the two disagreeing.
        let Ok(mut slot) = self.inner.hooks.trap.try_borrow_mut() else {
            return;
        };
        unsafe {
            sys::zwasm_engine_set_trap_hook(self.inner.ptr, None, std::ptr::null_mut());
        }
        *slot = None;
    }

    /// Reports each instance whose fuel budget ran out.
    ///
    /// Raised *in addition* to a trap of kind
    /// [`TrapKind::OutOfFuel`] for the same
    /// instance, so a host that meters fuel need not switch on the kind to
    /// notice one.
    ///
    /// The other side of that: one exhaustion is two events. A collector
    /// watching both this and [`set_trap_hook`](Self::set_trap_hook) counts it
    /// twice unless it excludes the kind there.
    ///
    /// A budget is armed per instance with
    /// [`Instance::set_fuel`](crate::instance::Instance::set_fuel); without one
    /// nothing here ever fires.
    ///
    /// See [`Engine`] for the rules every hook shares.
    pub fn set_fuel_exhausted_hook(&self, f: impl Fn(Option<u64>) + 'static) {
        // The borrow is taken first and held across both writes, so the slot
        // and the C side never disagree about whether a hook is installed.
        // It is also what makes replacing one safe against a displaced
        // closure whose captures have destructors: the assignment below drops
        // the old closure while this borrow is held, so a `Drop` that reaches
        // back into the engine finds the slot taken and is ignored, rather
        // than clearing the closure that just replaced it.
        let Ok(mut slot) = self.inner.hooks.fuel_exhausted.try_borrow_mut() else {
            return;
        };
        *slot = Some(Box::new(f));
        unsafe {
            sys::zwasm_engine_set_fuel_exhausted_hook(
                self.inner.ptr,
                Some(fuel_exhausted_trampoline),
                Rc::as_ptr(&self.inner) as *mut c_void,
            );
        }
    }

    /// Takes the fuel-exhausted hook off, dropping the closure that was
    /// installed.
    ///
    /// Clearing a slot nothing is installed in does nothing.
    pub fn clear_fuel_exhausted_hook(&self) {
        // Both sides change together or neither does. Without the borrow
        // first, a hook clearing itself would unregister the C callback and
        // then fail to empty the slot, leaving the two disagreeing.
        let Ok(mut slot) = self.inner.hooks.fuel_exhausted.try_borrow_mut() else {
            return;
        };
        unsafe {
            sys::zwasm_engine_set_fuel_exhausted_hook(self.inner.ptr, None, std::ptr::null_mut());
        }
        *slot = None;
    }

    /// Reports each linear memory that grew, with the page counts either side
    /// of the growth.
    ///
    /// The arguments are the instance, the memory's index, the size before and
    /// the size after. Counts are in that memory's own page-size units, 64 KiB
    /// by default — not bytes.
    ///
    /// Both sides raise it: a guest's `memory.grow` and a host's
    /// [`Memory::grow`](crate::memory::Memory::grow).
    ///
    /// # A refused grow is not an event
    ///
    /// Failing to grow is the spec's recoverable `-1`, and nothing grew, so
    /// nothing is reported — a cap set with
    /// [`Instance::set_memory_pages_limit`](crate::instance::Instance::set_memory_pages_limit)
    /// is invisible here. This hook counts growth, not attempts.
    ///
    /// [`Memory::grow`](crate::memory::Memory::grow) also checks the module's
    /// declared maximum itself and returns `Err` before reaching the C call, so
    /// that refusal raises nothing either.
    ///
    /// # Which memories have an instance
    ///
    /// A memory belonging to an instance is named by it, including one the host
    /// reached through
    /// [`Instance::get_memory`](crate::instance::Instance::get_memory) — the
    /// handle is a copy, and the copy keeps the association. A memory the host
    /// made with [`Memory::new`](crate::memory::Memory::new) belongs to no
    /// instance and arrives as `None`. Both measured.
    ///
    /// See [`Engine`] for the rules every hook shares.
    pub fn set_memory_growth_hook(&self, f: impl Fn(Option<u64>, u32, u64, u64) + 'static) {
        // The borrow is taken first and held across both writes, so the slot
        // and the C side never disagree about whether a hook is installed.
        // It is also what makes replacing one safe against a displaced
        // closure whose captures have destructors: the assignment below drops
        // the old closure while this borrow is held, so a `Drop` that reaches
        // back into the engine finds the slot taken and is ignored, rather
        // than clearing the closure that just replaced it.
        let Ok(mut slot) = self.inner.hooks.memory_growth.try_borrow_mut() else {
            return;
        };
        *slot = Some(Box::new(f));
        unsafe {
            sys::zwasm_engine_set_memory_growth_hook(
                self.inner.ptr,
                Some(memory_growth_trampoline),
                Rc::as_ptr(&self.inner) as *mut c_void,
            );
        }
    }

    /// Takes the memory-growth hook off, dropping the closure that was
    /// installed.
    ///
    /// Clearing a slot nothing is installed in does nothing.
    pub fn clear_memory_growth_hook(&self) {
        // Both sides change together or neither does. Without the borrow
        // first, a hook clearing itself would unregister the C callback and
        // then fail to empty the slot, leaving the two disagreeing.
        let Ok(mut slot) = self.inner.hooks.memory_growth.try_borrow_mut() else {
            return;
        };
        unsafe {
            sys::zwasm_engine_set_memory_growth_hook(self.inner.ptr, None, std::ptr::null_mut());
        }
        *slot = None;
    }
}

impl Default for Engine {
    /// Creates an engine, panicking on failure.
    ///
    /// Use [`Engine::new`] to handle the allocation failure instead.
    fn default() -> Self {
        Self::new().expect("failed to create default Engine")
    }
}

/// Written out rather than derived: the field is an `Rc<EngineInner>`, so
/// deriving would need `EngineInner: Debug` and would nest one struct inside
/// another to say one thing.
///
/// That one thing is identity. An `Engine` is a handle whose clones share the
/// C engine, and the address is what shows two handles are the same engine.
impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("ptr", &self.inner.ptr)
            .finish()
    }
}

/// Calls the boxed closure on zwasm's behalf. The other four have this shape.
///
/// Everything is inside `catch_unwind`, because a panic crossing an
/// `extern "C"` boundary aborts the process. Unlike `func.rs`, which turns the
/// panic it catches into a trap, there is nothing to turn it into here: the C
/// callback returns `void`. So it is dropped, which `Engine`'s own docs say.
///
/// `try_borrow` rather than `borrow`, and the failure is a silent return. The
/// slot is only ever borrowed here, so the one way to find it taken is a hook
/// that re-entered the engine — which the header already calls undefined, and
/// which a panic could not report from inside a `catch_unwind` that drops it.
unsafe extern "C" fn compile_trampoline(user_data: *mut c_void, wasm_len: usize, accepted: bool) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // The `EngineInner` behind every `Engine`'s `Rc`. Alive for as long as
        // a hook can fire: deleting the C engine is this struct's own `Drop`.
        let inner = unsafe { &*(user_data as *const EngineInner) };
        let Ok(slot) = inner.hooks.compile.try_borrow() else {
            return;
        };

        let Some(f) = slot.as_deref() else { return };

        f(wasm_len, accepted);
    }));
}

unsafe extern "C" fn instantiate_trampoline(user_data: *mut c_void, raw_instance_id: u64) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let inner = unsafe { &*(user_data as *const EngineInner) };
        let Ok(slot) = inner.hooks.instantiate.try_borrow() else {
            return;
        };

        let Some(f) = slot.as_deref() else { return };

        f(instance_id(raw_instance_id));
    }));
}

unsafe extern "C" fn trap_trampoline(
    user_data: *mut c_void,
    raw_instance_id: u64,
    trap_kind: i32,
    message: *const c_char,
    message_len: usize,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let inner = unsafe { &*(user_data as *const EngineInner) };
        let Ok(slot) = inner.hooks.trap.try_borrow() else {
            return;
        };
        let Some(f) = slot.as_deref() else { return };

        // Borrowed for this call, and not NUL-terminated — the header's third
        // rule. `wasm_trap_message` counts a terminator in its size and
        // `error.rs` strips it back off; there is nothing to strip here.
        let bytes: &[u8] = if message.is_null() || message_len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(message as *const u8, message_len) }
        };
        let text = String::from_utf8_lossy(bytes);

        f(
            instance_id(raw_instance_id),
            TrapKind::from(trap_kind),
            &text,
        );
    }));
}

unsafe extern "C" fn fuel_exhausted_trampoline(user_data: *mut c_void, raw_instance_id: u64) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let inner = unsafe { &*(user_data as *const EngineInner) };
        let Ok(slot) = inner.hooks.fuel_exhausted.try_borrow() else {
            return;
        };

        let Some(f) = slot.as_deref() else { return };

        f(instance_id(raw_instance_id));
    }));
}

unsafe extern "C" fn memory_growth_trampoline(
    user_data: *mut c_void,
    raw_instance_id: u64,
    memory_index: u32,
    old_pages: u64,
    new_pages: u64,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let inner = unsafe { &*(user_data as *const EngineInner) };
        let Ok(slot) = inner.hooks.memory_growth.try_borrow() else {
            return;
        };

        let Some(f) = slot.as_deref() else { return };

        f(
            instance_id(raw_instance_id),
            memory_index,
            old_pages,
            new_pages,
        );
    }));
}

/// Maps the C ABI's "no instance" onto `None`.
///
/// zwasm reserves id 0 for an event that belongs to no instance — one raised
/// before any existed, or one for a memory the host made itself. Passing the 0
/// through would make it a plausible map key, and collect those events under an
/// instance that does not exist.
fn instance_id(raw: u64) -> Option<u64> {
    (raw != 0).then_some(raw)
}
