//! The five engine events an embedder can listen to.
//!
//! Every expectation here was measured against the bundled zwasm 2.7.0 before
//! it was written down, so these pin the behaviour rather than restating the
//! header. Where the two could drift — the trap message's terminator, which id
//! a host-side grow carries, whether a refused grow is an event — the
//! measurement is what the assertion encodes.
//!
//! The hooks are registered on the engine, so each test builds its own engine
//! and nothing here shares a slot with anything else.
//!
//! `Rc<RefCell<Vec<Event>>>` is how every test collects: it is the shape an
//! embedder would use, and it compiles only because the setters ask for
//! neither `Send` nor `Sync` — the same reason `Func::new`'s closure can
//! capture an `Rc`. A hook that needed `Arc<Mutex<_>>` would be buying
//! synchronisation that `Engine` being `!Send` already makes unreachable.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use zwasm_sdk::{Engine, EngineKind, Instance, Memory, Module, Store, TrapKind, Val};

/// One hook firing, in the order it fired.
///
/// The five variants share a log so that a test can assert on ordering and on
/// what did *not* arrive, which several of the header's claims are about.
#[derive(Debug, Clone, PartialEq)]
enum Event {
    Compile {
        len: usize,
        accepted: bool,
    },
    Instantiate(Option<u64>),
    Trap {
        instance: Option<u64>,
        kind: TrapKind,
        message: String,
    },
    FuelExhausted(Option<u64>),
    Growth {
        instance: Option<u64>,
        index: u32,
        old_pages: u64,
        new_pages: u64,
    },
}

type Log = Rc<RefCell<Vec<Event>>>;

/// Registers all five hooks against one log.
///
/// Tests that care about one hook still install all five: an event arriving on
/// a slot the test did not expect is a finding, and only a shared log can show
/// it — `a_fuel_budget_running_out_raises_both_events` is the case this exists
/// for.
fn watch(engine: &Engine, events: &Log) {
    let sink = Rc::clone(events);
    engine.set_compile_hook(move |len, accepted| {
        sink.borrow_mut().push(Event::Compile { len, accepted });
    });

    let sink = Rc::clone(events);
    engine.set_instantiate_hook(move |instance| {
        sink.borrow_mut().push(Event::Instantiate(instance));
    });

    let sink = Rc::clone(events);
    engine.set_trap_hook(move |instance, kind, message| {
        sink.borrow_mut().push(Event::Trap {
            instance,
            kind,
            // Owned deliberately: the `&str` is borrowed for the call only,
            // which is the header's third rule. Keeping it is what a real
            // collector has to do, so the test does it too.
            message: message.to_owned(),
        });
    });

    let sink = Rc::clone(events);
    engine.set_fuel_exhausted_hook(move |instance| {
        sink.borrow_mut().push(Event::FuelExhausted(instance));
    });

    let sink = Rc::clone(events);
    engine.set_memory_growth_hook(move |instance, index, old_pages, new_pages| {
        sink.borrow_mut().push(Event::Growth {
            instance,
            index,
            old_pages,
            new_pages,
        });
    });
}

fn take_events(events: &Log) -> Vec<Event> {
    std::mem::take(&mut *events.borrow_mut())
}

// (module (func (export "f") (result i32) (i32.const 7)))
const TRIVIAL: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
    0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x06, 0x01, 0x04, 0x00, 0x41,
    0x07, 0x0b,
];

// (module (func (export "f") unreachable))
const UNREACHABLE: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03, 0x02,
    0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x05, 0x01, 0x03, 0x00, 0x00, 0x0b,
];

// (module (func (export "f") (param $n i32) (result i32) (local $i i32)
//   (block $done (loop $l
//     (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
//     (local.set $i (i32.add (local.get $i) (i32.const 1)))
//     (br $l)))
//   (local.get $i)))
const LOOP: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
    0x03, 0x02, 0x01, 0x00, 0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00, 0x0a, 0x1e, 0x01, 0x1c, 0x01,
    0x01, 0x7f, 0x02, 0x40, 0x03, 0x40, 0x20, 0x01, 0x20, 0x00, 0x4f, 0x0d, 0x01, 0x20, 0x01, 0x41,
    0x01, 0x6a, 0x21, 0x01, 0x0c, 0x00, 0x0b, 0x0b, 0x20, 0x01, 0x0b,
];

