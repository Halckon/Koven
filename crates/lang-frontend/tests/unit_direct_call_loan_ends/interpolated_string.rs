//! Completed interpolation owns its outer String; inner values are not that loan.
use super::*;

fn span(text: &str, marker: &str) -> ByteSpan {
    let positions: Vec<_> = text.match_indices(marker).map(|(start, _)| start).collect();
    let [start] = positions.as_slice() else {
        panic!("unique {marker:?}: {positions:?}");
    };
    (*start, *start + marker.len())
}

fn end(
    text: &str,
    call: &str,
    argument: &str,
    parameter: &str,
    point: ExpectedPoint,
) -> ExpectedEnd {
    let call_bytes = span(text, call);
    let argument_bytes = span(call, argument);
    ExpectedEnd {
        call: call_bytes,
        argument: (
            call_bytes.0 + argument_bytes.0,
            call_bytes.0 + argument_bytes.1,
        ),
        parameter: span(text, parameter),
        point,
        target: ExpectedTarget::Temporary,
    }
}

fn assert_outer_drops(observed: &Observed) {
    for unit in &observed.units {
        for event in unit.direct_call_loan_ends() {
            let loan = event.loan();
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            let matching: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| {
                    drop.target() == UnitDropTarget::Temporary(loan.argument())
                        && drop.point() == event.point()
                })
                .collect();
            assert_eq!(
                matching.len(),
                1,
                "exactly one matching outer String cleanup"
            );
        }
    }
}

#[test]
fn completed_interpolation_retires_its_actual_temporary_loan() {
    for argument in [r#""value=${1}""#, r#"(("value=${1}"))"#] {
        let call = format!("read({argument})");
        let text = format!("fun read(item: String): Unit {{}}\nfun run(): Unit {{ {call} }}");
        let observed = observe(&text);
        assert_units_admitted(&observed);
        assert_fixed_ends(
            &observed,
            &[end(
                &text,
                &call,
                argument,
                "item: String",
                ExpectedPoint::CallReturn,
            )],
        );
        assert_outer_drops(&observed);
        for unit in &observed.units {
            let [loan] = unit.loans() else {
                panic!("one real Shared loan");
            };
            let [event] = unit.direct_call_loan_ends() else {
                panic!("one actual completed interpolation end");
            };
            assert_eq!(event.loan(), loan);
            assert_eq!(event.preceding_call_loan_ends(), 0);
        }
    }
}

#[test]
fn completed_interpolation_unwinds_before_its_outer_owner_on_each_exit() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
    for jump in ["return", "break", "continue"] {
        let argument = r#""value=${1}""#;
        let call = format!("read({argument}, {jump})");
        let body = if jump == "return" {
            call.clone()
        } else {
            format!("for (_ in listOf(1)) {{ {call} }}")
        };
        let text =
            format!("fun read(item: String, tail: Unit): Unit {{}}\nfun run(): Unit {{ {body} }}");
        let point = match jump {
            "return" => ExpectedPoint::Return(span(&text, jump)),
            "break" => ExpectedPoint::Break(span(&text, jump)),
            _ => ExpectedPoint::Continue(span(&text, jump)),
        };
        let observed = observe(&text);
        assert_units_admitted(&observed);
        assert_fixed_ends(
            &observed,
            &[end(&text, &call, argument, "item: String", point)],
        );
        assert_outer_drops(&observed);
        for unit in &observed.units {
            let [event] = unit.direct_call_loan_ends() else {
                panic!("one actual unwind");
            };
            assert_eq!(event.preceding_call_loan_ends(), 0);
            if jump != "return" {
                let actions = unit
                    .iteration_cleanup_at(event.point())
                    .expect("actual for exit");
                let ends: Vec<_> = actions
                    .iter()
                    .enumerate()
                    .filter_map(|(index, action)| {
                        matches!(action, Action::EndCallLoan(loan) if loan == event.loan())
                            .then_some(index)
                    })
                    .collect();
                let [end] = ends.as_slice() else {
                    panic!("same token exactly once");
                };
                let drop = actions
                    .iter()
                    .position(|action| {
                        matches!(action, Action::Drop(drop)
                    if drop.target() == UnitDropTarget::Temporary(event.loan().argument()))
                    })
                    .unwrap();
                let element = actions
                    .iter()
                    .position(|action| matches!(action, Action::EndElement(_)))
                    .unwrap();
                assert!(*end < drop && drop < element);
            }
        }
    }
}

