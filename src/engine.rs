use std::sync::Arc;

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

unsafe impl Send for EngineInner {}
unsafe impl Sync for EngineInner {}

/// A compilation and runtime environment, wrapping `wasm_engine_t`.
///
/// One engine can back any number of [`Store`](crate::store::Store)s. It holds no
/// per-instance state, so it is `Send + Sync` and can be shared across threads.
///
/// `Clone` is shallow: clones share one `wasm_engine_t`, and the C engine is
/// deleted only when the last of them is gone. Every store keeps a clone, so an
/// `Engine` value can be dropped while its stores are still in use. zwasm
/// resolves allocation through the store's engine back-pointer, which makes the
/// engine outliving its stores a requirement of the C API, not a convenience.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<EngineInner>,
}

impl Engine {
    /// Creates an engine.
    ///
    /// Fails only when the C side cannot allocate.
    pub fn new() -> Result<Self, Error> {
        let ptr = non_null(unsafe { sys::wasm_engine_new() }, "failed to create engine")?;
        Ok(Engine {
            inner: Arc::new(EngineInner { ptr }),
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

/// Written out rather than derived: the field is an `Arc<EngineInner>`, so
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