// (module (memory (export "mem") 1)
//   (func (export "g") (param $d i32) (result i32) (memory.grow (local.get $d)))
//   (func (export "size") (result i32) (memory.size)))
const MEM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x0a, 0x02, 0x60, 0x01, 0x7f, 0x01, 0x7f,
    0x60, 0x00, 0x01, 0x7f, 0x03, 0x03, 0x02, 0x00, 0x01, 0x05, 0x03, 0x01, 0x00, 0x01, 0x07, 0x12,
    0x03, 0x03, 0x6d, 0x65, 0x6d, 0x02, 0x00, 0x01, 0x67, 0x00, 0x00, 0x04, 0x73, 0x69, 0x7a, 0x65,
    0x00, 0x01, 0x0a, 0x0d, 0x02, 0x06, 0x00, 0x20, 0x00, 0x40, 0x00, 0x0b, 0x04, 0x00, 0x3f, 0x00,
    0x0b,
];

/// Calls the export `name` with `args`, returning the first result if any.
fn call(store: &mut Store, instance: &Instance, name: &str, args: &[Val]) -> Option<Val> {
    let f = instance.get_func(store, name).expect("missing export");
    let mut results = vec![Val::I32(0); f.result_arity(store)];
    f.call(store, args, &mut results).ok()?;
    results.first().copied()
}

// ── T1: compile ────────────────────────────────────────────────────────────

// `wasm_len` is the length offered, and `accepted` says whether the engine took
// it. The false case is deliberately paired with the true one: a hook that
// fired only on success would pass a test that checked only the success.
#[test]
fn a_module_offered_to_the_engine_reports_its_length_and_whether_it_was_taken() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();

    Module::new(&mut store, TRIVIAL).expect("TRIVIAL is a valid module");
    let broken: &[u8] = &[0x00, 0x61, 0x73, 0x6d, 0xff, 0xff];
    Module::new(&mut store, broken).expect_err("a truncated header is not a module");

    assert_eq!(
        take_events(&events),
        vec![
            Event::Compile {
                len: TRIVIAL.len(),
                accepted: true
            },
            Event::Compile {
                len: broken.len(),
                accepted: false
            },
        ]
    );
}

// An empty byte vector is an offer of zero bytes, not the absence of an offer:
// it is reported, and rejected. Measured; `zwasm.h` says so too.
#[test]
fn an_empty_byte_vector_is_an_offer_that_gets_rejected() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();

    Module::new(&mut store, &[]).expect_err("zero bytes are not a module");

    assert_eq!(
        take_events(&events),
        vec![Event::Compile {
            len: 0,
            accepted: false
        }]
    );
}

// ── T2: instantiate ────────────────────────────────────────────────────────

// Ids start at 1 and climb. The point of the assertion is that none of them is
// `None`: 0 is the header's "no instance", so a real instantiation must never
// report it.
#[test]
fn instantiating_reports_an_id_that_is_never_the_no_instance_sentinel() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, TRIVIAL).unwrap();
    let _ = take_events(&events);

    Instance::new(&mut store, &module, &[]).unwrap();
    Instance::new(&mut store, &module, &[]).unwrap();

    let ids: Vec<Option<u64>> = take_events(&events)
        .into_iter()
        .map(|e| match e {
            Event::Instantiate(id) => id,
            other => panic!("unexpected event {other:?}"),
        })
        .collect();

    assert_eq!(ids.len(), 2);
    assert!(
        ids.iter().all(Option::is_some),
        "an instantiation that happened is never id 0: {ids:?}"
    );
    assert!(ids[0] < ids[1], "ids are monotonic: {ids:?}");
}

