use thiserror::Error;
use zwasm_sys::{self as sys};

use crate::store::Store;

/// What a guest trap was, beside the message it carries.
///
/// The variants mirror `ZWASM_TRAP_*` in zwasm's `include/zwasm.h` one to one,
/// so the mapping stays auditable against the header. Where wasmtime has an
/// equivalent it is named on the variant; the two sets do not coincide, which
/// is why the C names win here.
///
/// Non-exhaustive because zwasm documents the enum as append-only stable: a
/// kind added upstream should not be a breaking change here.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapKind {
    /// `ZWASM_TRAP_BINDING_ERROR`.
    BindingError,
    /// `ZWASM_TRAP_UNREACHABLE`. wasmtime calls this `UnreachableCodeReached`.
    Unreachable,
    /// `ZWASM_TRAP_DIV_BY_ZERO`. wasmtime calls this `IntegerDivisionByZero`.
    DivByZero,
    /// `ZWASM_TRAP_INT_OVERFLOW`. wasmtime calls this `IntegerOverflow`.
    IntOverflow,
    /// `ZWASM_TRAP_INVALID_CONVERSION`. wasmtime calls this `BadConversionToInteger`.
    InvalidConversion,
    /// `ZWASM_TRAP_OOB_MEMORY`. wasmtime calls this `MemoryOutOfBounds`.
    OobMemory,
    /// `ZWASM_TRAP_OOB_TABLE`. wasmtime calls this `TableOutOfBounds`.
    OobTable,
    /// `ZWASM_TRAP_UNINITIALIZED_ELEM`. wasmtime calls this `IndirectCallToNull`.
    UninitializedElem,
    /// `ZWASM_TRAP_INDIRECT_CALL_MISMATCH`. wasmtime calls this `BadSignature`.
    IndirectCallMismatch,
    /// `ZWASM_TRAP_STACK_OVERFLOW`. wasmtime calls this `StackOverflow`.
    StackOverflow,
    /// `ZWASM_TRAP_OUT_OF_MEMORY`.
    OutOfMemory,
    /// `ZWASM_TRAP_NULL_REFERENCE`. wasmtime calls this `NullReference`.
    NullReference,
    /// `ZWASM_TRAP_CAST_FAILURE`. wasmtime calls this `CastFailure`.
    CastFailure,
    /// `ZWASM_TRAP_UNCAUGHT_EXCEPTION`.
    UncaughtException,
    /// `ZWASM_TRAP_UNALIGNED_ATOMIC`. wasmtime calls this `HeapMisaligned`.
    UnalignedAtomic,
    /// `ZWASM_TRAP_EXPECTED_SHARED_MEMORY`. wasmtime calls this `AtomicWaitNonSharedMemory`.
    ExpectedSharedMemory,
    /// `ZWASM_TRAP_INTERRUPTED`. wasmtime calls this `Interrupt`.
    Interrupted,
    /// `ZWASM_TRAP_OUT_OF_FUEL`. wasmtime calls this `OutOfFuel`.
    OutOfFuel,
    /// `ZWASM_TRAP_WASI_EXIT`. The guest called WASI `proc_exit`, so the trap
    /// reports a guest that ended itself rather than a guest that faulted.
    ///
    /// This kind does not mean failure. A WASI command reaches `proc_exit`
    /// even when it succeeds — a wasi-libc `_start` that returns normally
    /// calls `proc_exit(0)` — so a clean run arrives here too, and the status
    /// that says which it was is not carried by the kind — [`Error::WasiExit`]
    /// carries it.
    ///
    /// wasmtime has no trap code for this: it surfaces the same event as an
    /// `I32Exit` error carrying the status, not as a trap.
    WasiExit,
    /// A kind this crate does not know about.
    ///
    /// Reached when the linked zwasm reports a kind added after this crate's
    /// conversion was written — a bumped submodule, say. Carrying the raw
    /// value keeps that from being a panic.
    Unknown(i32),
}

