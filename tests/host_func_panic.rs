//! A panic inside a host function becomes a trap, not an abort.
//!
//! This test has a file to itself, and the reason is the panic hook. Verifying
//! the trap means silencing the hook, or a passing run prints a backtrace that
//! reads like a failure — and the hook is process-wide, so doing that anywhere
//! near another test can swallow its diagnostics. `cargo` compiles each
//! `tests/*.rs` into its own binary, so a file holding one test is a process
//! holding one test, and there is nothing else in it to affect.

use zwasm_sdk::{Engine, Func, Store, Val};

/// Runs `body` with the panic hook silenced, restoring it however `body` leaves.
fn quietly<T>(body: impl FnOnce() -> T) -> T {
    type Hook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send>;

    struct Restore(Option<Hook>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(hook) = self.0.take() {
                std::panic::set_hook(hook);
            }
        }
    }

    let _restore = Restore(Some(std::panic::take_hook()));
    std::panic::set_hook(Box::new(|_| {}));
    body()
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
