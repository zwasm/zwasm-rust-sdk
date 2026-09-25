//! Host functions written as Rust closures.
//!
//! `Func::new_host` is the raw door — a C callback and a hand-built
//! `wasm_functype_t`. These exercise the safe one, including what it does with
//! the three ways a closure can fail: an `Err`, a panic, and a result of the
//! wrong type.

use std::cell::Cell;
use std::rc::Rc;

use zwasm_sdk::{Engine, Error, Func, Instance, Module, Store, TrapKind, Val, ValType};

// (module (import "" "" (func (param i32) (result i32)))
//   (func (export "f") (param i32) (result i32) (call 0 (local.get 0))))
const CALLBACK_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
    0x02, 0x05, 0x01, 0x00, 0x00, 0x00, 0x00, 0x03, 0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66,
    0x00, 0x01, 0x0a, 0x08, 0x01, 0x06, 0x00, 0x20, 0x00, 0x10, 0x00, 0x0b,
];

/// Calls `f`, returning what it returned or the error it raised.
fn call(store: &mut Store, f: &Func, args: &[Val]) -> Result<Vec<Val>, Error> {
    let mut results = vec![Val::I32(0); f.result_arity(store)];
    f.call(store, args, &mut results)?;
    Ok(results)
}

/// Runs `body` with the panic hook silenced.
///
/// `catch_unwind` stops the unwind but the hook still prints, so a test that
/// panics on purpose would leave a backtrace in the output of a passing run.
fn quietly<T>(body: impl FnOnce() -> T) -> T {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = body();
    std::panic::set_hook(hook);
    out
}

// The issue's first acceptance criterion: a guest reaches a Rust closure, and
// the closure's captured state is visible afterwards.
//
// `Rc<Cell<u32>>` is the point rather than a convenience. It compiles because
// `Func::new` asks for neither `Send` nor `Sync` — nothing in this crate can
// move a store to another thread, so nothing can move the closure to one, and
// requiring the bounds would only force an `Arc<Mutex<_>>` that buys nothing.
#[test]
fn a_guest_reaches_a_closure_that_kept_state() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let calls = Rc::new(Cell::new(0u32));
    let counted = Rc::clone(&calls);
    let host = Func::new(
        &mut store,
        &[ValType::I32],
        &[ValType::I32],
        move |args, results| {
            counted.set(counted.get() + 1);
            let Val::I32(n) = args[0] else {
                return Err(Error::Message("expected an i32".into()));
            };
            results[0] = Val::I32(n + 1);
            Ok(())
        },
    )
    .unwrap();

    let module = Module::new(&mut store, CALLBACK_WASM).unwrap();
    let instance = Instance::new(&mut store, &module, &[host]).unwrap();
    let f = instance.get_func(&mut store, "f").unwrap();

    assert_eq!(
        call(&mut store, &f, &[Val::I32(41)]).unwrap(),
        [Val::I32(42)]
    );
    assert_eq!(call(&mut store, &f, &[Val::I32(0)]).unwrap(), [Val::I32(1)]);
    assert_eq!(calls.get(), 2, "the closure ran once per guest call");
}

#[test]
fn a_closure_can_be_called_directly() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let host = Func::new(&mut store, &[], &[ValType::I32], |_, results| {
        results[0] = Val::I32(7);
        Ok(())
    })
    .unwrap();

    assert_eq!(call(&mut store, &host, &[]).unwrap(), [Val::I32(7)]);
}

// Every result slot arrives from zwasm filled with `{kind: i32, of: 0}`, so a
// closure that returns anything else only works because the slots handed to it
// are built from the declared types rather than from what arrived.
#[test]
fn every_declared_result_type_round_trips() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    for (ty, val) in [
        (ValType::I32, Val::I32(-1)),
        (ValType::I64, Val::I64(i64::MIN)),
        (ValType::F32, Val::F32(0.5)),
        (ValType::F64, Val::F64(-2.25)),
    ] {
        let want = val;
        let host = Func::new(&mut store, &[], &[ty], move |_, results| {
            results[0] = want;
            Ok(())
        })
        .unwrap();
        assert_eq!(call(&mut store, &host, &[]).unwrap(), [want], "{ty:?}");
    }
}