#[test]
fn partial_interpolation_keeps_inner_cleanup_separate_from_outer_call_end() {
    let argument = r#""${"inner"}-${if (flag) { return } else { 0 }}""#;
    let call = format!("read({argument})");
    let text =
        format!("fun read(item: String): Unit {{}}\nfun run(flag: Boolean): Unit {{ {call} }}");
    let observed = observe(&text);
    assert_units_admitted(&observed);
    assert_fixed_ends(
        &observed,
        &[end(
            &text,
            &call,
            argument,
            "item: String",
            ExpectedPoint::CallReturn,
        )],
    );
    assert_outer_drops(&observed);
    for unit in &observed.units {
        let [event] = unit.direct_call_loan_ends() else {
            panic!("only completed outer String is borrowed");
        };
        let outer = event.loan().argument();
        let outer_drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(outer))
            .collect();
        assert_eq!(outer_drops.len(), 1);
        let inner_drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| observed.sources.slice(drop.value_origin()) == Ok("\"inner\""))
            .collect();
        assert_eq!(inner_drops.len(), 2);
        assert!(matches!(
            inner_drops[0].point(),
            UnitDropPoint::ControlTransfer(_)
        ));
        assert_eq!(
            inner_drops[1].point(),
            UnitDropPoint::AfterExpression(outer)
        );
        assert!(
            inner_drops
                .iter()
                .all(|drop| drop.target() != UnitDropTarget::Temporary(outer))
        );
        assert_eq!(event.preceding_call_loan_ends(), 0);
    }
}

#[test]
fn uncompleted_and_unvisited_interpolation_do_not_invent_outer_tokens() {
    for text in [
        r#"fun read(item: String): Unit {}
fun run(): Unit { read("${"inner"}-${return}") }"#,
        r#"fun read(tail: Unit, item: String): Unit {}
fun run(): Unit { read(return, "value=${1}") }"#,
    ] {
        let observed = observe(text);
        assert_units_admitted(&observed);
        assert_fixed_ends(&observed, &[]);
        for unit in &observed.units {
            assert!(unit.loans().is_empty(), "no completed outer argument token");
            if text.contains("inner") {
                let [drop] = unit.drops() else {
                    panic!("only the completed inner input is cleaned");
                };
                assert_eq!(observed.sources.slice(drop.value_origin()), Ok("\"inner\""));
                assert!(matches!(drop.point(), UnitDropPoint::ControlTransfer(_)));
            } else {
                assert!(
                    unit.drops().is_empty(),
                    "unvisited interpolation has no owner"
                );
            }
        }
    }
}

#[test]
fn nested_interpolation_call_is_retired_once_before_later_outer_unwind() {
    let inner_argument = r#""inner=${1}""#;
    let inner_call = format!("count({inner_argument})");
    let outer_argument = format!("\"value=${{{inner_call}}}\"");
    let outer_call = format!("read({outer_argument}, return)");
    let text = format!(
        "fun count(value: String): Int = 1\nfun read(item: String, tail: Unit): Unit {{}}\nfun run(): Unit {{ {outer_call} }}"
    );
    let observed = observe(&text);
    assert_units_admitted(&observed);
    assert_fixed_ends(
        &observed,
        &[
            end(
                &text,
                &inner_call,
                inner_argument,
                "value: String",
                ExpectedPoint::CallReturn,
            ),
            end(
                &text,
                &outer_call,
                &outer_argument,
                "item: String",
                ExpectedPoint::Return(span(&text, "return")),
            ),
        ],
    );
    assert_outer_drops(&observed);
    for unit in &observed.units {
        let [inner, outer] = unit.direct_call_loan_ends() else {
            panic!("one end for each completed argument");
        };
        assert_ne!(inner.loan().argument(), outer.loan().argument());
        assert_ne!(inner.point(), outer.point());
        assert_eq!(inner.preceding_call_loan_ends(), 0);
        assert_eq!(outer.preceding_call_loan_ends(), 0);
    }
}

