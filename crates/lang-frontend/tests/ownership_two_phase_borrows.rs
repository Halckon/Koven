//! SPEC-0243: receiver reservation, activation, and call-scoped loan lifetime.
use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    lexer::lex,
    name_resolution::resolve_names,
    ownership_checking::{
        DropPoint, DropTarget, LoanEndPoint, OwnershipCheckedFile, check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

fn checked(body: &str) -> (SourceMap, OwnershipCheckedFile) {
    let text = format!(
        "class Worker(var count: Int) {{\n\
             fun read(): Int = count\n\
             inout fun update(own count: Int): Unit {{}}\n\
             inout fun link(other: Worker): Unit {{}}\n\
             inout fun change(): Int = 2\n\
         }}\n\
         fun inspect(item: Worker): Int = 1\n\
         fun take(own item: Worker): Int = 1\n\
         fun immediate(inout item: Worker, own count: Int): Unit {{}}\n\
         {body}"
    );
    let mut sources = SourceMap::new();
    let source = sources.add_source("two_phase.ko", &text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    (sources, owned)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics.iter().map(|d| d.code().to_string()).collect()
}

#[test]
fn receiver_reservation_allows_completed_nested_reads() {
    for value in [
        "worker.read()",
        "worker.count",
        "inspect(worker)",
        "inspect(worker) + worker.read()",
    ] {
        let (_, owned) = checked(&format!(
            "fun run(own worker: Worker): Unit {{ worker.update({value}) }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{value}: {:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn receiver_activation_keeps_callee_borrow_live_and_clears_invalid_facts() {
    let (sources, owned) = checked("fun run(own worker: Worker): Unit { worker.link(worker) }");
    assert_eq!(codes(owned.diagnostics()), ["L0135"]);
    let diagnostic = &owned.diagnostics()[0];
    assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), "worker");
    let DiagnosticDetail::Label(label) = &diagnostic.details()[0] else {
        panic!("loan label")
    };
    assert!(diagnostic.primary_span().start() > label.span().start());
    assert!(owned.loans().is_empty());
    assert!(owned.loan_ends().is_empty());
    assert!(owned.drops().is_empty());
}

#[test]
fn receiver_reservation_forbids_nested_exclusive_and_move() {
    for value in ["worker.change()", "take(worker)"] {
        let (_, owned) = checked(&format!(
            "fun run(own worker: Worker): Unit {{ worker.update({value}) }}"
        ));
        assert_eq!(codes(owned.diagnostics()), ["L0135"], "{value}");
        assert!(owned.loans().is_empty());
    }
}

#[test]
fn ordinary_inout_arguments_remain_immediately_exclusive() {
    let (_, owned) =
        checked("fun run(inout worker: Worker): Unit { immediate(&worker, worker.read()) }");
    assert_eq!(codes(owned.diagnostics()), ["L0135"]);
}

#[test]
fn receiver_reservation_allows_disjoint_fields_and_tracks_prefix_overlap() {
    let (_, owned) = checked(
        "class Pair(val left: Worker, val right: Worker)\n\
         fun run(own pair: Pair): Unit { pair.left.update(pair.right.change()) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn this_reservation_blocks_nested_mutation_and_field_alias_arguments() {
    for argument in ["change()", "this.change()", "takeThis(this)"] {
        let (_, owned) = checked(&format!(
            "class SelfWorker {{\n\
                 inout fun update(own value: Int): Unit {{}}\n\
                 inout fun change(): Int = 1\n\
                 inout fun run(): Unit {{ update({argument}) }}\n\
             }}\n\
             fun takeThis(own value: SelfWorker): Int = 1"
        ));
        assert_eq!(codes(owned.diagnostics()), ["L0135"], "{argument}");
    }
}

#[test]
fn this_reservation_allows_nested_readonly_receiver() {
    let (_, owned) = checked(
        "class SelfWorker(var count: Int) {\n\
             fun read(): Int = count\n\
             inout fun update(own value: Int): Unit {}\n\
             inout fun run(): Unit { update(read()); this.update(this.read()) }\n\
         }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn phase_facts_activate_only_on_reachable_call_entry() {
    for operand in ["worker.read()", "return", "error(\"stop\")"] {
        let (sources, owned) = checked(&format!(
            "fun run(own worker: Worker): Unit {{ worker.update({operand}) }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let reservation = owned
            .loans()
            .iter()
            .find(|fact| fact.is_receiver_reservation())
            .unwrap();
        assert_eq!(sources.slice(reservation.begin_span()).unwrap(), "worker");
        assert_eq!(
            reservation.activation_point(),
            (operand == "worker.read()").then_some(DropPoint::CallEntry(reservation.call()))
        );
        let endings: Vec<_> = owned
            .loan_ends()
            .iter()
            .filter(|fact| fact.call() == reservation.call())
            .collect();
        match operand {
            "worker.read()" => assert!(
                matches!(endings[0].point(), LoanEndPoint::CallReturn(call) if call == reservation.call())
            ),
            "return" => assert!(matches!(
                endings[0].point(),
                LoanEndPoint::ControlTransfer(_)
            )),
            _ => assert!(endings.is_empty(), "abort does not unwind"),
        }
    }
}

#[test]
fn loop_exits_and_branch_returns_do_not_leak_reservations() {
    for exit in ["break", "continue"] {
        let (_, owned) = checked(&format!(
            "fun run(own worker: Worker, flag: Boolean): Unit {{\n\
                 while (flag) {{ worker.update({exit}) }}\n\
                 worker.update(worker.read())\n\
             }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let reservations: Vec<_> = owned
            .loans()
            .iter()
            .filter(|fact| fact.is_receiver_reservation())
            .collect();
        assert_eq!(reservations.len(), 2);
        assert_eq!(reservations[0].activation_point(), None);
        assert_eq!(
            reservations[1].activation_point(),
            Some(DropPoint::CallEntry(reservations[1].call()))
        );
    }
    let (_, owned) = checked(
        "fun run(own worker: Worker, flag: Boolean): Unit { worker.update(if (flag) { return } else { worker.read() }) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let reservation = owned
        .loans()
        .iter()
        .find(|fact| fact.is_receiver_reservation())
        .unwrap();
    assert_eq!(
        reservation.activation_point(),
        Some(DropPoint::CallEntry(reservation.call()))
    );
}

#[test]
fn this_and_field_borrow_arguments_stay_live_until_activation() {
    for operand in ["count", "this.count", "this", "(this)"] {
        let param = if operand.contains("count") {
            "Int"
        } else {
            "Counter"
        };
        let (sources, owned) = checked(&format!(
            "class Counter(var count: Int) {{\n\
                 inout fun set(value: {param}): Unit {{}}\n\
                 inout fun test(): Unit {{ set({operand}) }}\n\
             }}"
        ));
        assert_eq!(codes(owned.diagnostics()), ["L0135"], "{operand}");
        assert_eq!(
            sources
                .slice(owned.diagnostics()[0].primary_span())
                .unwrap(),
            operand
        );
        assert!(owned.loans().is_empty());
    }
}

#[test]
fn reservations_keep_known_and_unknown_index_alias_rules() {
    for (index, expected) in [
        ("0", vec!["L0135"]),
        ("1", vec![]),
        ("index", vec!["L0135"]),
    ] {
        let (_, owned) = checked(&format!(
            "fun run(own workers: MutableList<Worker>, index: Int): Unit {{ workers[0].update(workers[{index}].change()) }}"
        ));
        assert_eq!(codes(owned.diagnostics()), expected, "{index}");
    }
    let (_, owned) = checked(
        "fun run(own workers: MutableList<Worker>): Unit { workers[0].update(workers[0].read()) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn receiver_reservation_holds_owner_and_borrow_temporary_through_call() {
    let (sources, owned) = checked(
        "class Holder { inout fun update(text: String, own count: Int): Unit {}\nfun read(): Int = 1 }\n\
         fun run(own holder: Holder): Unit { holder.update(\"a\" + \"b\", holder.read()) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let reservation = owned
        .loans()
        .iter()
        .find(|fact| fact.is_receiver_reservation())
        .unwrap();
    let holder_drop = owned
        .drops()
        .iter()
        .find(|fact| {
            matches!(fact.target(), DropTarget::Named(_))
                && sources
                    .slice(fact.value_origin())
                    .unwrap()
                    .contains("holder")
        })
        .unwrap();
    assert_eq!(
        holder_drop.point(),
        DropPoint::CallReturn(reservation.call())
    );
    let text_drop = owned
        .drops()
        .iter()
        .find(|fact| {
            matches!(fact.target(), DropTarget::Temporary(_))
                && sources.slice(fact.value_origin()).unwrap() == "\"a\" + \"b\""
        })
        .unwrap();
    assert_eq!(text_drop.point(), DropPoint::CallReturn(reservation.call()));
}

#[test]
fn move_during_reservation_precedes_non_owning_move_diagnostic() {
    let (_, owned) = checked("fun run(inout worker: Worker): Unit { worker.update(take(worker)) }");
    assert_eq!(codes(owned.diagnostics()), ["L0135"]);
    assert!(owned.loans().is_empty());
}

#[test]
fn overload_trials_publish_only_selected_receiver_and_argument_facts() {
    let (_, owned) = checked(
        "class Counter {\n\
             fun read(): Int = 1\n\
             inout fun set(own next: Int): Unit {}\n\
             fun set(next: String): Unit {}\n\
         }\n\
         fun run(own counter: Counter): Unit { counter.set(counter.read()) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.loans().len(), 2);
    assert_eq!(
        owned
            .loans()
            .iter()
            .filter(|fact| fact.is_receiver_reservation())
            .count(),
        1
    );
}

#[test]
fn owned_implicit_receiver_cannot_move_during_outer_reservation() {
    for operand in ["finish()", "this.finish()"] {
        let (_, owned) = checked(&format!(
            "class Counter {{\n\
                 own fun finish(): Int = 1\n\
                 inout fun set(own value: Int): Unit {{}}\n\
                 inout fun run(): Unit {{ set({operand}) }}\n\
             }}"
        ));
        assert_eq!(codes(owned.diagnostics()), ["L0135"], "{operand}");
    }
}

#[test]
fn conditional_capture_loan_cannot_disappear_before_receiver_activation() {
    let (_, owned) = checked(
        "class Counter {\n\
             fun read(): Int = 1\n\
             inout fun set(action: () -> Unit): Unit {}\n\
         }\n\
         fun run(own counter: Counter, flag: Boolean): Unit {\n\
             counter.set(if (flag) ({ val n = counter.read() }) else ({}))\n\
         }",
    );
    assert_eq!(codes(owned.diagnostics()), ["L0135"]);
    assert!(owned.loans().is_empty());
}

#[test]
fn completed_nested_call_releases_conditional_temporary_capture() {
    let (_, owned) = checked(
        "class Counter {\n\
             fun read(): Int = 1\n\
             inout fun set(own value: Int): Unit {}\n\
         }\n\
         fun inspectAction(action: () -> Unit): Int = 0\n\
         fun run(own counter: Counter, flag: Boolean): Unit {\n\
             counter.set(inspectAction(if (flag) ({ val n = counter.read() }) else ({})))\n\
         }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let reservation = owned
        .loans()
        .iter()
        .find(|fact| fact.is_receiver_reservation())
        .unwrap();
    assert_eq!(
        reservation.activation_point(),
        Some(DropPoint::CallEntry(reservation.call()))
    );
}

#[test]
fn explicit_this_inout_is_immediate_and_respects_readonly_capability() {
    let (_, owned) = checked(
        "class Counter {\n\
             fun read(): Int = 1\n\
             inout fun run(): Unit { exclusiveThis(&this, read()) }\n\
         }\n\
         fun exclusiveThis(inout target: Counter, own value: Int): Unit {}",
    );
    assert_eq!(codes(owned.diagnostics()), ["L0135"]);
    let (_, owned) = checked(
        "class Counter { fun run(): Unit { exclusiveThis(&this) } }\n\
         fun exclusiveThis(inout target: Counter): Unit {}",
    );
    assert_eq!(codes(owned.diagnostics()), ["L0134"]);
}
