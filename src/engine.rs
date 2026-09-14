use std::rc::Rc;

use zwasm_sys as sys;

use crate::error::{non_null, Error};

struct EngineInner {
    ptr: *mut sys::wasm_engine_t,
}

impl Drop for EngineInner {
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
            inner: Rc::new(EngineInner { ptr }),
        })
    }

    pub(crate) fn ptr(&self) -> *mut sys::wasm_engine_t {
        self.inner.ptr
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