impl From<i32> for TrapKind {
    fn from(code: i32) -> Self {
        match code {
            0 => TrapKind::BindingError,
            1 => TrapKind::Unreachable,
            2 => TrapKind::DivByZero,
            3 => TrapKind::IntOverflow,
            4 => TrapKind::InvalidConversion,
            5 => TrapKind::OobMemory,
            6 => TrapKind::OobTable,
            7 => TrapKind::UninitializedElem,
            8 => TrapKind::IndirectCallMismatch,
            9 => TrapKind::StackOverflow,
            10 => TrapKind::OutOfMemory,
            11 => TrapKind::NullReference,
            12 => TrapKind::CastFailure,
            13 => TrapKind::UncaughtException,
            14 => TrapKind::UnalignedAtomic,
            15 => TrapKind::ExpectedSharedMemory,
            16 => TrapKind::Interrupted,
            17 => TrapKind::OutOfFuel,
            18 => TrapKind::WasiExit,
            other => TrapKind::Unknown(other),
        }
    }
}

/// Anything that can go wrong in this crate.
#[derive(Error, Debug)]
pub enum Error {
    /// An operation failed without producing a trap.
    ///
    /// Most of the C API reports failure as a null pointer or `false` and carries
    /// no reason, so these messages are written here rather than taken from zwasm.
    #[error("{0}")]
    Message(String),

    /// Guest execution trapped. Carries the message from `wasm_trap_message`.
    #[error("{message}")]
    Trap { kind: TrapKind, message: String },

    /// The guest ended itself through WASI `proc_exit`, asking for `code`.
    ///
    /// Not a failure. A WASI command reaches `proc_exit` even when it succeeds
    /// — a wasi-libc `_start` that returns normally calls `proc_exit(0)` — so a
    /// clean run arrives here too, and `code` is what says which it was.
    ///
    /// wasmtime reports the same event as `I32Exit`.
    #[error("exited with status {code}")]
    WasiExit { code: u32 },
}

impl Error {
    pub fn trap_kind(&self) -> Option<TrapKind> {
        match self {
            Error::Trap { kind, message: _ } => Some(*kind),
            Error::Message(_) => None,
            Error::WasiExit { .. } => Some(TrapKind::WasiExit),
        }
    }
}

pub(crate) fn non_null<T>(ptr: *mut T, msg: &str) -> Result<*mut T, Error> {
    if ptr.is_null() {
        Err(Error::Message(msg.to_string()))
    } else {
        Ok(ptr)
    }
}

pub(crate) unsafe fn trap_to_error(trap: *mut sys::wasm_trap_t, store: &Store) -> Error {
    let kind = sys::zwasm_trap_kind(trap);
    if kind == sys::ZWASM_TRAP_WASI_EXIT as i32 {
        let mut code: u32 = 0;
        if sys::zwasm_store_wasi_exit_code(store.ptr, &mut code) {
            sys::wasm_trap_delete(trap);
            return Error::WasiExit { code };
        }
    }

    let mut message = sys::wasm_message_t {
        size: 0,
        data: std::ptr::null_mut(),
    };
    sys::wasm_trap_message(trap, &mut message);
    let msg = if message.data.is_null() {
        "trap with no message".to_string()
    } else {
        String::from_utf8_lossy(std::slice::from_raw_parts(
            message.data as *const u8,
            message.size,
        ))
        .to_string()
    };
    sys::wasm_byte_vec_delete(&mut message);
    sys::wasm_trap_delete(trap);
    Error::Trap {
        kind: kind.into(),
        message: msg,
    }
}

pub(crate) fn trap_into_result(trap: *mut sys::wasm_trap_t, store: &Store) -> Result<(), Error> {
    if trap.is_null() {
        Ok(())
    } else {
        Err(unsafe { trap_to_error(trap, store) })
    }
}
