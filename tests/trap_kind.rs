//! `Error::Trap` carries a machine-readable kind beside the message, so a host
//! can tell a guest trap apart from a cancellation or a fuel exhaustion without
//! matching on the message text.
//!
//! Every kind asserted here was measured against zwasm 2.5.0 through
//! `zwasm_trap_kind`, so these pin the mapping rather than restating it.

use zwasm_sdk::engine::Engine;
use zwasm_sdk::error::{Error, TrapKind};
use zwasm_sdk::instance::Instance;
use zwasm_sdk::module::Module;
use zwasm_sdk::store::Store;
use zwasm_sdk::val::Val;

// (module (func (export "f") unreachable))
const UNREACHABLE_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03, 0x02,
    0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x05, 0x01, 0x03, 0x00, 0x00, 0x0b,
];

// (module (func (export "f") (result i32) (i32.div_s (i32.const 1) (i32.const 0))))
const DIV_BY_ZERO_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
    0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x09, 0x01, 0x07, 0x00, 0x41,
    0x01, 0x41, 0x00, 0x6d, 0x0b,
];

// (module (func (export "f") (result i32) (i32.div_s (i32.const -2147483648) (i32.const -1))))
const INT_OVERFLOW_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
    0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x0d, 0x01, 0x0b, 0x00, 0x41,
    0x80, 0x80, 0x80, 0x80, 0x78, 0x41, 0x7f, 0x6d, 0x0b,
];

// (module (memory 1) (func (export "f") (result i32) (i32.load (i32.const 65536))))
const OOB_MEMORY_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
    0x02, 0x01, 0x00, 0x05, 0x03, 0x01, 0x00, 0x01, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a,
    0x0b, 0x01, 0x09, 0x00, 0x41, 0x80, 0x80, 0x04, 0x28, 0x02, 0x00, 0x0b,
];

// (module (table 1 funcref)
//   (func (export "f") (result i32) (call_indirect (type 0) (i32.const 0))))
const UNINIT_ELEM_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
    0x02, 0x01, 0x00, 0x04, 0x04, 0x01, 0x70, 0x00, 0x01, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00,
    0x0a, 0x09, 0x01, 0x07, 0x00, 0x41, 0x00, 0x11, 0x00, 0x00, 0x0b,
];

/// Calls the module's `f` export, which is expected to trap.
fn trap(wasm: &[u8]) -> Error {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, wasm).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let func = instance.get_func(&mut store, "f").unwrap();

    let mut results = vec![Val::I32(0); func.result_arity(&store)];
    func.call(&mut store, &[], &mut results).unwrap_err()
}

fn kind(wasm: &[u8]) -> TrapKind {
    trap(wasm).trap_kind().expect("expected a trap")
}

#[test]
fn unreachable_has_its_own_kind() {
    assert_eq!(kind(UNREACHABLE_WASM), TrapKind::Unreachable);
}

#[test]
fn division_by_zero_has_its_own_kind() {
    assert_eq!(kind(DIV_BY_ZERO_WASM), TrapKind::DivByZero);
}

#[test]
fn integer_overflow_has_its_own_kind() {
    assert_eq!(kind(INT_OVERFLOW_WASM), TrapKind::IntOverflow);
}

#[test]
fn out_of_bounds_memory_has_its_own_kind() {
    assert_eq!(kind(OOB_MEMORY_WASM), TrapKind::OobMemory);
}

#[test]
fn uninitialized_element_has_its_own_kind() {
    assert_eq!(kind(UNINIT_ELEM_WASM), TrapKind::UninitializedElem);
}

// The kind is carried beside the message, not instead of it, so a caller that
// only prints the error sees no change.
#[test]
fn the_message_survives_alongside_the_kind() {
    let err = trap(DIV_BY_ZERO_WASM);
    assert_eq!(err.to_string(), "integer divide by zero");
    assert_eq!(err.trap_kind(), Some(TrapKind::DivByZero));
}

// zwasm documents its TrapKind enum as append-only stable, so a kind this
// crate does not know about has to round-trip rather than panic. The case that
// makes this real is bumping the submodule to a zwasm that added a kind
// without updating the conversion here.
#[test]
fn an_unknown_kind_round_trips() {
    assert_eq!(TrapKind::from(99), TrapKind::Unknown(99));
    assert_eq!(TrapKind::from(-1), TrapKind::Unknown(-1));
}

