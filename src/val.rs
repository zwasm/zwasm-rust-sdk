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

/// A value type, for declaring a host function's signature to
/// [`Func::new`](crate::func::Func::new).
///
/// The same four types [`Val`] carries. Reference types have none here either,
/// and for the same reason — see #45.
///
/// Non-exhaustive because that list is expected to grow.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    /// A 32 bit integer.
    I32,
    /// A 64 bit integer.
    I64,
    /// A 32 bit float.
    F32,
    /// A 64 bit float.
    F64,
}

impl ValType {
    /// A zero of this type, for initialising a result slot.
    ///
    /// Filling every slot with `Val::I32(0)` instead would hand an `f64` result
    /// the wrong kind before the callback has written anything.
    pub(crate) fn zero(self) -> Val {
        match self {
            ValType::I32 => Val::I32(0),
            ValType::I64 => Val::I64(0),
            ValType::F32 => Val::F32(0.0),
            ValType::F64 => Val::F64(0.0),
        }
    }

    pub(crate) fn kind(self) -> u8 {
        match self {
            ValType::I32 => sys::wasm_valkind_enum_WASM_I32 as u8,
            ValType::I64 => sys::wasm_valkind_enum_WASM_I64 as u8,
            ValType::F32 => sys::wasm_valkind_enum_WASM_F32 as u8,
            ValType::F64 => sys::wasm_valkind_enum_WASM_F64 as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `kind` is `pub(crate)`, so this is the only place the mapping can be
    // compared against the C constants rather than restated.
    #[test]
    fn valtype_kinds_are_the_c_constants() {
        let pairs: &[(ValType, u32)] = &[
            (ValType::I32, sys::wasm_valkind_enum_WASM_I32),
            (ValType::I64, sys::wasm_valkind_enum_WASM_I64),
            (ValType::F32, sys::wasm_valkind_enum_WASM_F32),
            (ValType::F64, sys::wasm_valkind_enum_WASM_F64),
        ];
        for &(ty, kind) in pairs {
            assert_eq!(ty.kind(), kind as u8, "{ty:?}");
        }
    }

    // A `Val` reports the kind its `ValType` declares, which is what the
    // trampoline's result check compares.
    #[test]
    fn a_val_reports_the_kind_of_its_type() {
        let pairs: &[(Val, ValType)] = &[
            (Val::I32(0), ValType::I32),
            (Val::I64(0), ValType::I64),
            (Val::F32(0.0), ValType::F32),
            (Val::F64(0.0), ValType::F64),
        ];
        for (val, ty) in pairs {
            assert_eq!(val.kind(), ty.kind(), "{ty:?}");
        }
    }
}
