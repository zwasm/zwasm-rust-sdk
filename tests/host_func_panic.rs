//! A panic inside a host function becomes a trap, not an abort.
//!
//! This test has a file to itself, and the reason is the panic hook. Verifying
//! the trap means silencing the hook, or a passing run prints a backtrace that
//! reads like a failure — and the hook is process-wide, so doing that anywhere
//! near another test can swallow its diagnostics. `cargo` compiles each
//! `tests/*.rs` into its own binary, so a file holding one test is a process
//! holding one test, and there is nothing else in it to affect.

use zwasm_sdk::{Engine, Func, Store, Val};

/// Runs `body` with the panic hook silenced, restoring it afterwards.
///
/// The restore cannot go in a `Drop`. `std::panic::set_hook` refuses to run on a
/// panicking thread — it panics itself, `library/std/src/panicking.rs` checks
/// `thread::panicking()` and says so — so a guard dropped during an unwind would
/// panic while panicking and abort, with the silent hook still installed and the
/// original message already lost. `catch_unwind` first, restore off the panicking
/// path, then resume.
fn quietly<T>(body: impl FnOnce() -> T) -> T {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    std::panic::set_hook(hook);
    out.unwrap_or_else(|payload| std::panic::resume_unwind(payload))
}

// A panic unwinding out of an `extern "C"` function aborts the process, so the
// trampoline catches it. This test passing at all is the assertion: an abort
// would take the whole run with it.
#[test]
fn a_panic_becomes_a_trap_rather_than_an_abort() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();
    let host = Func::new(&mut store, &[], &[], |_, _| panic!("the host gave up")).unwrap();

    let mut results: Vec<Val> = Vec::new();
    let err = quietly(|| host.call(&mut store, &[], &mut results)).unwrap_err();
    assert!(
        err.to_string().contains("the host gave up"),
        "the panic's own message should survive: {err}"
    );
}