// Each ZWASM_TRAP_* constant maps to the variant named after it. The numbers
// come from the bindings rather than being written out again here, so bumping
// the submodule to a zwasm that renumbers a kind breaks this test instead of
// silently mislabelling traps at runtime.
//
// A table cannot catch an *added* kind — it has nothing to compare against.
// `every_kind_the_header_declares_has_a_variant` covers that half.
#[test]
fn every_constant_maps_to_the_variant_named_after_it() {
    use zwasm_sys as sys;

    let pairs: &[(u32, TrapKind)] = &[
        (sys::ZWASM_TRAP_BINDING_ERROR, TrapKind::BindingError),
        (sys::ZWASM_TRAP_UNREACHABLE, TrapKind::Unreachable),
        (sys::ZWASM_TRAP_DIV_BY_ZERO, TrapKind::DivByZero),
        (sys::ZWASM_TRAP_INT_OVERFLOW, TrapKind::IntOverflow),
        (
            sys::ZWASM_TRAP_INVALID_CONVERSION,
            TrapKind::InvalidConversion,
        ),
        (sys::ZWASM_TRAP_OOB_MEMORY, TrapKind::OobMemory),
        (sys::ZWASM_TRAP_OOB_TABLE, TrapKind::OobTable),
        (
            sys::ZWASM_TRAP_UNINITIALIZED_ELEM,
            TrapKind::UninitializedElem,
        ),
        (
            sys::ZWASM_TRAP_INDIRECT_CALL_MISMATCH,
            TrapKind::IndirectCallMismatch,
        ),
        (sys::ZWASM_TRAP_STACK_OVERFLOW, TrapKind::StackOverflow),
        (sys::ZWASM_TRAP_OUT_OF_MEMORY, TrapKind::OutOfMemory),
        (sys::ZWASM_TRAP_NULL_REFERENCE, TrapKind::NullReference),
        (sys::ZWASM_TRAP_CAST_FAILURE, TrapKind::CastFailure),
        (
            sys::ZWASM_TRAP_UNCAUGHT_EXCEPTION,
            TrapKind::UncaughtException,
        ),
        (sys::ZWASM_TRAP_UNALIGNED_ATOMIC, TrapKind::UnalignedAtomic),
        (
            sys::ZWASM_TRAP_EXPECTED_SHARED_MEMORY,
            TrapKind::ExpectedSharedMemory,
        ),
        (sys::ZWASM_TRAP_INTERRUPTED, TrapKind::Interrupted),
        (sys::ZWASM_TRAP_OUT_OF_FUEL, TrapKind::OutOfFuel),
        (sys::ZWASM_TRAP_WASI_EXIT, TrapKind::WasiExit),
    ];

    for &(code, expected) in pairs {
        assert_eq!(
            TrapKind::from(code as i32),
            expected,
            "ZWASM_TRAP_* constant {code} maps to the wrong variant"
        );
    }
}

/// The header is the authority on how many kinds exist, so the sweep reads it.
///
/// Bounding the sweep by the table above instead makes the check satisfy
/// itself: when zwasm appends a kind, the old bound still passes and the new
/// constant falls silently to `TrapKind::Unknown`. `ZWASM_TRAP_WASI_EXIT` (18)
/// is the case that proved it — the whole suite stayed green while a clean WASI
/// exit was reported as an unknown trap.
#[test]
fn every_kind_the_header_declares_has_a_variant() {
    let header = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/zwasm-sys/zwasm/include/zwasm.h"
    );
    let text =
        std::fs::read_to_string(header).unwrap_or_else(|e| panic!("cannot read {header}: {e}"));

    let mut declared = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("#define ZWASM_TRAP_") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let (Some(suffix), Some(value)) = (parts.next(), parts.next()) else {
            continue;
        };
        let value: i32 = value.parse().unwrap_or_else(|e| {
            panic!("ZWASM_TRAP_{suffix} has a non-integer value {value:?}: {e}")
        });
        declared.push((suffix.to_string(), value));
    }

    // A parse that silently matched nothing would pass for the wrong reason.
    assert!(
        !declared.is_empty(),
        "no ZWASM_TRAP_* defines found in {header}"
    );

    let unmapped: Vec<String> = declared
        .iter()
        .filter(|(_, value)| matches!(TrapKind::from(*value), TrapKind::Unknown(_)))
        .map(|(suffix, value)| format!("ZWASM_TRAP_{suffix} = {value}"))
        .collect();

    assert!(
        unmapped.is_empty(),
        "the header declares kinds TrapKind maps to Unknown: {}",
        unmapped.join(", ")
    );
}

// Failures that are not guest traps carry no kind.
#[test]
fn a_non_trap_error_has_no_kind() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let err = Module::new(&mut store, &[0x00, 0x00, 0x00, 0x00]).unwrap_err();
    assert_eq!(err.trap_kind(), None);
}
