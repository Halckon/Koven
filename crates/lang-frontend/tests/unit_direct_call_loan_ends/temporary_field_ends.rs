//! Field ends reuse the real call token; adjacent producers and capability gates stay bounded.
use super::*;
use temporary_field_pending::{Cleanup, assert_field_cleanup, expression};

#[derive(Clone, Copy)]
struct End<'a> {
    call: &'a str,
    argument: &'a str,
    parent: &'a str,
    point: Cleanup<'a>,
    rank: usize,
}

fn assert_ends(observed: &Observed, expected: &[End<'_>]) {
    assert_units_admitted(observed);
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [source] = names.names().index().source_units() else {
            panic!("one independently indexed source")
        };
        let qualify = |text| UnitExpressionId::new(source.id(), expression(observed, text));
        let expected = expected
            .iter()
            .map(|end| {
                let point = match end.point {
                    Cleanup::Returned(call) => UnitDropPoint::CallReturn(qualify(call)),
                    Cleanup::Transfer(transfer) => {
                        UnitDropPoint::ControlTransfer(qualify(transfer))
                    }
                };
                (
                    qualify(end.call),
                    qualify(end.argument),
                    UnitLoanTarget::Temporary(qualify(end.parent)),
                    point,
                    0,
                    end.rank,
                )
            })
            .collect::<Vec<_>>();
        let actual = unit
            .direct_call_loan_ends()
            .iter()
            .map(|end| {
                let loan = end.loan();
                assert!(
                    unit.loans().contains(loan),
                    "the event carries the actual registered token"
                );
                assert_eq!(loan.kind(), LoanKind::Shared);
                let contracts = unit
                    .call_argument_contracts_for(loan.call())
                    .filter(|contract| contract.argument() == loan.argument())
                    .collect::<Vec<_>>();
                let [contract] = contracts.as_slice() else {
                    panic!("one selected argument contract")
                };
                assert_eq!(contract.parameter_span(), loan.parameter_span());
                assert!(loan.parameter_span().is_some());
                assert_eq!(
                    loan.begin_span(),
                    observed
                        .parsed
                        .ast()
                        .expressions()
                        .get(loan.argument().expression())
                        .unwrap()
                        .span()
                );
                assert_eq!(
                    loan.end_span(),
                    observed
                        .parsed
                        .ast()
                        .expressions()
                        .get(loan.call().expression())
                        .unwrap()
                        .span()
                );
                (
                    loan.call(),
                    loan.argument(),
                    loan.target().clone(),
                    end.point(),
                    end.preceding_drops(),
                    end.preceding_call_loan_ends(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "complete ordered event vector");
    }
}

#[test]
fn factory_field_ends_protect_the_complete_returned_parent() {
    let call = "read((make().inner.text), 0)";
    let observed = observe(&format!(
        "class Holder(val text: String)\nclass Outer(val inner: Holder)\nfun make(): Outer = Outer(Holder(\"field\"))\nfun read(text: String, own n: Int): Unit {{}}\nfun run(): Unit {{ {call} }}"
    ));
    assert_field_cleanup(
        &observed,
        call,
        &[("(make().inner.text)", "make()")],
        &[(Cleanup::Returned(call), "make()")],
    );
}

#[test]
fn completed_nested_call_is_not_ended_again_by_a_later_return() {
    let inner = "number(Holder(\"inner\").text)";
    let outer = format!("read(Holder(\"outer\").text, {inner}, return)");
    let observed = observe(&format!(
        "class Holder(val text: String)\nfun number(text: String): Int = 0\nfun read(text: String, own n: Int, stop: Unit): Unit {{}}\nfun run(): Unit {{ {outer} }}"
    ));
    assert_ends(
        &observed,
        &[
            End {
                call: inner,
                argument: "Holder(\"inner\").text",
                parent: "Holder(\"inner\")",
                point: Cleanup::Returned(inner),
                rank: 0,
            },
            End {
                call: &outer,
                argument: "Holder(\"outer\").text",
                parent: "Holder(\"outer\")",
                point: Cleanup::Transfer("return"),
                rank: 0,
            },
        ],
    );
    for unit in &observed.units {
        let drops = unit
            .drops()
            .iter()
            .map(|drop| observed.sources.slice(drop.value_origin()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(drops, ["Holder(\"inner\")", "Holder(\"outer\")"]);
    }
}

#[test]
fn hidden_exclusive_end_keeps_its_rank_between_field_and_source_ends() {
    let sink = "sink(Holder(\"first\").text, &count, Holder(\"last\").text, return)";
    let observed = observe(&format!(
        "class Holder(val text: String)\nfun select(source: String, tail: Unit): borrow String from source = source\nfun sink(first: String, inout count: Int, last: String, stop: Unit): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; var count = 0; borrow val item = select(owner, {sink}) }}"
    ));
    assert_ends(
        &observed,
        &[
            End {
                call: sink,
                argument: "Holder(\"last\").text",
                parent: "Holder(\"last\")",
                point: Cleanup::Transfer("return"),
                rank: 0,
            },
            End {
                call: sink,
                argument: "Holder(\"first\").text",
                parent: "Holder(\"first\")",
                point: Cleanup::Transfer("return"),
                rank: 2,
            },
        ],
    );
    for unit in &observed.units {
        let [source] = unit.borrow_call_prefix_ends() else {
            panic!("one real continued source unwind")
        };
        assert_eq!(source.preceding_call_loan_ends(), 3);
        assert_eq!(source.preceding_drops(), 0);
        assert_eq!(source.point(), unit.direct_call_loan_ends()[0].point());
    }
}

#[test]
fn borrow_result_calls_do_not_gain_direct_field_end_events() {
    let observed = observe(
        "class Holder(val text: String)\nfun view(source: String, extra: String): borrow String from source = source\nfun read(text: String): Unit {}\nfun run(own owner: String): Unit { borrow val alias = view(owner, Holder(\"auxiliary\").text); read(alias) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert!(
            unit.loans()
                .iter()
                .any(|loan| observed.sources.slice(loan.begin_span())
                    == Ok("Holder(\"auxiliary\").text")
                    && matches!(loan.target(), UnitLoanTarget::Temporary(_)))
        );
        assert!(unit.direct_call_loan_ends().is_empty());
    }
}

#[test]
fn later_error_or_deferred_work_revokes_a_proven_field_event() {
    let prefix = "class Holder(val text: String)\nfun read(text: String): Unit {}\nfun run(): Unit { read(Holder(\"prefix\").text)";
    let admitted = observe(&format!("{prefix} }}"));
    assert_ends(
        &admitted,
        &[End {
            call: "read(Holder(\"prefix\").text)",
            argument: "Holder(\"prefix\").text",
            parent: "Holder(\"prefix\")",
            point: Cleanup::Returned("read(Holder(\"prefix\").text)"),
            rank: 0,
        }],
    );
    for (suffix, diagnostic) in [
        (
            "; val source = \"held\"; val moved = source; println(source)",
            Some("L0131"),
        ),
        (
            "; var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 })",
            None,
        ),
    ] {
        let observed = observe(&format!("{prefix}{suffix} }}"));
        for unit in &observed.units {
            if let Some(code) = diagnostic {
                assert!(
                    unit.diagnostics()
                        .iter()
                        .any(|d| d.code().to_string() == code)
                );
                assert!(unit.drops().is_empty());
            } else {
                assert!(unit.diagnostics().is_empty());
                assert!(!unit.deferred().is_empty());
            }
            assert!(unit.direct_call_loan_ends().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn field_parent_ends_before_iteration_owner_and_provider_cleanup() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
    for transfer in ["return", "break", "continue"] {
        let call = format!("read(Holder(\"field\").text, {transfer})");
        let observed = observe(&format!(
            "class Holder(val text: String) {{ deinit() {{}} }}\nfun read(text: String, stop: Unit): Unit {{}}\nfun run() {{ for (_ in listOf(1)) {{ {call} }} }}"
        ));
        assert_ends(
            &observed,
            &[End {
                call: &call,
                argument: "Holder(\"field\").text",
                parent: "Holder(\"field\")",
                point: Cleanup::Transfer(transfer),
                rank: 0,
            }],
        );
        for unit in &observed.units {
            let [event] = unit.direct_call_loan_ends() else {
                panic!("one established field token")
            };
            let actions = unit
                .iteration_cleanup_at(event.point())
                .expect("real iteration exit");
            let end = actions
                .iter()
                .position(
                    |action| matches!(action, Action::EndCallLoan(loan) if loan == event.loan()),
                )
                .unwrap();
            let UnitLoanTarget::Temporary(parent) = event.loan().target() else {
                panic!("complete parent")
            };
            let drop = actions.iter().position(|action| matches!(action, Action::Drop(drop) if drop.target() == UnitDropTarget::Temporary(*parent))).unwrap();
            let element = actions
                .iter()
                .position(|action| matches!(action, Action::EndElement(_)))
                .unwrap();
            assert!(
                end < drop && drop < element,
                "field loan, complete parent, then iteration cleanup"
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(
                        |action| matches!(action, Action::EndCallLoan(loan) if loan == event.loan())
                    )
                    .count(),
                1
            );
        }
    }
}

#[test]
fn rc_backed_fields_do_not_gain_direct_parent_events() {
    for (field, ty) in [
        ("Rc(Holder(\"field\")).value", "Holder"),
        ("Rc(Holder(\"field\")).value.text", "String"),
    ] {
        let observed = observe(&format!(
            "class Holder(val text: String)\nfun read(value: {ty}): Unit {{}}\nfun run() {{ read({field}) }}"
        ));
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert!(
                unit.loans()
                    .iter()
                    .any(|loan| observed.sources.slice(loan.begin_span()) == Ok(field)),
                "real unsupported projection loan"
            );
            assert!(unit.direct_call_loan_ends().is_empty());
        }
    }
}

#[test]
fn index_backed_field_keeps_its_exact_deferred_boundary() {
    let field = "listOf(Holder(\"field\"))[0].text";
    let observed = observe(&format!(
        "class Holder(val text: String)\nfun read(text: String): Unit {{}}\nfun run() {{ read({field}) }}"
    ));
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        assert!(unit.diagnostics().is_empty());
        let [source] = names.names().index().source_units() else {
            panic!("one source")
        };
        assert_eq!(
            unit.deferred()
                .iter()
                .map(|fact| (fact.expression(), fact.reason()))
                .collect::<Vec<_>>(),
            [(
                UnitExpressionId::new(source.id(), expression(&observed, field)),
                OwnershipDeferredReason::IndexPlace
            )]
        );
        assert!(unit.direct_call_loan_ends().is_empty());
        assert!(unit.clone().validate().is_err());
    }
}
