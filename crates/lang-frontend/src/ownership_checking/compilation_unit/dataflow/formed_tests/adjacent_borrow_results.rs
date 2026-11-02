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