// A failed instantiation raises nothing here — and still consumes an id, so the
// next successful one skips. Both halves are measured, and together they are
// why an embedder cannot treat the ids as dense or as a count of instances.
#[test]
fn a_failed_instantiation_raises_nothing_but_still_consumes_an_id() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();

    // (module (import "m" "f" (func)))
    let needs_import: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x02,
        0x07, 0x01, 0x01, 0x6d, 0x01, 0x66, 0x00, 0x00,
    ];
    let unsatisfiable = Module::new(&mut store, needs_import).unwrap();
    let good = Module::new(&mut store, TRIVIAL).unwrap();
    let _ = take_events(&events);

    Instance::new(&mut store, &unsatisfiable, &[]).expect_err("the import is not supplied");
    assert_eq!(
        take_events(&events),
        vec![],
        "a failed instantiation raises no event at all"
    );

    Instance::new(&mut store, &good, &[]).unwrap();
    let after = take_events(&events);
    assert_eq!(
        after.len(),
        1,
        "only the successful instantiation reports: {after:?}"
    );
    let Event::Instantiate(Some(id)) = after[0] else {
        panic!("unexpected event {:?}", after[0])
    };
    assert!(
        id > 1,
        "the failure consumed an id, so the first success is not 1: {id}"
    );
}

// ── T3: trap ───────────────────────────────────────────────────────────────

// The kind matches what the error carries, and the message matches too — with
// one difference worth pinning. `wasm_trap_message` counts a NUL terminator in
// its size and `Error::Trap` strips it; the hook's (pointer, length) string has
// no terminator to strip. The two therefore agree only because both ends are
// handled, which is what the equality below holds in place.
#[test]
fn a_guest_trap_reports_the_same_kind_and_message_the_error_carries() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, UNREACHABLE).unwrap();
    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    let expected_id = match take_events(&events).last() {
        Some(Event::Instantiate(id)) => *id,
        other => panic!("expected an instantiate event, got {other:?}"),
    };

    let f = instance.get_func(&mut store, "f").unwrap();
    let err = f
        .call(&mut store, &[], &mut [])
        .expect_err("the guest executes `unreachable`");

    assert_eq!(
        take_events(&events),
        vec![Event::Trap {
            instance: expected_id,
            kind: TrapKind::Unreachable,
            message: err.to_string(),
        }]
    );
    assert_eq!(
        err.to_string(),
        "unreachable",
        "the hook's string carries no terminator, and neither should the error"
    );
}

// The header says a trap the embedder mints with `wasm_trap_new` is its own and
// is not reported. `Func::new` turns a closure's `Err` into exactly such a
// trap, so neither an `Err` nor a panic from a host function reaches this hook
// — which is surprising enough that it is asserted rather than left to the doc.
#[test]
fn a_trap_the_embedder_minted_is_not_reported() {
    use zwasm_sdk::{Error, Func};

    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();

    let host = Func::new(&mut store, &[], &[], |_, _| {
        Err(Error::Message("the host declined".into()))
    })
    .unwrap();
    let _ = take_events(&events);

    host.call(&mut store, &[], &mut [])
        .expect_err("the closure returned an error");

    assert_eq!(
        take_events(&events),
        vec![],
        "only traps the engine raised are reported"
    );
}

// A start function that traps reports a trap carrying the id of the instance
// being born — and no instantiate event, because the instantiation failed. So
// an id can reach the trap hook without ever reaching the instantiate hook,
// and a collector keyed on instantiate has to tolerate that.
#[test]
fn a_trapping_start_function_reports_an_id_the_instantiate_hook_never_sees() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();

    // (module (memory 1) (func $s unreachable) (start $s))
    let start_traps: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03,
        0x02, 0x01, 0x00, 0x05, 0x03, 0x01, 0x00, 0x01, 0x08, 0x01, 0x00, 0x0a, 0x05, 0x01, 0x03,
        0x00, 0x00, 0x0b,
    ];
    let module = Module::new(&mut store, start_traps).unwrap();
    let _ = take_events(&events);

    Instance::new(&mut store, &module, &[]).expect_err("the start function traps");

    let seen = take_events(&events);
    assert_eq!(seen.len(), 1, "exactly one event: {seen:?}");
    let Event::Trap {
        instance,
        kind,
        ref message,
    } = seen[0]
    else {
        panic!("expected a trap event, got {:?}", seen[0])
    };
    assert_eq!(kind, TrapKind::Unreachable);
    assert_eq!(message, "unreachable");
    assert!(
        instance.is_some(),
        "the instance being born is named even though it never existed"
    );
}

