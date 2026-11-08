//! 三种已发布切片共享实际 EndCallLoan 的同点顺序，不冒充完整 loan/cleanup 流。
use super::*;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Event {
    point: ByteSpan,
    rank: usize,
    slice: &'static str,
    call: ByteSpan,
    argument: ByteSpan,
    parameter: ByteSpan,
    preceding_drops: usize,
}

fn unique_span(text: &str, part: &str) -> ByteSpan {
    let starts: Vec<_> = text.match_indices(part).map(|(start, _)| start).collect();
    let [start] = starts.as_slice() else {
        panic!("expected one independent source marker {part:?}: {starts:?}");
    };
    (*start, *start + part.len())
}

fn expected(
    text: &str,
    point: ByteSpan,
    rank: usize,
    slice: &'static str,
    call: &str,
    argument: &str,
    parameter: &str,
) -> Event {
    let call_span = unique_span(text, call);
    let argument_span = if slice == "exclusive" {
        let marker = unique_span(call, &format!("&{argument}"));
        (marker.0 + 1, marker.1)
    } else {
        unique_span(call, argument)
    };
    Event {
        point,
        rank,
        slice,
        call: call_span,
        argument: (call_span.0 + argument_span.0, call_span.0 + argument_span.1),
        parameter: unique_span(text, parameter),
        preceding_drops: 0,
    }
}

fn assert_events(text: &str, mut expected: Vec<Event>) -> Observed {
    let observed = observe(text);
    assert_units_admitted(&observed);
    expected.sort();
    for (entry, (unit, names)) in observed.units.iter().zip(&observed.unit_names).enumerate() {
        let [source] = names.names().index().source_units() else {
            panic!("one actual source");
        };
        assert_eq!(source.source_id(), observed.parsed.source_id());
        match (entry, source.key()) {
            (0, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "call_ends.ko");
            }
            (1, SourceUnitKey::MemoryDocument) => {}
            other => panic!("unexpected source key: {other:?}"),
        }
        let bytes = |span: Span| {
            assert_eq!(span.source_id(), observed.parsed.source_id());
            (span.start(), span.end())
        };
        let node = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), source.id());
            observed
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
        };
        let project = |slice,
                       loan: &lang_frontend::ownership_checking::UnitLoanFact,
                       point,
                       rank,
                       preceding_drops| {
            assert!(unit.loans().contains(loan));
            assert_eq!(
                loan.kind(),
                if slice == "exclusive" {
                    LoanKind::Exclusive
                } else {
                    LoanKind::Shared
                }
            );
            let call = node(loan.call());
            let argument = node(loan.argument());
            let Expression::Call { arguments, .. } = call.payload() else {
                panic!("actual call");
            };
            let actual_argument = arguments
                .iter()
                .find(|arg| arg.value == loan.argument().expression())
                .unwrap();
            if slice == "exclusive" {
                let Some(lang_frontend::parser::ParameterModeMarker::Inout(marker)) =
                    actual_argument.mode_marker
                else {
                    panic!("explicit Inout marker");
                };
                assert_eq!(loan.begin_span(), marker);
                let UnitLoanTarget::Place(place) = loan.target() else {
                    panic!("owned root place");
                };
                assert!(place.is_root());
                assert_eq!(place.root().source_unit(), source.id());
                assert!(unit.formed_owned_locals().contains(&place.root()));
            } else {
                assert_eq!(loan.begin_span(), argument.span());
            }
            assert_eq!(loan.end_span(), call.span());
            let UnitDropPoint::ControlTransfer(transfer) = point else {
                panic!("only actual Return endpoints expected");
            };
            let transfer = node(transfer);
            assert!(matches!(transfer.payload(), Expression::Return { .. }));
            assert_ne!(transfer.span(), loan.end_span());
            Event {
                point: bytes(transfer.span()),
                rank,
                slice,
                call: bytes(call.span()),
                argument: bytes(argument.span()),
                parameter: bytes(loan.parameter_span().unwrap()),
                preceding_drops,
            }
        };
        let mut actual = Vec::new();
        actual.extend(unit.direct_call_loan_ends().iter().map(|event| {
            project(
                "direct",
                event.loan(),
                event.point(),
                event.preceding_call_loan_ends(),
                event.preceding_drops(),
            )
        }));
        actual.extend(unit.exclusive_call_prefix_ends().iter().map(|event| {
            project(
                "exclusive",
                event.loan(),
                event.point(),
                event.preceding_call_loan_ends(),
                event.preceding_drops(),
            )
        }));
        actual.extend(unit.borrow_call_prefix_ends().iter().map(|event| {
            project(
                "source",
                event.loan(),
                event.point(),
                event.preceding_call_loan_ends(),
                event.preceding_drops(),
            )
        }));
        // Check each published slice before merging by rank; sorting must not mask
        // a regression in the producer's own publication order or duplicate facts.
        for slice in ["direct", "exclusive", "source"] {
            let published: Vec<_> = actual.iter().filter(|event| event.slice == slice).collect();
            let fixed: Vec<_> = expected
                .iter()
                .filter(|event| event.slice == slice)
                .collect();
            assert_eq!(published, fixed, "{slice}: {text}");
        }
        actual.sort();
        assert_eq!(actual, expected, "{text}");
        assert!(unit.borrow_call_handoffs().is_empty());
        assert!(unit.borrow_results().bindings().is_empty());
    }
    observed
}

