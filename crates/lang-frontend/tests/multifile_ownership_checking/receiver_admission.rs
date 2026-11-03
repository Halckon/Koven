//! SPEC-0341 receiver admission: failed receiver evaluation must not apply its contract.
//! Each entry is admitted independently; diagnostics are compared in full across entries.
//! Receiver/loan fact assertions below are selected contracts, not whole-product equivalence.
use super::*;
use lang_frontend::{
    diagnostic::{Diagnostic, Severity},
    type_checking::complete_compilation_unit_types,
};

fn observe(text: &str, expected: &[&str], inspect: impl Fn(&SourceMap, &CompilationUnitOwnership)) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "receiver.ko", text);
    let (ne, te) = standard_environments();
    let mut observations = Vec::new();
    for input in [
        SourceUnitInput::memory_document(source, &file),
        SourceUnitInput::new("root", "receiver.ko", source, &file),
    ] {
        let inputs = [input];
        let names = validated_names(&sources, &inputs, &ne);
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
        assert!(
            raw.diagnostics().is_empty(),
            "{text}: {:?}",
            raw.diagnostics()
        );
        let _complete = complete_compilation_unit_types(&sources, &inputs, &names, &te, &raw)
            .expect("final type admission, not merely basic validation");
        let typed = raw.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &te, &typed).unwrap();
        assert_eq!(diagnostic_codes(&owned), expected, "{text}");
        assert!(
            owned.deferred().is_empty(),
            "{text}: {:?}",
            owned.deferred()
        );
        assert_eq!(owned.clone().validate().is_ok(), expected.is_empty());
        inspect(&sources, &owned);
        observations.push(owned);
    }
    assert_eq!(
        observations[0].diagnostics(),
        observations[1].diagnostics(),
        "{text}"
    );
}

fn no_inspection(_: &SourceMap, _: &CompilationUnitOwnership) {}

#[test]
fn five_original_moved_mutable_list_receivers_report_only_use_after_move() {
    // Exact original sources from ownership_containers. Existing Single witnesses stay intact.
    let cases = [
        "fun consume(own l: MutableList<Int>): Unit {}\n\
         fun clear_it(own list: MutableList<Int>): Unit {\n\
             consume(list)\n\
             list.clear()\n\
         }",
        "fun consume(own l: MutableList<Int>): Unit {}\n\
         fun remove_it(own list: MutableList<Int>): Unit {\n\
             consume(list)\n\
             val item = list.removeAt(0)\n\
         }",
        "fun consume(own l: MutableList<Int>): Unit {}\n\
         fun remove_tail(own list: MutableList<Int>): Unit {\n\
             consume(list)\n\
             val item = list.removeLast()\n\
         }",
        "fun consume(own l: MutableList<Int>): Unit {}\n\
         fun remove_head(own list: MutableList<Int>): Unit {\n\
             consume(list)\n\
             val item = list.removeFirst()\n\
         }",
        "fun consume(own l: MutableList<Int>): Unit {}\n\
         fun insert_moved(own list: MutableList<Int>): Unit {\n\
             consume(list)\n\
             list.insertAt(0, 42)\n\
         }",
    ];
    for text in cases {
        observe(text, &["L0131"], |sources, owned| {
            let diagnostic = &owned.diagnostics()[0];
            let source = diagnostic.primary_span().source_id();
            let primary = sources
                .span(
                    source,
                    text.rfind("list.").unwrap(),
                    text.rfind("list.").unwrap() + 4,
                )
                .unwrap();
            let origin_start = text.find("consume(list)").unwrap() + "consume(".len();
            let origin = sources
                .span(source, origin_start, origin_start + 4)
                .unwrap();
            let mut expected = Diagnostic::new(
                sources,
                Severity::Error,
                diagnostic.code(),
                "use of moved value",
                primary,
            )
            .unwrap();
            expected
                .add_label(sources, origin, "value was moved here")
                .unwrap();
            assert_eq!(owned.diagnostics(), &[expected]);
            assert!(
                owned.receiver_facts().is_empty(),
                "failed receiver cannot publish a receiver fact"
            );
        });
    }
}