// ── T4: fuel exhausted ─────────────────────────────────────────────────────

// The header says the fuel event is raised IN ADDITION to a trap of kind
// OUT_OF_FUEL, so a host that meters fuel need not switch on the kind. The
// shared log is what shows both arrived for the same instance — and it is also
// the warning: a collector that counts both events double-counts one exhaustion.
#[test]
fn a_fuel_budget_running_out_raises_both_events_for_the_same_instance() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, LOOP).unwrap();
    let instance = Instance::new_with_engine(&mut store, &module, &[], EngineKind::Interp).unwrap();

    let id = match take_events(&events).last() {
        Some(Event::Instantiate(id)) => *id,
        other => panic!("expected an instantiate event, got {other:?}"),
    };

    instance.set_fuel(&mut store, 1_000);
    let f = instance.get_func(&mut store, "f").unwrap();
    let mut results = [Val::I32(0)];
    let err = f
        .call(&mut store, &[Val::I32(100_000)], &mut results)
        .expect_err("the budget cannot cover this");
    assert_eq!(err.trap_kind(), Some(TrapKind::OutOfFuel));

    let seen = take_events(&events);
    assert!(
        seen.contains(&Event::FuelExhausted(id)),
        "the fuel event names the instance that ran out: {seen:?}"
    );
    assert!(
        seen.iter().any(|e| matches!(
            e,
            Event::Trap {
                instance,
                kind: TrapKind::OutOfFuel,
                ..
            } if *instance == id
        )),
        "and the trap event is raised as well: {seen:?}"
    );
}

// ── T5: memory growth ──────────────────────────────────────────────────────

// A guest `memory.grow` reports the page counts either side of the growth, and
// names the instance that owns the memory.
#[test]
fn a_guest_growing_its_memory_reports_the_pages_either_side() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, MEM).unwrap();
    let instance = Instance::new_with_engine(&mut store, &module, &[], EngineKind::Interp).unwrap();

    let id = match take_events(&events).last() {
        Some(Event::Instantiate(id)) => *id,
        other => panic!("expected an instantiate event, got {other:?}"),
    };

    assert_eq!(
        call(&mut store, &instance, "g", &[Val::I32(2)]),
        Some(Val::I32(1)),
        "memory.grow returns the previous size"
    );

    assert_eq!(
        take_events(&events),
        vec![Event::Growth {
            instance: id,
            index: 0,
            old_pages: 1,
            new_pages: 3,
        }]
    );
}

// A refused grow is the spec's recoverable `-1`, not an event: nothing grew, so
// nothing is reported. This is the half that a hook counting "grow attempts"
// would get wrong.
#[test]
fn a_refused_grow_is_not_an_event() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, MEM).unwrap();
    let instance = Instance::new_with_engine(&mut store, &module, &[], EngineKind::Interp).unwrap();
    instance.set_memory_pages_limit(&mut store, 2);
    let _ = take_events(&events);

    assert_eq!(
        call(&mut store, &instance, "g", &[Val::I32(4)]),
        Some(Val::I32(-1)),
        "growing past the cap fails the grow rather than trapping"
    );

    assert_eq!(
        take_events(&events),
        vec![],
        "a grow that did not happen is not a growth event"
    );
}

