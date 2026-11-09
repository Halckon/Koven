//! Seed every borrow-result table through real sources before observing raw rejection.
use super::*;
use crate::ownership_checking::OwnershipDeferredReason;

const DIAGNOSTIC_SUFFIX: &str = "fun rejected(): Unit { val target = 0; target = 1 }";
const DEFERRED_SUFFIX: &str =
    "fun rejected(): Unit { var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }";

fn fixture(texts: &[(&str, &str)], range: bool) -> Fixture {
    if range {
        Fixture::range_source(texts, 0)
    } else {
        Fixture::new(texts)
    }
}

fn assert_cleared(raw: &Analysis) {
    assert!(raw.formed_owned_locals.is_empty());
    assert!(raw.nullable_whens.is_empty());
    assert!(raw.borrow_return_origins.is_empty());
    assert!(raw.borrow_results.bindings.is_empty());
    assert!(raw.borrow_results.ends.is_empty());
    assert!(raw.borrow_results.forwarded.is_empty());
    assert!(raw.borrow_results.range_returns.is_empty());
    assert!(raw.borrow_results.range_uses.is_empty());
    assert!(raw.borrow_results.call_uses.is_empty());
    assert!(raw.borrow_forward_transports.is_empty());
    assert!(raw.receiver_call_prefix_ends.is_empty());
    assert!(raw.receiver_call_return_ends.is_empty());
    assert!(raw.borrow_call_transports.is_empty());
    assert!(raw.borrow_binding_transports.is_empty());
}