#[test]
fn valid_immutable_receiver_still_reports_l0134() {
    for call in [
        "list.clear()",
        "list.insertAt(0, 42)",
        "list.removeAt(0)",
        "list.removeFirst()",
        "list.removeLast()",
    ] {
        observe(
            &format!("fun run(own list: MutableList<Int>): Unit {{ {call} }}"),
            &["L0134"],
            no_inspection,
        );
    }
}

#[test]
fn failed_receiver_does_not_skip_independent_argument_diagnostics() {
    observe("class Item {}
fun takeList(own list: MutableList<Item>): Unit {}
fun take(own item: Item): Unit {}
fun run(own list: MutableList<Item>, own item: Item): Unit { takeList(list); take(item); list.insertAt(0, item) }", &["L0131", "L0131"], |sources, owned| {
        assert_eq!(sources.slice(owned.diagnostics()[0].primary_span()).unwrap(), "list");
        assert_eq!(sources.slice(owned.diagnostics()[1].primary_span()).unwrap(), "item");
    });
}

#[test]
fn ordinary_two_phase_reservation_allows_completed_reads() {
    for argument in ["worker.read()", "worker.count"] {
        observe(&format!("class Worker(var count: Int) {{ fun read(): Int = count; inout fun update(own count: Int): Unit {{}} }}
fun run(own worker: Worker): Unit {{ worker.update({argument}) }}"), &[], |_, owned| {
            assert!(owned.receiver_facts().iter().any(|fact| fact.kind() == UnitReceiverOwnershipKind::ExclusiveLoan
                        && fact.is_receiver_reservation()
                        && fact.activation_point() == Some(fact.call())));
        });
    }
}

#[test]
fn ordinary_two_phase_reservation_rejects_nested_mutation() {
    observe(
        "class Worker { inout fun update(own value: Int): Unit {}; inout fun change(): Int = 1 }
fun run(own worker: Worker): Unit { worker.update(worker.change()) }",
        &["L0135"],
        no_inspection,
    );
}

#[test]
fn explicit_and_implicit_this_readonly_nested_calls_remain_valid() {
    for call in ["update(read())", "this.update(this.read())"] {
        observe(
            &format!(
                "class Worker(var count: Int) {{ fun read(): Int = count; inout fun update(own value: Int): Unit {{}}; inout fun run(): Unit {{ {call} }} }}"
            ),
            &[],
            no_inspection,
        );
    }
}

#[test]
fn explicit_and_implicit_this_nested_mutation_remains_rejected() {
    for call in ["update(change())", "this.update(this.change())"] {
        observe(
            &format!(
                "class Worker {{ inout fun update(own value: Int): Unit {{}}; inout fun change(): Int = 1; inout fun run(): Unit {{ {call} }} }}"
            ),
            &["L0135"],
            no_inspection,
        );
    }
}

#[test]
fn map_consume_rejection_keeps_its_existing_admission_gate() {
    for (text, expected) in [
        (
            "fun run(): Unit { val result = mutableMapOf<Int, Int>().consume() }",
            "L0130",
        ),
        (
            "fun run(source: MutableMap<Int, Int>): Unit { val result = source.consume() }",
            "L0133",
        ),
        (
            "fun run(): Unit { val source = mutableMapOf<Int, Int>(); val first = source.consume(); val second = source.consume() }",
            "L0131",
        ),
    ] {
        observe(text, &[expected], no_inspection);
    }
}

#[test]
fn valid_map_consume_still_publishes_transfer() {
    observe(
        "fun run(): Unit { val source = mutableMapOf<Int, Int>(); val result = source.consume() }",
        &[],
        |_, owned| {
            assert_eq!(owned.receiver_facts().len(), 1);
            assert_eq!(
                owned.receiver_facts()[0].kind(),
                UnitReceiverOwnershipKind::Move
            );
        },
    );
}

#[test]
fn receiver_activation_still_rejects_live_argument_borrow() {
    observe(
        "class Worker { inout fun link(other: Worker): Unit {} }\nfun run(own worker: Worker): Unit { worker.link(worker) }",
        &["L0135"],
        no_inspection,
    );
}