// The host side raises the event too, and the id is what separates the two
// cases: an instance's exported memory is named by that instance, while a
// memory the host made with `Memory::new` belongs to no instance and arrives as
// `None` — the header's id 0.
#[test]
fn a_host_side_grow_names_the_instance_only_when_there_is_one() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();
    let module = Module::new(&mut store, MEM).unwrap();
    let instance = Instance::new_with_engine(&mut store, &module, &[], EngineKind::Interp).unwrap();

    let id = match take_events(&events).last() {
        Some(Event::Instantiate(id)) => *id,
        other => panic!("expected an instantiate event, got {other:?}"),
    };

    let exported = instance
        .get_memory(&mut store, "mem")
        .expect("the module exports `mem`");
    exported
        .grow(&mut store, 4)
        .expect("the memory is uncapped");
    assert_eq!(
        take_events(&events),
        vec![Event::Growth {
            instance: id,
            index: 0,
            old_pages: 1,
            new_pages: 5,
        }],
        "growing an instance's memory from the host names that instance"
    );

    let host_owned = Memory::new(&mut store, 1, Some(10)).unwrap();
    host_owned.grow(&mut store, 2).unwrap();
    assert_eq!(
        take_events(&events),
        vec![Event::Growth {
            instance: None,
            index: 0,
            old_pages: 1,
            new_pages: 3,
        }],
        "a memory no instance owns reports the no-instance sentinel"
    );
}

// ── T6 / T7: the slots themselves ──────────────────────────────────────────

#[test]
fn a_cleared_hook_stops_firing() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();

    Module::new(&mut store, TRIVIAL).unwrap();
    assert_eq!(take_events(&events).len(), 1, "installed, so it fires");

    engine.clear_compile_hook();
    Module::new(&mut store, TRIVIAL).unwrap();
    assert_eq!(take_events(&events), vec![], "cleared, so it does not");
}

// Clearing one slot leaves the other four alone. Written because a shared
// `user_data` pointer makes "clear" a per-slot operation that could plausibly
// have been engine-wide.
#[test]
fn clearing_one_slot_leaves_the_others_installed() {
    let engine = Engine::new().unwrap();
    let events = Log::default();
    watch(&engine, &events);
    let mut store = Store::new(&engine).unwrap();

    engine.clear_compile_hook();
    let module = Module::new(&mut store, TRIVIAL).unwrap();
    Instance::new(&mut store, &module, &[]).unwrap();

    let seen = take_events(&events);
    assert!(
        seen.iter().all(|e| !matches!(e, Event::Compile { .. })),
        "the cleared slot is silent: {seen:?}"
    );
    assert!(
        seen.iter().any(|e| matches!(e, Event::Instantiate(_))),
        "the others still fire: {seen:?}"
    );
}

// Setting twice replaces rather than adds, and the closure that was displaced
// is dropped rather than leaked — which the `Rc` strong count is what shows.
#[test]
fn setting_twice_replaces_the_closure_and_drops_the_old_one() {
    let engine = Engine::new().unwrap();
    let mut store = Store::new(&engine).unwrap();

    let first = Log::default();
    let second = Log::default();

    let sink = Rc::clone(&first);
    engine.set_compile_hook(move |len, accepted| {
        sink.borrow_mut().push(Event::Compile { len, accepted });
    });
    assert_eq!(Rc::strong_count(&first), 2, "the engine holds the closure");

    let sink = Rc::clone(&second);
    engine.set_compile_hook(move |len, accepted| {
        sink.borrow_mut().push(Event::Compile { len, accepted });
    });
    assert_eq!(
        Rc::strong_count(&first),
        1,
        "the displaced closure was dropped, not leaked"
    );

    Module::new(&mut store, TRIVIAL).unwrap();
    assert_eq!(take_events(&first), vec![], "the old closure is gone");
    assert_eq!(
        take_events(&second),
        vec![Event::Compile {
            len: TRIVIAL.len(),
            accepted: true
        }],
        "only the newest one fires"
    );
}