fn assert_rejections_clear(text: &str, range: bool, positive: impl FnOnce(&Fixture, &Analysis)) {
    let accepted = fixture(&[("a.ko", text)], range);
    let shared = accepted.shared();
    assert!(
        shared.diagnostics().is_empty(),
        "{:?}",
        shared.diagnostics()
    );
    assert!(shared.deferred().is_empty(), "{:?}", shared.deferred());
    shared.clone().validate().unwrap();
    let raw = accepted.dataflow(&accepted.parts(&shared));
    assert!(raw.diagnostics.is_empty(), "{:?}", raw.diagnostics);
    assert!(raw.deferred.is_empty(), "{:?}", raw.deferred);
    positive(&accepted, &raw);

    for (suffix, deferred) in [(DIAGNOSTIC_SUFFIX, false), (DEFERRED_SUFFIX, true)] {
        // Keep the complete accepted source unchanged and analyze it first. In
        // particular its drop planner must produce ends before a later L0134 can
        // prevent the rejecting source's drop planner from running.
        let rejected = fixture(&[("a.ko", text), ("z.ko", suffix)], range);
        assert_eq!(
            rejected
                .names
                .names()
                .index()
                .source_units()
                .iter()
                .map(|source| source.id())
                .collect::<Vec<_>>(),
            [rejected.source(0), rejected.source(1)],
            "the real positive source is analyzed before the rejecting source"
        );
        let shared = rejected.shared();
        let raw = rejected.dataflow(&rejected.parts(&shared));
        if deferred {
            assert!(raw.diagnostics.is_empty(), "{:?}", raw.diagnostics);
            let expression = UnitExpressionId::new(
                rejected.source(1),
                rejected.expression(1, "replace(&callback, { 2 })"),
            );
            assert_eq!(
                raw.deferred,
                [UnitOwnershipDeferredFact::new(
                    expression,
                    OwnershipDeferredReason::OwnershipPrimitiveClosureTransport,
                )],
                "a real deferred-only input, not a synthetic publication replay"
            );
        } else {
            assert!(raw.deferred.is_empty(), "{:?}", raw.deferred);
            let target = rejected.expression(1, "target");
            let span = rejected.parsed[1]
                .ast()
                .expressions()
                .get(target)
                .unwrap()
                .span();
            let symbol = rejected.symbol(1, "target");
            let declaration = rejected.names.names().source_units()[symbol.source_unit().index()]
                .resolution()
                .symbols()[symbol.symbol().index()]
            .span();
            let mut expected = Diagnostic::new(
                &rejected.sources,
                Severity::Error,
                codes::catalog()
                    .unwrap()
                    .resolve(codes::IMMUTABLE_INOUT_PLACE)
                    .unwrap(),
                "assignment target is not a mutable place",
                span,
            )
            .unwrap();
            expected
                .add_label(
                    &rejected.sources,
                    declaration,
                    "immutable binding declared here",
                )
                .unwrap();
            assert_eq!(raw.diagnostics, [expected]);
            assert_eq!(rejected.sources.slice(span).unwrap(), "target");
        }
        // Observe dataflow::analyze directly: CompilationUnitOwnership::new must not
        // hide a missing clear in the earlier production gate.
        assert_cleared(&raw);
    }
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_adjacent_history_and_borrow_tables() {
    assert_rejections_clear(super::adjacent::SOURCE, false, |_, raw| {
        assert!(!raw.formed_owned_locals.is_empty());
        assert!(!raw.nullable_whens.is_empty());
        assert!(!raw.borrow_return_origins.is_empty());
        assert_eq!(raw.borrow_results.bindings.len(), 1);
        assert_eq!(raw.borrow_results.ends.len(), 1);
    });
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_forwarded_and_range_return_tables() {
    // Unchanged String cell from ownership_range_producer's ordinary producer witness.
    let text = "class Item(val number: Int) {}\nfun <T> prefix(source: List<T>, count: Int): View<T> from source { return rangeView(source, 0, count) }\nfun observe(source: View<String>): Unit {}\nfun consume(own source: List<String>): Unit {}\nfun run(): Unit { val source = listOf(\"kept\"); borrow val part = prefix(source, 1); observe(part); consume(source) }";
    assert_rejections_clear(text, true, |_, raw| {
        assert_eq!(raw.borrow_results.bindings.len(), 1);
        assert_eq!(raw.borrow_results.ends.len(), 1);
        assert_eq!(raw.borrow_results.forwarded.len(), 1);
        assert_eq!(raw.borrow_results.range_returns.len(), 1);
    });
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_temporary_range_use_table() {
    // Unchanged ordinary temporary source from ownership_range_construction.
    let text = "fun observe(source: View<String>): Unit {}\nfun alias(source: View<String>): borrow View<String> from source = source\nfun consume(own source: List<String>): Unit {}\nfun run():Unit{for(item in rangeView(listOf(\"temporary\"),0,1)){println(item)}}";
    assert_rejections_clear(text, true, |accepted, raw| {
        let [fact] = raw.borrow_results.range_uses.as_slice() else {
            panic!("one actual temporary range use");
        };
        let UnitLoanTarget::Temporary(root) = *fact.origin() else {
            panic!("the ordinary source's temporary owner");
        };
        assert_ne!(root, fact.expression());
        assert_eq!(root.source_unit(), accepted.source(0));
        assert_eq!(raw.iterations[0].source(), Some(fact.origin()));
    });
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_synchronous_call_use_table() {
    // Unchanged direct cell and ordinary prefix from ownership_nested_borrow_arguments.
    let text = "fun view(source: String): borrow String from source = source\nfun observe(item: String) { println(item) }\nfun consume(own item: String) {}\nfun run() { val source = \"kept\"; observe(view(source)); consume(source) }";
    assert_rejections_clear(text, false, |_, raw| {
        assert_eq!(raw.borrow_results.call_uses.len(), 1);
    });
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_forwarded_source_transport() {
    let text = "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)";
    assert_rejections_clear(text, false, |_, raw| {
        assert_eq!(raw.borrow_forward_transports.len(), 1);
    });
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_call_and_binding_transports() {
    let text = "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; observe(view(owner)); borrow val item = view(owner); println(item) }";
    assert_rejections_clear(text, false, |_, raw| {
        assert_eq!(raw.borrow_call_transports.len(), 1);
        assert_eq!(raw.borrow_binding_transports.len(), 1);
    });
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_receiver_prefix_table() {
    let text = "class Node { fun touch(text: String, own next: Int): Int = next }\nfun run(): Unit { val node = Node(); node.touch(\"prefix\", return) }";
    assert_rejections_clear(text, false, |_, raw| {
        assert_eq!(raw.receiver_call_prefix_ends.len(), 1);
    });
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_receiver_loop_prefix_table() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let text = format!(
            "class Node {{ fun touch(text: String, own next: Int): Int = next }}\nfun run(flag: Boolean): Unit {{ {loop_start} {{ val node = Node(); node.touch(\"prefix\", {jump}) }} }}"
        );
        assert_rejections_clear(&text, false, |_, raw| {
            assert_eq!(raw.receiver_call_prefix_ends.len(), 1);
        });
    }
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_receiver_return_table() {
    let text = "class Node { fun touch(text: String, own next: Int): Int = next }\nfun run(): Unit { val node = Node(); node.touch(\"ready\", 0) }";
    assert_rejections_clear(text, false, |_, raw| {
        assert_eq!(raw.receiver_call_return_ends.len(), 1);
    });
}

#[test]
fn normal_receiver_ends_keep_identical_ast_indices_in_distinct_sources_separate() {
    let accepted = Fixture::new(&[
        (
            "a.ko",
            "class Left { fun read(): Int = 1 }\nfun first(own node: Left): Unit { node.read() }",
        ),
        (
            "b.ko",
            "class Rght { fun read(): Int = 1 }\nfun other(own node: Rght): Unit { node.read() }",
        ),
    ]);
    let shared = accepted.shared();
    assert!(
        shared.diagnostics().is_empty(),
        "{:?}",
        shared.diagnostics()
    );
    assert!(shared.deferred().is_empty(), "{:?}", shared.deferred());
    shared.clone().validate().unwrap();
    let [left, right] = shared.receiver_call_return_ends() else {
        panic!("one normal end per source")
    };
    assert_eq!(
        left.receiver().call().expression(),
        right.receiver().call().expression()
    );
    assert_eq!(left.receiver().call().source_unit(), accepted.source(0));
    assert_eq!(right.receiver().call().source_unit(), accepted.source(1));
    assert_ne!(left.receiver().call(), right.receiver().call());
    assert_ne!(left.receiver().target(), right.receiver().target());
    let raw = accepted.dataflow(&accepted.parts(&shared));
    assert_eq!(
        raw.receiver_call_return_ends,
        shared.receiver_call_return_ends()
    );
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_borrowed_receiver_return_table() {
    let text = "class Node { fun touch(text: String, own next: Int): Int = next }\nfun run(node: Node): Unit { node.touch(\"ready\", 0) }";
    assert_rejections_clear(text, false, |_, raw| {
        assert_eq!(raw.receiver_call_return_ends.len(), 1);
    });
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_borrowed_receiver_prefix_table() {
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let text = format!(
            "class Node {{ fun touch(text: String, own next: Int): Int = next }}\nfun run(node: Node, flag: Boolean): Unit {{ {opener} node.touch(\"prefix\", {jump}) {closer} }}"
        );
        assert_rejections_clear(&text, false, |_, raw| {
            assert_eq!(raw.receiver_call_prefix_ends.len(), 1);
        });
    }
}

#[test]
fn borrowed_receiver_prefixes_keep_identical_ast_indices_in_distinct_sources_separate() {
    let accepted = Fixture::new(&[
        (
            "a.ko",
            "class Left { fun read(own next: Int): Int = next }\nfun first(node: Left): Unit { node.read(return) }",
        ),
        (
            "b.ko",
            "class Rght { fun read(own next: Int): Int = next }\nfun other(node: Rght): Unit { node.read(return) }",
        ),
    ]);
    let shared = accepted.shared();
    assert!(
        shared.diagnostics().is_empty(),
        "{:?}",
        shared.diagnostics()
    );
    assert!(shared.deferred().is_empty(), "{:?}", shared.deferred());
    shared.clone().validate().unwrap();
    let [left, right] = shared.receiver_call_prefix_ends() else {
        panic!("one abandoned reborrow per source")
    };
    assert_eq!(
        left.receiver().call().expression(),
        right.receiver().call().expression()
    );
    assert_eq!(left.receiver().call().source_unit(), accepted.source(0));
    assert_eq!(right.receiver().call().source_unit(), accepted.source(1));
    assert_ne!(left.receiver().call(), right.receiver().call());
    assert_ne!(left.receiver().target(), right.receiver().target());
    assert_ne!(left.point(), right.point());
    for event in [left, right] {
        assert_eq!(event.preceding_drops(), 0);
        assert_eq!(event.preceding_call_loan_ends(), 0);
        assert_eq!(event.preceding_receiver_loan_ends(), 0);
    }
    let raw = accepted.dataflow(&accepted.parts(&shared));
    assert_eq!(
        raw.receiver_call_prefix_ends,
        shared.receiver_call_prefix_ends()
    );
}

#[test]
fn real_deferred_and_diagnostic_clear_nonempty_inout_receiver_reborrow_tables() {
    for (opener, closer, next) in [
        ("", "", "0"),
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let text = format!(
            "class Node {{ fun touch(text: String, own next: Int): Int = next }}\nfun run(inout node: Node, flag: Boolean): Unit {{ {opener} node.touch(\"ready\", {next}) {closer} }}"
        );
        assert_rejections_clear(&text, false, |_, raw| {
            assert_eq!(
                raw.receiver_call_return_ends.len(),
                usize::from(next == "0")
            );
            assert_eq!(
                raw.receiver_call_prefix_ends.len(),
                usize::from(next != "0")
            );
        });
    }
}

#[test]
fn inout_receiver_reborrow_tables_keep_same_ast_indices_source_qualified() {
    for next in ["0", "return"] {
        let left = format!(
            "class Left {{ fun read(own next: Int): Int = next }}\nfun first(inout node: Left): Unit {{ node.read({next}) }}"
        );
        let right = format!(
            "class Rght {{ fun read(own next: Int): Int = next }}\nfun other(inout node: Rght): Unit {{ node.read({next}) }}"
        );
        let accepted = Fixture::new(&[("a.ko", &left), ("b.ko", &right)]);
        let shared = accepted.shared();
        assert!(
            shared.diagnostics().is_empty(),
            "{:?}",
            shared.diagnostics()
        );
        assert!(shared.deferred().is_empty(), "{:?}", shared.deferred());
        shared.clone().validate().unwrap();
        let receivers: Vec<_> = shared
            .receiver_call_return_ends()
            .iter()
            .map(|event| event.receiver())
            .chain(
                shared
                    .receiver_call_prefix_ends()
                    .iter()
                    .map(|event| event.receiver()),
            )
            .collect();
        let [first, second] = receivers.as_slice() else {
            panic!("one real end per source")
        };
        assert_eq!(first.call().expression(), second.call().expression());
        assert_eq!(first.call().source_unit(), accepted.source(0));
        assert_eq!(second.call().source_unit(), accepted.source(1));
        assert_ne!(first.call(), second.call());
        assert_ne!(first.target(), second.target());
        assert_eq!(
            shared.receiver_call_return_ends().len(),
            if next == "0" { 2 } else { 0 }
        );
        assert_eq!(
            shared.receiver_call_prefix_ends().len(),
            if next == "return" { 2 } else { 0 }
        );
        let raw = accepted.dataflow(&accepted.parts(&shared));
        assert_eq!(
            raw.receiver_call_return_ends,
            shared.receiver_call_return_ends()
        );
        assert_eq!(
            raw.receiver_call_prefix_ends,
            shared.receiver_call_prefix_ends()
        );
    }
}

#[test]
fn real_rejections_clear_nonempty_this_receiver_reborrow_tables() {
    for (opener, closer, next) in [
        ("", "", "0"),
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let text = format!(
            "class Node {{ fun touch(text: String, own next: Int): Int = next; fun run(flag: Boolean): Unit {{ {opener} this.touch(\"ready\", {next}) {closer} }} }}"
        );
        assert_rejections_clear(&text, false, |_, raw| {
            assert_eq!(
                raw.receiver_call_return_ends.len(),
                usize::from(next == "0")
            );
            assert_eq!(
                raw.receiver_call_prefix_ends.len(),
                usize::from(next != "0")
            );
        });
    }
}

#[test]
fn this_reborrow_ends_keep_same_ast_indices_and_different_nominal_owners() {
    for operand in ["", "((this))."] {
        for next in ["0", "return"] {
            let left = format!(
                "class Left {{ fun read(own next: Int): Int = next; fun run(): Unit {{ {operand}read({next}) }} }}"
            );
            let right = format!(
                "class Rght {{ fun read(own next: Int): Int = next; fun run(): Unit {{ {operand}read({next}) }} }}"
            );
            let accepted = Fixture::new(&[("a.ko", &left), ("b.ko", &right)]);
            let shared = accepted.shared();
            assert!(
                shared.diagnostics().is_empty(),
                "{:?}",
                shared.diagnostics()
            );
            assert!(shared.deferred().is_empty(), "{:?}", shared.deferred());
            shared.clone().validate().unwrap();
            let receivers: Vec<_> = shared
                .receiver_call_return_ends()
                .iter()
                .map(|event| event.receiver())
                .chain(
                    shared
                        .receiver_call_prefix_ends()
                        .iter()
                        .map(|event| event.receiver()),
                )
                .collect();
            let [first, second] = receivers.as_slice() else {
                panic!("one real end per source")
            };
            assert_eq!(first.call().expression(), second.call().expression());
            assert_eq!(first.call().source_unit(), accepted.source(0));
            assert_eq!(second.call().source_unit(), accepted.source(1));
            assert_ne!(first.call(), second.call());
            assert!(matches!(
                first.target(),
                crate::ownership_checking::UnitReceiverOwnershipTarget::This(_)
            ));
            assert!(matches!(
                second.target(),
                crate::ownership_checking::UnitReceiverOwnershipTarget::This(_)
            ));
            assert_ne!(first.target(), second.target());
            assert_eq!(
                shared.receiver_call_return_ends().len(),
                if next == "0" { 2 } else { 0 }
            );
            assert_eq!(
                shared.receiver_call_prefix_ends().len(),
                if next == "return" { 2 } else { 0 }
            );
            let raw = accepted.dataflow(&accepted.parts(&shared));
            assert_eq!(
                raw.receiver_call_return_ends,
                shared.receiver_call_return_ends()
            );
            assert_eq!(
                raw.receiver_call_prefix_ends,
                shared.receiver_call_prefix_ends()
            );
        }
    }
}

#[test]
fn real_rejections_clear_nonempty_field_receiver_reborrow_tables() {
    for (open, close, next) in [
        ("", "", "0"),
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let text = format!(
            "class Node {{ fun touch(text: String, own next: Int): Int = next }}\nclass Holder(val node: Node)\nfun run(holder: Holder, flag: Boolean): Unit {{ {open} holder.node.touch(\"ready\", {next}) {close} }}"
        );
        assert_rejections_clear(&text, false, |_, raw| {
            assert_eq!(
                raw.receiver_call_return_ends.len(),
                usize::from(next == "0")
            );
            assert_eq!(
                raw.receiver_call_prefix_ends.len(),
                usize::from(next != "0")
            );
        });
    }
}

#[test]
fn field_receiver_ends_keep_consumer_roots_and_provider_field_identity() {
    use crate::ownership_checking::UnitReceiverOwnershipTarget;
    let types = "class Node { fun read(own next: Int): Int = next }\nclass Holder(val node: Node)";
    for next in ["0", "return"] {
        let left = format!("fun first(holder: Holder): Unit {{ holder.node.read({next}) }}");
        let right = format!("fun other(holder: Holder): Unit {{ holder.node.read({next}) }}");
        let accepted = Fixture::new(&[("a.ko", types), ("b.ko", &left), ("c.ko", &right)]);
        let shared = accepted.shared();
        assert!(
            shared.diagnostics().is_empty(),
            "{:?}",
            shared.diagnostics()
        );
        assert!(shared.deferred().is_empty(), "{:?}", shared.deferred());
        shared.clone().validate().unwrap();
        let receivers: Vec<_> = shared
            .receiver_call_return_ends()
            .iter()
            .map(|e| e.receiver())
            .chain(
                shared
                    .receiver_call_prefix_ends()
                    .iter()
                    .map(|e| e.receiver()),
            )
            .collect();
        let [first, second] = receivers.as_slice() else {
            panic!("one real end per consumer")
        };
        assert_eq!(first.call().expression(), second.call().expression());
        assert_eq!(first.call().source_unit(), accepted.source(1));
        assert_eq!(second.call().source_unit(), accepted.source(2));
        assert_ne!(first.call(), second.call());
        let UnitReceiverOwnershipTarget::Place(first_path) = first.target() else {
            panic!("first path")
        };
        let UnitReceiverOwnershipTarget::Place(second_path) = second.target() else {
            panic!("second path")
        };
        assert_eq!(first_path.root().source_unit(), accepted.source(1));
        assert_eq!(second_path.root().source_unit(), accepted.source(2));
        assert_ne!(first_path.root(), second_path.root());
        assert_eq!(first_path.fields().len(), 1);
        assert_eq!(first_path.fields(), second_path.fields());
        assert_eq!(first_path.fields()[0].source_unit(), accepted.source(0));
        assert_eq!(
            shared.receiver_call_return_ends().len(),
            if next == "0" { 2 } else { 0 }
        );
        assert_eq!(
            shared.receiver_call_prefix_ends().len(),
            if next == "return" { 2 } else { 0 }
        );
        let raw = accepted.dataflow(&accepted.parts(&shared));
        assert_eq!(
            raw.receiver_call_return_ends,
            shared.receiver_call_return_ends()
        );
        assert_eq!(
            raw.receiver_call_prefix_ends,
            shared.receiver_call_prefix_ends()
        );
    }
}

#[test]
fn real_rejections_clear_nonempty_direct_binding_receiver_tables() {
    for (open, close, next) in [
        ("", "", "0"),
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let text = format!(
            "class Node {{ fun touch(text: String, own next: Int): Int = next }}\nfun run(node: Node, flag: Boolean): Unit {{ borrow val borrowed = node; {open} borrowed.touch(\"ready\", {next}) {close} }}"
        );
        assert_rejections_clear(&text, false, |_, raw| {
            assert_eq!(raw.borrow_results.bindings.len(), 1);
            assert_eq!(
                raw.receiver_call_return_ends.len(),
                usize::from(next == "0")
            );
            assert_eq!(
                raw.receiver_call_prefix_ends.len(),
                usize::from(next != "0")
            );
        });
    }
}

#[test]
fn direct_binding_receiver_ends_distinguish_same_index_aliases_and_source_leases() {
    use crate::ownership_checking::{UnitLoanTarget, UnitReceiverOwnershipTarget};
    let types = "class Node { fun read(own next: Int): Int = next }";
    for next in ["0", "return"] {
        let left = format!(
            "fun first(node: Node): Unit {{ borrow val borrowed = node; borrowed.read({next}) }}"
        );
        let right = format!(
            "fun other(node: Node): Unit {{ borrow val borrowed = node; borrowed.read({next}) }}"
        );
        let accepted = Fixture::new(&[("a.ko", types), ("b.ko", &left), ("c.ko", &right)]);
        let shared = accepted.shared();
        assert!(
            shared.diagnostics().is_empty(),
            "{:?}",
            shared.diagnostics()
        );
        assert!(shared.deferred().is_empty(), "{:?}", shared.deferred());
        shared.clone().validate().unwrap();
        let receivers: Vec<_> = shared
            .receiver_call_return_ends()
            .iter()
            .map(|e| e.receiver())
            .chain(
                shared
                    .receiver_call_prefix_ends()
                    .iter()
                    .map(|e| e.receiver()),
            )
            .collect();
        let [first, second] = receivers.as_slice() else {
            panic!("one real end per source")
        };
        assert_eq!(first.call().expression(), second.call().expression());
        assert_ne!(first.call(), second.call());
        assert_ne!(first.target(), second.target());
        let [first_binding, second_binding] = shared.borrow_results().bindings() else {
            panic!("one binding per source")
        };
        assert_eq!(
            first_binding.binding().symbol(),
            second_binding.binding().symbol()
        );
        assert_ne!(first_binding.binding(), second_binding.binding());
        for (source, receiver, binding) in [(1, first, first_binding), (2, second, second_binding)]
        {
            assert_eq!(receiver.call().source_unit(), accepted.source(source));
            assert_eq!(binding.binding().source_unit(), accepted.source(source));
            let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
                panic!("source lease")
            };
            assert_eq!(place.root(), accepted.symbol(source, "node"));
            assert_eq!(binding.binding(), accepted.symbol(source, "borrowed"));
            assert_ne!(place.root(), binding.binding());
            assert_eq!(binding.origin(), &UnitLoanTarget::Place(place.clone()));
            assert_eq!(binding.initializer().source_unit(), accepted.source(source));
        }
        assert_eq!(
            shared.receiver_call_return_ends().len(),
            if next == "0" { 2 } else { 0 }
        );
        assert_eq!(
            shared.receiver_call_prefix_ends().len(),
            if next == "return" { 2 } else { 0 }
        );
        let raw = accepted.dataflow(&accepted.parts(&shared));
        assert_eq!(
            raw.receiver_call_return_ends,
            shared.receiver_call_return_ends()
        );
        assert_eq!(
            raw.receiver_call_prefix_ends,
            shared.receiver_call_prefix_ends()
        );
    }
}
