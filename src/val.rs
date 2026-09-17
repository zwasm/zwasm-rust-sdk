use zwasm_sys as sys;

/// A WebAssembly value, wrapping the tagged union `wasm_val_t`.
///
/// Only the four numeric types are modelled. Reference values (`funcref`,
/// `anyref`) round-trip through the C API but have no variant here, and `v128` is
/// absent from `wasm_val_t` itself by design of the wasm-c-api.
///
/// Converting a reference-kind `wasm_val_t` into a `Val` panics, so a function
/// returning one cannot be called through [`Func::call`](crate::func::Func::call)
/// yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Val {
    /// A 32 bit integer.
    I32(i32),
    /// A 64 bit integer.
    I64(i64),
    /// A 32 bit float.
    F32(f32),
    /// A 64 bit float.
    F64(f64),
}

impl Val {
    pub(crate) fn kind(&self) -> u8 {
        match self {
            Val::I32(_) => sys::wasm_valkind_enum_WASM_I32 as u8,
            Val::I64(_) => sys::wasm_valkind_enum_WASM_I64 as u8,
            Val::F32(_) => sys::wasm_valkind_enum_WASM_F32 as u8,
            Val::F64(_) => sys::wasm_valkind_enum_WASM_F64 as u8,
        }
    }
}

impl From<Val> for sys::wasm_val_t {
    fn from(val: Val) -> Self {
        match val {
            Val::I32(i) => sys::wasm_val_t {
                kind: sys::wasm_valkind_enum_WASM_I32 as u8,
                of: sys::wasm_val_t__bindgen_ty_1 { i32_: i },
            },
            Val::I64(i) => sys::wasm_val_t {
                kind: sys::wasm_valkind_enum_WASM_I64 as u8,
                of: sys::wasm_val_t__bindgen_ty_1 { i64_: i },
            },
            Val::F32(f) => sys::wasm_val_t {
                kind: sys::wasm_valkind_enum_WASM_F32 as u8,
                of: sys::wasm_val_t__bindgen_ty_1 { f32_: f },
            },
            Val::F64(f) => sys::wasm_val_t {
                kind: sys::wasm_valkind_enum_WASM_F64 as u8,
                of: sys::wasm_val_t__bindgen_ty_1 { f64_: f },
            },
        }
    }
}

impl From<sys::wasm_val_t> for Val {
    fn from(val: sys::wasm_val_t) -> Self {
        match val.kind {
            x if x == sys::wasm_valkind_enum_WASM_I32 as u8 => Val::I32(unsafe { val.of.i32_ }),
            x if x == sys::wasm_valkind_enum_WASM_I64 as u8 => Val::I64(unsafe { val.of.i64_ }),
            x if x == sys::wasm_valkind_enum_WASM_F32 as u8 => Val::F32(unsafe { val.of.f32_ }),
            x if x == sys::wasm_valkind_enum_WASM_F64 as u8 => Val::F64(unsafe { val.of.f64_ }),
            _ => panic!("Unknown wasm_val_t kind: {}", val.kind),
        }
    }
}
