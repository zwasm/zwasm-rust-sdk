use zwasm_sys as sys;

use crate::{
    error::{non_null, Error},
    store::Store,
};

/// A linear memory, wrapping `wasm_memory_t`.
///
/// A handle into a [`Store`]; the store owns the C memory and frees it on its own
/// drop, so the handle is `Copy` and carries no destructor.
#[derive(Debug, Clone, Copy)]
pub struct Memory {
    pub(crate) ptr: *mut sys::wasm_memory_t,
    pub(crate) store_id: u64,
}

impl Memory {
    /// Creates a memory of `min` pages, growable to `max` pages.
    ///
    /// Sizes are in 64 KiB pages. `None` for `max` means no maximum.
    ///
    /// `Some(u32::MAX)` is indistinguishable from `None`: the wasm-c-api gives
    /// `wasm_limits_t` one `u32` for the maximum and reserves `u32::MAX` as its
    /// "no maximum" sentinel (`wasm_limits_max_default`, `wasm.h`), so the
    /// value cannot also mean a limit. Nothing is lost — that many 64 KiB pages
    /// is 256 TiB, and a wasm32 memory tops out at 65536 pages.
    pub fn new(store: &mut Store, min: u32, max: Option<u32>) -> Result<Self, Error> {
        let limits = sys::wasm_limits_t {
            min,
            max: max.unwrap_or(sys::wasm_limits_max_default),
        };
        let memorytype = non_null(
            unsafe { sys::wasm_memorytype_new(&limits) },
            "failed to create memory type",
        )?;
        let ptr = unsafe { sys::wasm_memory_new(store.ptr, memorytype) };
        unsafe { sys::wasm_memorytype_delete(memorytype) };

        let ptr = non_null(ptr, "failed to create memory")?;
        store.memories.push(ptr);

        Ok(Memory {
            ptr,
            store_id: store.id,
        })
    }

    /// Borrows the memory's bytes.
    ///
    /// The slice borrows the store, so anything that could move the backing
    /// buffer — [`Memory::grow`], or [`Func::call`](crate::func::Func::call)
    /// running guest code — is rejected by the borrow checker while it is held:
    ///
    /// ```compile_fail
    /// # use zwasm_sdk::{engine::Engine, store::Store, memory::Memory};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = Engine::new()?;
    /// let mut store = Store::new(&engine)?;
    /// let memory = Memory::new(&mut store, 1, Some(4))?;
    ///
    /// let bytes = memory.data(&store);
    /// memory.grow(&mut store, 1)?; // error: store is already borrowed
    /// println!("{}", bytes[0]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn data<'a>(&self, store: &'a Store) -> &'a [u8] {
        store.check(self.store_id);
        let data_ptr = unsafe { sys::wasm_memory_data(self.ptr) };
        if data_ptr.is_null() {
            return &[];
        }
        let data_size = unsafe { sys::wasm_memory_data_size(self.ptr) };
        unsafe { std::slice::from_raw_parts(data_ptr as *const u8, data_size) }
    }

    /// Borrows the memory's bytes mutably.
    ///
    /// The same invalidation rule as [`Memory::data`] applies.
    pub fn data_mut<'a>(&self, store: &'a mut Store) -> &'a mut [u8] {
        store.check(self.store_id);
        let data_ptr = unsafe { sys::wasm_memory_data(self.ptr) };
        if data_ptr.is_null() {
            return &mut [];
        }
        let data_size = unsafe { sys::wasm_memory_data_size(self.ptr) };
        unsafe { std::slice::from_raw_parts_mut(data_ptr as *mut u8, data_size) }
    }

    /// Grows the memory by `delta` pages and returns the previous size in pages.
    ///
    /// Fails when the result would exceed the maximum the memory was created
    /// with. zwasm's host-side `wasm_memory_grow` does not enforce that maximum
    /// ("v0.1: no max-pages check", `src/api/instance.zig`), so it is checked
    /// here against the memory's own type.
    pub fn grow(&self, store: &mut Store, delta: u32) -> Result<u32, Error> {
        store.check(self.store_id);
        let memory_type = unsafe { sys::wasm_memory_type(self.ptr) };
        if memory_type.is_null() {
            return Err(Error::Message(
                "failed to read the memory's type".to_string(),
            ));
        }
        let limits = unsafe { sys::wasm_memorytype_limits(memory_type) };
        let max = unsafe { (*limits).max };
        unsafe { sys::wasm_memorytype_delete(memory_type) };
        let size = unsafe { sys::wasm_memory_size(self.ptr) };
        if max != sys::wasm_limits_max_default
            && size.checked_add(delta).is_none_or(|total| total > max)
        {
            return Err(Error::Message("failed to grow memory".to_string()));
        }

        let result = unsafe { sys::wasm_memory_grow(self.ptr, delta) };
        if result {
            Ok(size)
        } else {
            Err(Error::Message("failed to grow memory".to_string()))
        }
    }

    /// Returns the current size in 64 KiB pages.
    ///
    /// For a byte count, take the length of [`Memory::data`].
    pub fn size(&self, store: &Store) -> u32 {
        store.check(self.store_id);
        unsafe { sys::wasm_memory_size(self.ptr) }
    }
}