// 与旧 prefix_order 见证逐字相同；不替换其 Shared 序号/缺口 oracle。
const MIXED: &str = "fun select(source: String, tail: Unit): borrow String from source = source\nfun sink(first: String, inout count: Int, last: String, stop: Unit): Unit {}\nfun run(): Unit { val owner = \"kept\"; var count = 0; borrow val item = select(owner, sink(\"first\", &count, \"last\", return)) }";

#[test]
fn exclusive_rank_connects_the_existing_shared_and_source_order() {
    let point = unique_span(MIXED, "return");
    let call = "sink(\"first\", &count, \"last\", return)";
    assert_events(
        MIXED,
        vec![
            expected(MIXED, point, 0, "direct", call, "\"last\"", "last: String"),
            expected(
                MIXED,
                point,
                1,
                "exclusive",
                call,
                "count",
                "inout count: Int",
            ),
            expected(
                MIXED,
                point,
                2,
                "direct",
                call,
                "\"first\"",
                "first: String",
            ),
            expected(
                MIXED,
                point,
                3,
                "source",
                "select(owner, sink(\"first\", &count, \"last\", return))",
                "owner",
                "source: String",
            ),
        ],
    );
}

#[test]
fn excluded_projection_tokens_still_count_between_exclusive_roots() {
    let text = "class Holder(var value: Int)\nfun select(source: String, tail: Unit): borrow String from source = source\nfun sink(inout first: Int, inout hidden: Int, inout last: Int, stop: Unit): Unit {}\nfun run(): Unit { val owner = \"kept\"; val holder = Holder(0); var a = 1; var b = 2; borrow val item = select(owner, sink(&a, &holder.value, &b, return)) }";
    let point = unique_span(text, "return");
    let call = "sink(&a, &holder.value, &b, return)";
    let observed = assert_events(
        text,
        vec![
            expected(text, point, 0, "exclusive", call, "b", "inout last: Int"),
            expected(text, point, 2, "exclusive", call, "a", "inout first: Int"),
            expected(
                text,
                point,
                3,
                "source",
                "select(owner, sink(&a, &holder.value, &b, return))",
                "owner",
                "source: String",
            ),
        ],
    );
    for unit in &observed.units {
        assert_eq!(
            unit.loans()
                .iter()
                .filter(|loan| matches!(loan.target(),
            UnitLoanTarget::Place(place) if !place.is_root()))
                .count(),
            1,
            "the excluded projection really establishes its own loan"
        );
    }
}

#[test]
fn each_return_branch_has_an_independent_rank_without_normal_return_leakage() {
    let text = "fun touch(inout value: Int): Unit {}\nfun select(source: String, tail: Unit): borrow String from source = source\nfun sink(inout count: Int, stop: Unit): Unit {}\nfun run(flag: Boolean): Unit { val owner = \"kept\"; var count = 0; touch(&count); borrow val item = select(owner, sink(&count, if (flag) { return } else { return })) }";
    let mut events = Vec::new();
    for (start, _) in text.match_indices("return") {
        let point = (start, start + "return".len());
        events.push(expected(
            text,
            point,
            0,
            "exclusive",
            "sink(&count, if (flag) { return } else { return })",
            "count",
            "inout count: Int",
        ));
        events.push(expected(
            text,
            point,
            1,
            "source",
            "select(owner, sink(&count, if (flag) { return } else { return }))",
            "owner",
            "source: String",
        ));
    }
    assert_eq!(events.len(), 4);
    let observed = assert_events(text, events);
    for unit in &observed.units {
        assert!(
            unit.loans().iter().any(|loan| {
                let call = observed
                    .parsed
                    .ast()
                    .expressions()
                    .get(loan.call().expression())
                    .unwrap();
                observed.sources.slice(call.span()) == Ok("touch(&count)")
            }),
            "the completed Exclusive call exists but has no Return-prefix event"
        );
    }
}

#[test]
fn later_error_or_deferred_work_revokes_the_ranked_event_slices() {
    exclusive_rank_connects_the_existing_shared_and_source_order();
    for (suffix, error) in [
        (
            "\nfun bad() { val value = \"held\"; val moved = value; println(value) }",
            true,
        ),
        (
            "\nfun deferred() { var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
            false,
        ),
    ] {
        let observed = observe(&format!("{MIXED}{suffix}"));
        for unit in &observed.units {
            if error {
                assert!(
                    unit.diagnostics()
                        .iter()
                        .any(|d| d.code().to_string() == "L0131")
                );
            } else {
                assert!(unit.diagnostics().is_empty());
                assert!(!unit.deferred().is_empty());
            }
            assert!(unit.direct_call_loan_ends().is_empty());
            assert!(unit.exclusive_call_prefix_ends().is_empty());
            assert!(unit.borrow_call_prefix_ends().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}
