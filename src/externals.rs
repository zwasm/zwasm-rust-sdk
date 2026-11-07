use zwasm_sys as sys;

use crate::{module::ExternKind, Func, Global, Memory, Table};

/// Anything a module can import, mirroring `wasmtime::Extern`.
///
/// Each variant is the `Copy` handle the store owns, so an `Extern` costs what
/// the handle does, and the cross-store check in
/// [`Instance::new`](crate::instance::Instance::new) reads the handle's store
/// through it.
///
/// Non-exhaustive because [`ExternKind::Tag`] already names an import nothing
/// here can satisfy: the handle for it, when it comes, should not be a
/// breaking change.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub enum Extern {
    /// A function, from [`Func::new`] or an instance's exports.
    Func(Func),
    /// A global, from [`Global::new`] or an instance's exports.
    Global(Global),
    /// A table, from [`Table::new`] or an instance's exports.
    Table(Table),
    /// A memory, from [`Memory::new`] or an instance's exports.
    Memory(Memory),
}

impl Extern {
    /// Which kind of extern this is, in the terms
    /// [`Module::imports`](crate::module::Module::imports) reports, so a
    /// caller can check its list against the module's before instantiating.
    pub fn kind(&self) -> ExternKind {
        match self {
            Extern::Func(_) => ExternKind::Func,
            Extern::Global(_) => ExternKind::Global,
            Extern::Table(_) => ExternKind::Table,
            Extern::Memory(_) => ExternKind::Memory,
        }
    }

    /// The function inside, or `None` when this is not one.
    pub fn into_func(self) -> Option<Func> {
        match self {
            Extern::Func(f) => Some(f),
            _ => None,
        }
    }

    /// The global inside, or `None` when this is not one.
    pub fn into_global(self) -> Option<Global> {
        match self {
            Extern::Global(g) => Some(g),
            _ => None,
        }
    }

    /// The table inside, or `None` when this is not one.
    pub fn into_table(self) -> Option<Table> {
        match self {
            Extern::Table(t) => Some(t),
            _ => None,
        }
    }

    /// The memory inside, or `None` when this is not one.
    pub fn into_memory(self) -> Option<Memory> {
        match self {
            Extern::Memory(m) => Some(m),
            _ => None,
        }
    }

    pub(crate) fn store_id(&self) -> u64 {
        match self {
            Extern::Func(f) => f.store_id,
            Extern::Global(g) => g.store_id,
            Extern::Table(t) => t.store_id,
            Extern::Memory(m) => m.store_id,
        }
    }

    // The `wasm_extern_t` is a view zwasm allocates once per handle and keeps
    // until the store drops, so nothing here deletes it.
    pub(crate) fn as_raw(&self) -> *mut sys::wasm_extern_t {
        match self {
            Extern::Func(f) => unsafe { sys::wasm_func_as_extern(f.ptr) },
            Extern::Global(g) => unsafe { sys::wasm_global_as_extern(g.ptr) },
            Extern::Table(t) => unsafe { sys::wasm_table_as_extern(t.ptr) },
            Extern::Memory(m) => unsafe { sys::wasm_memory_as_extern(m.ptr) },
        }
    }
}

impl From<Func> for Extern {
    fn from(func: Func) -> Self {
        Extern::Func(func)
    }
}

impl From<Global> for Extern {
    fn from(global: Global) -> Self {
        Extern::Global(global)
    }
}

impl From<Table> for Extern {
    fn from(table: Table) -> Self {
        Extern::Table(table)
    }
}

impl From<Memory> for Extern {
    fn from(memory: Memory) -> Self {
        Extern::Memory(memory)
    }
}