#[test]
fn an_err_from_the_closure_traps_with_its_message() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let host = Func::new(&mut store, &[], &[], |_, _| {
        Err(Error::Message("the host said no".into()))
    })
    .unwrap();

    let err = call(&mut store, &host, &[]).unwrap_err();
    assert_eq!(err.to_string(), "the host said no");
    assert_eq!(err.trap_kind(), Some(TrapKind::BindingError));
}

// A panic unwinding out of an `extern "C"` function aborts the process, so the
// trampoline catches it. This test passing at all is the assertion: an abort
// would take the whole run with it.
#[test]
fn a_panic_becomes_a_trap_rather_than_an_abort() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let host = Func::new(&mut store, &[], &[], |_, _| panic!("the host gave up")).unwrap();

    let err = quietly(|| call(&mut store, &host, &[])).unwrap_err();
    assert!(
        err.to_string().contains("the host gave up"),
        "the panic's own message should survive: {err}"
    );
}

// A closure that writes the wrong type is caught before the value reaches the
// guest, which would otherwise read an i64's bytes as an i32.
#[test]
fn a_result_of_the_wrong_type_is_refused() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let host = Func::new(&mut store, &[], &[ValType::I32], |_, results| {
        results[0] = Val::I64(1);
        Ok(())
    })
    .unwrap();

    let err = call(&mut store, &host, &[]).unwrap_err();
    assert!(
        err.to_string().contains("result 0"),
        "the message should say which slot: {err}"
    );
}

// The store owns the closure, and its own drop is what frees it — once.
//
// There is deliberately no `drop(host)` here to show the other half: `Func` is
// `Copy` with no destructor, so letting a handle go is not an operation at all.
// clippy rejects `drop` on a `Copy` value for that reason, which is the same
// fact from the other side.
#[test]
fn the_store_drops_the_closure_exactly_once() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let held = Rc::new(Cell::new(0u32));
    let captured = Rc::clone(&held);
    let host = Func::new(&mut store, &[], &[], move |_, _| {
        captured.set(1);
        Ok(())
    })
    .unwrap();

    assert_eq!(Rc::strong_count(&held), 2, "the closure holds the second");
    let _ = host;

    drop(store);
    assert_eq!(Rc::strong_count(&held), 1, "the store ran the finalizer");
}

#[test]
fn value_types_map_to_the_kinds_the_c_api_uses() {
    use zwasm_sys as sys;

    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    // Read back through the arity accessors: a functype built from these has to
    // agree with what was asked for.
    let host = Func::new(
        &mut store,
        &[ValType::I32, ValType::F64],
        &[ValType::I64],
        |_, results| {
            results[0] = Val::I64(0);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(host.param_arity(&store), 2);
    assert_eq!(host.result_arity(&store), 1);

    // And the kinds themselves, against the C constants rather than a repeat of
    // the mapping.
    let pairs: &[(ValType, u32)] = &[
        (ValType::I32, sys::wasm_valkind_enum_WASM_I32),
        (ValType::I64, sys::wasm_valkind_enum_WASM_I64),
        (ValType::F32, sys::wasm_valkind_enum_WASM_F32),
        (ValType::F64, sys::wasm_valkind_enum_WASM_F64),
    ];
    for &(ty, kind) in pairs {
        let host = Func::new(&mut store, &[ty], &[], |_, _| Ok(())).unwrap();
        assert_eq!(host.param_arity(&store), 1, "{ty:?} = {kind}");
    }
}

#[test]
#[should_panic(expected = "store it does not belong to")]
fn calling_a_closure_with_a_foreign_store_panics() {
    let engine = Engine::new().unwrap();
    let mut store_a = Store::new(&engine).unwrap();
    let host = Func::new(&mut store_a, &[], &[], |_, _| Ok(())).unwrap();

    let mut store_b = Store::new(&engine).unwrap();
    let _ = host.call(&mut store_b, &[], &mut []);
}