// Replacing a hook keeps the replacement, even when the closure it displaced
// owns something whose `Drop` reaches back into the engine. The displaced
// closure is dropped while the slot is held, so a `Drop` that calls `clear_*`
// finds it taken and is ignored rather than clearing what just replaced it.
//
// Not a re-entrancy case: the `Drop` runs inside `set_compile_hook`, not inside
// a hook, so the rule against a hook calling back into the engine does not
// cover it.
#[test]
fn a_displaced_closure_cannot_clear_the_one_replacing_it() {
    struct ClearsOnDrop(Engine);
    impl Drop for ClearsOnDrop {
        fn drop(&mut self) {
            self.0.clear_compile_hook();
        }
    }

    let engine = Engine::new().unwrap();
    let guard = ClearsOnDrop(engine.clone());
    engine.set_compile_hook(move |_, _| {
        let _ = &guard;
    });

    let events = Log::default();
    let sink = Rc::clone(&events);
    // Installing this drops `guard`, which calls `clear_compile_hook`.
    engine.set_compile_hook(move |len, accepted| {
        sink.borrow_mut().push(Event::Compile { len, accepted });
    });

    let mut store = Store::new(&engine).unwrap();
    Module::new(&mut store, TRIVIAL).unwrap();

    assert_eq!(
        take_events(&events),
        vec![Event::Compile {
            len: TRIVIAL.len(),
            accepted: true
        }],
        "the replacement survived its predecessor's destructor"
    );
}

// Dropping the engine drops the closures it holds. The `Rc` outliving the
// engine is what makes the count readable afterwards.
#[test]
fn dropping_the_engine_drops_the_closures() {
    let events = Log::default();
    {
        let engine = Engine::new().unwrap();
        watch(&engine, &events);
        assert_eq!(
            Rc::strong_count(&events),
            6,
            "one per hook, plus the handle here"
        );
    }
    assert_eq!(
        Rc::strong_count(&events),
        1,
        "the engine released all five on drop"
    );
}

// A closure that captures an `Engine` clone makes a reference cycle, and
// `clear_*` is the way out of it: dropping the closure drops the captured
// handle with it. The doc promises this, so it is measured rather than
// reasoned about — and it is why the escape has to happen while a handle is
// still around to call it with.
#[test]
fn clearing_a_hook_releases_what_its_closure_captured() {
    struct Tell(Rc<Cell<bool>>);
    impl Drop for Tell {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    let released = Rc::new(Cell::new(false));
    let engine = Engine::new().unwrap();

    let tell = Tell(Rc::clone(&released));
    let captured = engine.clone();
    engine.set_compile_hook(move |_, _| {
        let _ = (&tell, &captured);
    });
    assert!(!released.get(), "installed, so the closure is still held");

    engine.clear_compile_hook();
    assert!(
        released.get(),
        "clearing drops the closure, and the engine clone inside it"
    );
}

// `Engine` is `Clone`, and the clones share one C engine — so a hook installed
// through one is installed for all of them. Asserted because the alternative
// reading (a hook belonging to the handle) is the plausible one.
#[test]
fn a_hook_installed_through_one_clone_is_installed_for_every_clone() {
    let engine = Engine::new().unwrap();
    let events = Log::default();

    let clone = engine.clone();
    watch(&clone, &events);

    let mut store = Store::new(&engine).unwrap();
    Module::new(&mut store, TRIVIAL).unwrap();

    assert_eq!(
        take_events(&events),
        vec![Event::Compile {
            len: TRIVIAL.len(),
            accepted: true
        }],
        "the engine is shared, so the slot is too"
    );

    clone.clear_compile_hook();
    Module::new(&mut store, TRIVIAL).unwrap();
    assert_eq!(take_events(&events), vec![], "and so is clearing it");
}

// ── T8: no Send, no Sync ───────────────────────────────────────────────────

// `Rc<Cell<u32>>` is the point rather than a convenience, the same way it is in
// `host_func.rs`: it compiles only because the setters ask for neither `Send`
// nor `Sync`. Requiring them would force an `Arc<Mutex<_>>` that buys nothing,
// since `Engine` being `!Send` already keeps every hook on one thread.
#[test]
fn a_hook_can_capture_an_rc() {
    let engine = Engine::new().unwrap();
    let compiles = Rc::new(Cell::new(0u32));

    let counted = Rc::clone(&compiles);
    engine.set_compile_hook(move |_, _| counted.set(counted.get() + 1));

    let mut store = Store::new(&engine).unwrap();
    Module::new(&mut store, TRIVIAL).unwrap();
    Module::new(&mut store, TRIVIAL).unwrap();

    assert_eq!(compiles.get(), 2);
}