#[test]
fn named_interpolations_keep_reverse_order_across_an_exclusive_token() {
    let first = r#""first=${1}""#;
    let last = r#""last=${2}""#;
    let call = format!("mix(last = {last}, count = &count, first = {first}, tail = return)");
    let text = format!(
        "fun mix(first: String, inout count: Int, last: String, tail: Unit): Unit {{}}\nfun run(): Unit {{ var count = 0; {call} }}"
    );
    let point = ExpectedPoint::Return(span(&text, "return"));
    let observed = observe(&text);
    assert_units_admitted(&observed);
    assert_fixed_ends(
        &observed,
        &[
            end(&text, &call, first, "first: String", point),
            end(&text, &call, last, "last: String", point),
        ],
    );
    assert_outer_drops(&observed);
    for unit in &observed.units {
        let [first, last] = unit.direct_call_loan_ends() else {
            panic!("two completed String operands");
        };
        assert_eq!(first.preceding_call_loan_ends(), 0);
        assert_eq!(last.preceding_call_loan_ends(), 2);
        let [exclusive] = unit.exclusive_call_prefix_ends() else {
            panic!("actual middle Exclusive token");
        };
        assert_eq!(exclusive.preceding_call_loan_ends(), 1);
        assert_eq!(first.point(), exclusive.point());
        assert_eq!(last.point(), exclusive.point());
    }
}

#[test]
fn inner_loop_control_does_not_end_an_already_completed_outer_argument() {
    for inner in ["loop { break }; 1", "while (flag) { continue }; 1"] {
        let first = r#""first=${1}""#;
        let second = format!("\"second=${{if (flag) {{ {inner} }} else {{ 2 }}}}\"");
        let call = format!("read({first}, {second})");
        let text = format!(
            "fun read(first: String, second: String): Unit {{}}\nfun run(flag: Boolean): Unit {{ {call} }}"
        );
        let observed = observe(&text);
        assert_units_admitted(&observed);
        assert_fixed_ends(
            &observed,
            &[
                end(
                    &text,
                    &call,
                    &second,
                    "second: String",
                    ExpectedPoint::CallReturn,
                ),
                end(
                    &text,
                    &call,
                    first,
                    "first: String",
                    ExpectedPoint::CallReturn,
                ),
            ],
        );
        assert_outer_drops(&observed);
        for unit in &observed.units {
            assert_eq!(
                unit.direct_call_loan_ends()[0].preceding_call_loan_ends(),
                0
            );
            assert_eq!(
                unit.direct_call_loan_ends()[1].preceding_call_loan_ends(),
                1
            );
        }
    }
}

#[test]
fn rejected_units_do_not_publish_interpolated_string_ends() {
    let prefix = "fun read(item: String): Unit {}\nfun run(): Unit { read(\"value=${1}\") }";
    let good = observe(prefix);
    assert_units_admitted(&good);
    for unit in &good.units {
        assert_eq!(unit.direct_call_loan_ends().len(), 1);
    }
    for (suffix, deferred) in [
        ("fun rejected(): Unit { val target = 0; target = 1 }", false),
        (
            "fun rejected(): Unit { var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
            true,
        ),
    ] {
        let rejected = observe(&format!("{prefix}\n{suffix}"));
        for unit in &rejected.units {
            if deferred {
                assert!(unit.diagnostics().is_empty());
                let [fact] = unit.deferred() else {
                    panic!("one actual deferred operation");
                };
                assert_eq!(
                    fact.reason(),
                    OwnershipDeferredReason::OwnershipPrimitiveClosureTransport
                );
            } else {
                assert!(unit.deferred().is_empty());
                let [diagnostic] = unit.diagnostics() else {
                    panic!("one actual immutable inout error");
                };
                assert_eq!(diagnostic.code().to_string(), "L0134");
                assert_eq!(
                    rejected.sources.slice(diagnostic.primary_span()),
                    Ok("target")
                );
            }
            assert!(unit.direct_call_loan_ends().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn other_owned_string_expressions_keep_their_original_observation_boundary() {
    for operand in ["factory()", r#""left" + "right""#] {
        let text = format!(
            "fun factory(): String = \"made\"\nfun read(item: String): Unit {{}}\nfun run(): Unit {{ read({operand}) }}"
        );
        let observed = observe(&text);
        assert_units_admitted(&observed);
        assert_fixed_ends(&observed, &[]);
        for unit in &observed.units {
            let [loan] = unit.loans() else {
                panic!("the excluded operand actually forms a loan");
            };
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            assert_eq!(loan.kind(), LoanKind::Shared);
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Temporary(loan.argument()))
                .collect();
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].point(), UnitDropPoint::CallReturn(loan.call()));
        }
    }
}
