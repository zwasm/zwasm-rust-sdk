use thiserror::Error;
use zwasm_sys as sys;

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
    /// `ZWASM_TRAP_INVALID_MODULE`. The JIT judged the module invalid, by a
    /// check the front-end validator does not make yet.
    ///
    /// Not a guest fault: nothing ran. It arrives from instantiation rather
    /// than from a call, and `AUTO` does not fall back to the interpreter for
    /// it — a module refused here is refused on the stock engine. The message
    /// names the verdict.
    InvalidModule,
    /// `ZWASM_TRAP_UNSUPPORTED`. The engine running this instance has no
    /// implementation for the shape of this call.
    ///
    /// Neither a guest fault nor a binding error, and `AUTO` does not retry on
    /// the interpreter at call time, so a different engine choice does not fix
    /// it at the point it appears. The message names the shape.
    Unsupported,
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
            19 => TrapKind::InvalidModule,
            20 => TrapKind::Unsupported,
            other => TrapKind::Unknown(other),
        }
    }
}

/// One frame a trap unwound: which function, and where in it.
///
/// Mirrors wasmtime's `FrameInfo` in what it has and parts from it in what it
/// lacks. There is no `module()`, because zwasm's `wasm_frame_instance` is
/// always null (an instance's runtime can outlive the instance), and no
/// `func_name()` or `symbols()`, because zwasm reads no name section. The two
/// offsets are not `Option`s: zwasm records no frame for a function it has no
/// offset table for, so a frame that exists always knows where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameInfo {
    func_index: u32,
    func_offset: usize,
    module_offset: usize,
}

impl FrameInfo {
    /// The function's index in the module's function index space, imports
    /// included.
    pub fn func_index(&self) -> u32 {
        self.func_index
    }

    /// The byte offset of the instruction that was executing, counted from the
    /// start of the function body (its local declarations). The same origin
    /// wasmtime's `FrameInfo::func_offset` uses.
    pub fn func_offset(&self) -> usize {
        self.func_offset
    }

    /// The byte offset of the instruction that was executing, counted from the
    /// start of the module. It points at the opcode itself, so the byte at this
    /// offset in the module's binary is the instruction, and the number lines
    /// up with `wasm-tools print --print-offsets`.
    pub fn module_offset(&self) -> usize {
        self.module_offset
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

    /// Guest execution trapped. Carries the message from `wasm_trap_message`
    /// and the frames from `wasm_trap_trace`, innermost first, which
    /// [`frames`](Error::frames) reads.
    ///
    /// The frames are filled on an interpreter-backed instance and empty on a
    /// JIT-backed one, for an exception no frame caught, and for a trap that
    /// never passed through a guest (a host function called directly whose
    /// closure returned `Err`). A trap a host function returns into a guest
    /// starts its trace at the guest's call into the host. At most 64 frames
    /// are kept, the outermost dropped first.
    #[error("{message}")]
    Trap {
        kind: TrapKind,
        message: String,
        frames: Vec<FrameInfo>,
    },

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
            Error::Trap { kind, .. } => Some(*kind),
            Error::Message(_) => None,
            Error::WasiExit { .. } => Some(TrapKind::WasiExit),
        }
    }

    /// The frames the trap unwound, innermost first.
    ///
    /// Empty when zwasm recorded none (see [`Error::Trap`] for when that is)
    /// and for an error that is not a trap. The first frame is the one that
    /// trapped, which is what `wasm_trap_origin` reports.
    pub fn frames(&self) -> &[FrameInfo] {
        match self {
            Error::Trap { frames, .. } => frames,
            Error::Message(_) => &[],
            Error::WasiExit { .. } => &[],
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

    // A readable status is what separates a guest that ended itself from one
    // that faulted; the kind only names the case and never carries the number,
    // so one read answers both questions and `wasi.h` says to prefer it.
    //
    // Reading the kind first instead was a workaround, from when the status
    // outlived the call that produced it. That is fixed (zwasm/zwasm#341, #345,
    // #352), and the tests below hold zwasm to it rather than the SDK stepping
    // around it.
    let mut code: u32 = 0;
    if sys::zwasm_store_wasi_exit_code(store.ptr, &mut code) {
        sys::wasm_trap_delete(trap);
        return Error::WasiExit { code };
    }

    let mut message = sys::wasm_message_t {
        size: 0,
        data: std::ptr::null_mut(),
    };
    sys::wasm_trap_message(trap, &mut message);
    let msg = if message.data.is_null() {
        "trap with no message".to_string()
    } else {
        // `wasm.h` counts the terminator in `size`, so the last byte is
        // dropped rather than shown. Asked rather than subtracted because an
        // unconditional `size - 1` would wrap on an empty message.
        let bytes = std::slice::from_raw_parts(message.data as *const u8, message.size);
        String::from_utf8_lossy(bytes.strip_suffix(&[0]).unwrap_or(bytes)).into_owned()
    };
    sys::wasm_byte_vec_delete(&mut message);

    // The frames are borrowed from the vector, so they are copied out before it
    // is freed; an empty vector is what zwasm gives when it recorded none.
    let mut trace = sys::wasm_frame_vec_t {
        size: 0,
        data: std::ptr::null_mut(),
    };
    sys::wasm_trap_trace(trap, &mut trace);
    let mut frames = Vec::with_capacity(trace.size);
    for i in 0..trace.size {
        let frame = *trace.data.add(i);
        frames.push(FrameInfo {
            func_index: sys::wasm_frame_func_index(frame),
            func_offset: sys::wasm_frame_func_offset(frame),
            module_offset: sys::wasm_frame_module_offset(frame),
        });
    }
    sys::wasm_frame_vec_delete(&mut trace);

    sys::wasm_trap_delete(trap);
    Error::Trap {
        kind: kind.into(),
        message: msg,
        frames,
    }
}

pub(crate) fn error_to_trap(store: *mut sys::wasm_store_t, error: &Error) -> *mut sys::wasm_trap_t {
    let msg = error.to_string();
    let bytes = msg.as_bytes();
    let message = sys::wasm_message_t {
        size: bytes.len(),
        data: bytes.as_ptr() as *mut _,
    };
    unsafe { sys::wasm_trap_new(store, &message) }
}

pub(crate) fn trap_into_result(trap: *mut sys::wasm_trap_t, store: &Store) -> Result<(), Error> {
    if trap.is_null() {
        Ok(())
    } else {
        Err(unsafe { trap_to_error(trap, store) })
    }
}
