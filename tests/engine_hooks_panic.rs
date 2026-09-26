//! A panic inside an observability hook is swallowed, not an abort.
//!
//! This test has a file to itself for the reason `host_func_panic.rs` does: the
//! panic hook is process-wide, so silencing it anywhere near another test can
//! swallow that test's diagnostics. `cargo` compiles each `tests/*.rs` into its
//! own binary, so a file holding one test is a process holding one test.
//!
//! What is asserted here differs from the host-function case, and the
//! difference is the point. A panic out of `Func::new`'s closure becomes a
//! trap, because that callback returns one. A hook returns `void`: there is
//! nowhere for a caught panic to go, so it is dropped. An embedder that needs
//! to know its collector failed has to catch inside its own closure.

use std::cell::Cell;
use std::rc::Rc;

use zwasm_sdk::{Engine, Module, Store};

/// Runs `body` with the panic hook silenced, restoring it afterwards.
///
/// The restore cannot go in a `Drop`. `std::panic::set_hook` refuses to run on
/// a panicking thread — it panics itself — so a guard dropped during an unwind
/// would panic while panicking and abort, with the silent hook still installed.
/// `catch_unwind` first, restore off the panicking path, then resume.
fn quietly<T>(body: impl FnOnce() -> T) -> T {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    std::panic::set_hook(hook);
    out.unwrap_or_else(|payload| std::panic::resume_unwind(payload))
}

// (module (func (export "f") (result i32) (i32.const 7)))
const TRIVIAL: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
    0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x06, 0x01, 0x04, 0x00, 0x41,
    0x07, 0x0b,
];

// A panic unwinding out of an `extern "C"` function aborts the process, so the
// trampoline catches it. This test reaching its asserts at all is most of the
// assertion — an abort would take the whole binary with it.
#[test]
fn a_panicking_hook_is_swallowed_and_the_operation_still_succeeds() {
    let engine = Engine::new().unwrap();

    let fired = Rc::new(Cell::new(0u32));
    let counted = Rc::clone(&fired);
    engine.set_compile_hook(move |_, _| {
        counted.set(counted.get() + 1);
        panic!("the collector gave up");
    });

    let mut store = Store::new(&engine).unwrap();

    // The panic does not reach the caller: `Module::new` returns normally, and
    // the module is real. A hook is an observer, so its failure cannot be
    // allowed to change what the operation did.
    let module = quietly(|| Module::new(&mut store, TRIVIAL));
    module.expect("a panicking hook must not fail the compile it was watching");
    assert_eq!(fired.get(), 1);

    // And the slot survives: one panic does not uninstall the hook.
    let module = quietly(|| Module::new(&mut store, TRIVIAL));
    module.expect("still fine the second time");
    assert_eq!(
        fired.get(),
        2,
        "the hook is still installed after panicking"
    );

    // `quietly` itself survives a panic in its body, which a `Drop`-based
    // version would not: restoring the hook from a `Drop` would run `set_hook`
    // on a panicking thread, panic again, and abort. Reaching the assert is the
    // proof. Kept here rather than in its own test so the file stays at one
    // test and nothing races over the process-wide hook.
    let escaped = std::panic::catch_unwind(|| quietly(|| panic!("escaped")));
    assert!(
        escaped.is_err(),
        "the panic has to arrive here rather than abort"
    );
}
