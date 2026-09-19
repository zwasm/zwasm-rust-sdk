use zwasm_sys as sys;

use crate::{
    error::{non_null, Error},
    store::Store,
};

/// What an import or export is, mirroring `wasm_externkind_enum` in the
/// wasm-c-api.
///
/// A tag, not a type: it says a slot wants a function without saying which
/// signature. The fuller shape wasmtime calls `ExternType` would carry that,
/// and nothing here needs it yet.
///
/// Non-exhaustive because the C enum is append-only: a kind added upstream
/// should not be a breaking change here.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternKind {
    /// `WASM_EXTERN_FUNC`.
    Func,
    /// `WASM_EXTERN_GLOBAL`.
    Global,
    /// `WASM_EXTERN_TABLE`.
    Table,
    /// `WASM_EXTERN_MEMORY`.
    Memory,
    /// `WASM_EXTERN_TAG`, from the exception-handling proposal.
    Tag,
    /// A kind this crate does not know about, carrying the raw value.
    Unknown(u8),
}

impl From<u8> for ExternKind {
    fn from(kind: u8) -> Self {
        match kind {
            0 => ExternKind::Func,
            1 => ExternKind::Global,
            2 => ExternKind::Table,
            3 => ExternKind::Memory,
            4 => ExternKind::Tag,
            _ => ExternKind::Unknown(kind),
        }
    }
}

/// A compiled and validated module, wrapping `wasm_module_t`.
///
/// A handle into a [`Store`]; the store owns the C module and frees it on its own
/// drop, so the handle is `Copy` and carries no destructor. A module holds no
/// runtime state, so one module can back several
/// [`Instance`](crate::instance::Instance)s.
///
/// Unlike wasmtime, where a module belongs to an engine and can be instantiated
/// in any store, the wasm-c-api ties a module to the store it was created in.
#[derive(Debug, Clone, Copy)]
pub struct Module {
    pub(crate) ptr: *mut sys::wasm_module_t,
    pub(crate) store_id: u64,
}

impl Module {
    /// Decodes and validates `wasm_bytes`.
    ///
    /// The bytes are copied, so they do not have to outlive the module. Returns an
    /// error when the input is not a valid module; the C API reports no reason, so
    /// the message is generic.
    pub fn new(store: &mut Store, wasm_bytes: &[u8]) -> Result<Self, Error> {
        let binary = sys::wasm_byte_vec_t {
            size: wasm_bytes.len(),
            data: wasm_bytes.as_ptr() as *mut _,
        };
        let ptr = non_null(
            unsafe { sys::wasm_module_new(store.ptr, &binary) },
            "failed to create module",
        )?;
        store.modules.push(ptr);
        Ok(Module {
            ptr,
            store_id: store.id,
        })
    }

    /// The module's import section, in declaration order.
    ///
    /// [`Instance::new`](crate::instance::Instance::new) takes its imports by
    /// position, so this is what lets a caller order its own functions without
    /// having read the module's source. The order here is that order.
    ///
    /// Returns an empty `Vec` for a module that imports nothing; there is no
    /// failure to report.
    ///
    /// Unlike wasmtime's `Module::imports`, which lends names out of the
    /// module, these are owned: the names live in a C vector this call has to
    /// free before returning, so they are copied rather than borrowed.
    ///
    /// A name that is not valid UTF-8 arrives as U+FFFD rather than an error.
    /// The binary format rules such a name out, but zwasm decodes it anyway
    /// (zwasm/zwasm#474), so the conversion is lossy where the spec says
    /// nothing should need converting.
    ///
    /// # Panics
    ///
    /// Panics when `self` belongs to a different store.
    pub fn imports(&self, store: &Store) -> Vec<ImportType> {
        store.check(self.store_id);
        let mut imports = sys::wasm_importtype_vec_t {
            size: 0,
            data: std::ptr::null_mut(),
        };
        unsafe { sys::wasm_module_imports(self.ptr, &mut imports) };
        let mut result = Vec::new();
        for i in 0..imports.size {
            let importtype = unsafe { *imports.data.add(i) };
            let module = unsafe { &*sys::wasm_importtype_module(importtype) };
            let name = unsafe { &*sys::wasm_importtype_name(importtype) };
            let kind =
                unsafe { sys::wasm_externtype_kind(sys::wasm_importtype_type(importtype)) }.into();
            result.push(ImportType {
                module: String::from_utf8_lossy(name_bytes(module)).into_owned(),
                name: String::from_utf8_lossy(name_bytes(name)).into_owned(),
                kind,
            })
        }
        unsafe { sys::wasm_importtype_vec_delete(&mut imports) };
        result
    }
}

/// One entry of a module's import section: what a module asks to be given,
/// named the way the module names it.
///
/// The names are owned rather than borrowed from the module, which is where
/// this parts from wasmtime's `ImportType<'module>` — see
/// [`Module::imports`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportType {
    module: String,
    name: String,
    kind: ExternKind,
}

impl ImportType {
    /// The module name the import is expected to come from — the first half of
    /// `env::h`. Can be empty; an empty name is a valid one.
    pub fn module(&self) -> &str {
        &self.module
    }

    /// The field name within that module — the second half of `env::h`. Can
    /// also be empty.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What kind of thing the slot wants.
    ///
    /// Named `kind` rather than wasmtime's `ty` because it is a tag: it says
    /// "a function" without saying which signature.
    pub fn kind(&self) -> ExternKind {
        self.kind
    }
}

/// Borrows a `wasm_name_t`'s bytes, tolerating the empty-name representation.
///
/// An empty name comes back as `{size: 0, data: null}` (zwasm `vecNew`,
/// `src/api/vec.zig`), and `from_raw_parts` needs a non-null pointer even for a
/// zero length.
///
/// Not the rule for every `wasm_byte_vec_t`. A trap message is the same C type
/// and `src/error.rs` reads it differently, stripping a trailing NUL that
/// `wasm.h` counts in the size — a name carries no terminator, so folding the
/// two together would put a NUL back on every trap message.
pub(crate) fn name_bytes(name: &sys::wasm_name_t) -> &[u8] {
    if name.size == 0 || name.data.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(name.data as *const u8, name.size) }
    }
}
