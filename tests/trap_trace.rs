//! Where a trap happened.
//!
//! zwasm 2.8.0 fills `wasm_trap_trace` on an interpreter-backed instance
//! (zwasm/zwasm#498); `Error::frames` is that list. The module offsets are
//! checked against the module's own bytes — the offset of an `unreachable` must
//! land on 0x00 and that of a `call` on 0x10 — so the tests need no external
//! disassembler to say whether the numbers are right.

use zwasm_sdk::{Engine, EngineKind, Error, Func, Instance, Module, Store, TrapKind};

// (module (func $f2 unreachable) (func $f1 call $f2) (func (export "f") call $f1))
const NESTED: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03, 0x04,
    0x03, 0x00, 0x00, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x02, 0x0a, 0x0f, 0x03, 0x03, 0x00,
    0x00, 0x0b, 0x04, 0x00, 0x10, 0x00, 0x0b, 0x04, 0x00, 0x10, 0x01, 0x0b,
];

// (module (import "env" "h" (func $h)) (func $mid call $h) (func (export "f") call $mid))
const HOST: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x02, 0x09,
    0x01, 0x03, 0x65, 0x6e, 0x76, 0x01, 0x68, 0x00, 0x00, 0x03, 0x03, 0x02, 0x00, 0x00, 0x07, 0x05,
    0x01, 0x01, 0x66, 0x00, 0x02, 0x0a, 0x0b, 0x02, 0x04, 0x00, 0x10, 0x00, 0x0b, 0x04, 0x00, 0x10,
    0x01, 0x0b,
];

// (module (func $r (param i32)
//           local.get 0 i32.eqz if unreachable end
//           local.get 0 i32.const 1 i32.sub call $r)
//         (func (export "f") i32.const 100 call $r))
//
// A hundred frames deep when it traps: more than the 64 zwasm keeps.
const DEEP: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x60, 0x01, 0x7f, 0x00, 0x60,
    0x00, 0x00, 0x03, 0x03, 0x02, 0x00, 0x01, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x01, 0x0a, 0x1a,
    0x02, 0x10, 0x00, 0x20, 0x00, 0x45, 0x04, 0x40, 0x00, 0x0b, 0x20, 0x00, 0x41, 0x01, 0x6b, 0x10,
    0x00, 0x0b, 0x07, 0x00, 0x41, 0xe4, 0x00, 0x10, 0x00, 0x0b,
];

const UNREACHABLE: u8 = 0x00;
const CALL: u8 = 0x10;

/// Instantiates `wasm` on `engine` with `imports`, calls its `f`, and returns
/// the trap it is expected to produce.
fn trap_on(wasm: &[u8], engine: EngineKind, imports: &[Func], store: &mut Store) -> Error {
    let module = Module::new(store, wasm).unwrap();
    let instance = Instance::new_with_engine(store, &module, imports, engine).unwrap();
    let f = instance.get_func(store, "f").unwrap();
    f.call(store, &[], &mut []).unwrap_err()
}

/// The opcode byte each frame points at, in trace order.
fn opcodes_at(wasm: &[u8], err: &Error) -> Vec<u8> {
    err.frames()
        .iter()
        .map(|frame| wasm[frame.module_offset()])
        .collect()
}

#[test]
fn an_interpreter_trap_lists_every_frame_innermost_first() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let err = trap_on(NESTED, EngineKind::Interp, &[], &mut store);

    assert_eq!(err.trap_kind(), Some(TrapKind::Unreachable));
    let indices: Vec<u32> = err.frames().iter().map(|f| f.func_index()).collect();
    assert_eq!(
        indices,
        [0, 1, 2],
        "$f2 trapped, called by $f1, called by f"
    );
}

#[test]
fn a_module_offset_points_at_the_opcode_that_was_executing() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let err = trap_on(NESTED, EngineKind::Interp, &[], &mut store);

    assert_eq!(opcodes_at(NESTED, &err), [UNREACHABLE, CALL, CALL]);
    // The function offset counts from the body's first byte (its local
    // declarations), so module offset minus function offset is where the body
    // starts: for these three bodies, one local-declaration byte before the
    // opcode.
    for frame in err.frames() {
        assert_eq!(frame.func_offset(), 1, "{frame:?}");
    }
}

// ADR-0232 D4: the JIT has no frame stack the runtime can read, so its traps
// carry nothing. Pinned so the doc's "empty on the JIT" stays measured rather
// than assumed.
#[test]
fn a_jit_trap_has_no_frames() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let err = trap_on(NESTED, EngineKind::Jit, &[], &mut store);

    assert_eq!(err.trap_kind(), Some(TrapKind::Unreachable));
    assert!(err.frames().is_empty(), "{:?}", err.frames());
}

// A trap a host function returns is a new failure at the guest's call into the
// host (zwasm's ADR-0232 D2): the trace starts there, and the host itself has
// no frame. The closure's own message does not come back through a guest —
// zwasm reports the kind's text, "host invocation error", on either engine
// (zwasm/zwasm#496) — so only the kind is asserted here; `host_func.rs` pins
// the message on a direct call, where it does survive.
#[test]
fn a_host_error_starts_its_trace_at_the_guest_call_site() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let host = Func::new(&mut store, &[], &[], |_, _| {
        Err(Error::Message("the host said no".into()))
    })
    .unwrap();
    let err = trap_on(HOST, EngineKind::Interp, &[host], &mut store);

    assert_eq!(err.trap_kind(), Some(TrapKind::BindingError));
    let indices: Vec<u32> = err.frames().iter().map(|f| f.func_index()).collect();
    assert_eq!(indices, [1, 2], "$mid, which called the host, then f");
    assert_eq!(opcodes_at(HOST, &err), [CALL, CALL]);
}

#[test]
fn a_host_error_with_no_guest_has_no_frames() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let host = Func::new(&mut store, &[], &[], |_, _| {
        Err(Error::Message("the host said no".into()))
    })
    .unwrap();

    let err = host.call(&mut store, &[], &mut []).unwrap_err();
    assert!(err.frames().is_empty(), "{:?}", err.frames());
}

// zwasm keeps 64 frames and drops the outermost beyond that (ADR-0232 D2), so a
// deep recursion's trace ends before the caller that started it. Pinned because
// a reader of a 64-frame trace with no entry point would otherwise take the
// list for complete.
//
// On its own thread with room to recurse: the interpreter nests a native call
// per guest call, and on the 2 MiB stack libtest gives a test it reports
// `call stack exhausted` before the hundredth level (measured: before the
// seventieth), which would make this a different trap with a different
// innermost frame. The engine is created inside the thread because it is not
// `Send`.
#[test]
fn a_trace_keeps_the_innermost_sixty_four_frames() {
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(|| {
            let engine = Engine::new().unwrap();
            let mut store = Store::new(&engine).unwrap();
            let err = trap_on(DEEP, EngineKind::Interp, &[], &mut store);

            assert_eq!(err.trap_kind(), Some(TrapKind::Unreachable));
            assert_eq!(err.frames().len(), 64);
            assert!(
                err.frames().iter().all(|f| f.func_index() == 0),
                "every kept frame is a $r; f, which started the recursion, fell off"
            );
            let opcodes = opcodes_at(DEEP, &err);
            assert_eq!(opcodes[0], UNREACHABLE);
            assert!(opcodes[1..].iter().all(|&op| op == CALL), "{opcodes:?}");
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn an_error_that_is_not_a_trap_has_no_frames() {
    let err = Error::Message("not a trap".into());
    assert!(err.frames().is_empty());
}
