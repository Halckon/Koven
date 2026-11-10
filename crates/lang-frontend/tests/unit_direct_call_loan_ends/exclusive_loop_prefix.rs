//! Real loop-exit tokens, not descriptors or synthetic cleanup actions.
use super::*;

type Projection = (ByteSpan, ByteSpan, ByteSpan, usize, usize);

fn span(text: &str, marker: &str) -> ByteSpan {
    let starts: Vec<_> = text.match_indices(marker).map(|(start, _)| start).collect();
    let [start] = starts.as_slice() else {
        panic!("unique marker {marker:?}: {starts:?}");
    };
    (*start, *start + marker.len())
}

fn expected(text: &str, call: &str, root: &str, jump: &str, rank: usize) -> Projection {
    let call_span = span(text, call);
    let argument = span(call, &format!("&{root}"));
    (
        call_span,
        (call_span.0 + argument.0 + 1, call_span.0 + argument.1),
        span(text, jump),
        rank,
        0,
    )
}

fn assert_events(text: &str, expected: &[Projection]) -> Observed {
    let observed = observe(text);
    assert_units_admitted(&observed);
    for (entry, (unit, names)) in observed.units.iter().zip(&observed.unit_names).enumerate() {
        let [source] = names.names().index().source_units() else {
            panic!("one source");
        };
        match (entry, source.key()) {
            (0, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "call_ends.ko");
            }
            (1, SourceUnitKey::MemoryDocument) => {}
            other => panic!("unexpected entry: {other:?}"),
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
        let actual: Vec<_> = unit
            .exclusive_call_prefix_ends()
            .iter()
            .map(|event| {
                let loan = event.loan();
                assert!(unit.loans().contains(loan));
                assert_eq!(loan.kind(), LoanKind::Exclusive);
                let call = node(loan.call());
                let argument = node(loan.argument());
                let Expression::Call { arguments, .. } = call.payload() else {
                    panic!("actual call");
                };
                let operand = arguments
                    .iter()
                    .find(|arg| arg.value == loan.argument().expression())
                    .unwrap();
                let Some(lang_frontend::parser::ParameterModeMarker::Inout(marker)) =
                    operand.mode_marker
                else {
                    panic!("actual Inout");
                };
                assert_eq!(loan.begin_span(), marker);
                assert_eq!(observed.sources.slice(marker), Ok("&"));
                assert_eq!(loan.end_span(), call.span());
                let UnitLoanTarget::Place(place) = loan.target() else {
                    panic!("root place");
                };
                assert!(place.is_root());
                assert_eq!(place.root().source_unit(), source.id());
                assert!(unit.formed_owned_locals().contains(&place.root()));
                let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
                    panic!("actual exit");
                };
                let transfer = node(transfer);
                assert!(matches!(
                    transfer.payload(),
                    Expression::Return { .. }
                        | Expression::Break { .. }
                        | Expression::Continue { .. }
                ));
                (
                    bytes(call.span()),
                    bytes(argument.span()),
                    bytes(transfer.span()),
                    event.preceding_call_loan_ends(),
                    event.preceding_drops(),
                )
            })
            .collect();
        assert_eq!(actual, expected, "{text}");
    }
    observed
}

#[test]
fn loop_exit_ends_only_the_established_exclusive_prefix() {
    for jump in ["break", "continue"] {
        let call = format!("replace(&a, {jump})");
        // The original excluded-loop input is retained byte for byte.
        let text = format!(
            "class Item(val n: Int)\nfun run(): Unit {{ loop {{ var a = Item(1); {call} }} }}"
        );
        let observed = assert_events(&text, &[expected(&text, &call, "a", jump, 0)]);
        for unit in &observed.units {
            let [event] = unit.exclusive_call_prefix_ends() else {
                panic!("one actual end");
            };
            let UnitLoanTarget::Place(place) = event.loan().target() else {
                panic!("owned root");
            };
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
                .collect();
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].point(), event.point());
            assert!(unit.ownership_primitives().is_empty());
        }
    }
}

#[test]
fn loop_exit_interleaves_shared_and_exclusive_tokens_in_actual_order() {
    for jump in ["break", "continue"] {
        let call = format!("sink(\"first\", &a, \"last\", {jump})");
        let text = format!(
            "fun sink(first: String, inout value: Int, last: String, stop: Unit): Unit {{}}\nfun run(): Unit {{ loop {{ var a = 0; {call} }} }}"
        );
        let observed = assert_events(&text, &[expected(&text, &call, "a", jump, 1)]);
        for unit in &observed.units {
            let [exclusive] = unit.exclusive_call_prefix_ends() else {
                panic!("one Exclusive");
            };
            let actual: Vec<_> = unit
                .direct_call_loan_ends()
                .iter()
                .map(|event| {
                    assert_eq!(event.point(), exclusive.point());
                    assert_eq!(event.preceding_drops(), 0);
                    (
                        observed.sources.slice(event.loan().begin_span()).unwrap(),
                        event.preceding_call_loan_ends(),
                    )
                })
                .collect();
            assert_eq!(actual, [("\"last\"", 0), ("\"first\"", 2)]);
        }
    }
}

#[test]
fn nested_exclusive_calls_unwind_in_reverse_order_at_the_same_loop_exit() {
    for jump in ["break", "continue"] {
        let inner = format!("replace(&b, {jump})");
        let outer = format!("replace(&a, {inner})");
        let text = format!(
            "class Item(val n: Int)\nfun run(): Unit {{ loop {{ var a = Item(1); var b = Item(2); {outer} }} }}"
        );
        let observed = assert_events(
            &text,
            &[
                expected(&text, &inner, "b", jump, 0),
                expected(&text, &outer, "a", jump, 1),
            ],
        );
        for unit in &observed.units {
            assert!(unit.ownership_primitives().is_empty());
            let events = unit.exclusive_call_prefix_ends();
            let targets: Vec<_> = events
                .iter()
                .map(|event| {
                    let UnitLoanTarget::Place(place) = event.loan().target() else {
                        panic!("root");
                    };
                    UnitDropTarget::Named(place.root())
                })
                .collect();
            let actual: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.point() == events[0].point())
                .map(|drop| drop.target())
                .collect();
            assert_eq!(actual, targets);
        }
    }
}

#[test]
fn an_inner_loop_exit_does_not_end_an_outer_exclusive_call_prefix() {
    for inner in ["loop { break }", "while (flag) { continue }"] {
        let call = format!("edit(&a, if (flag) {{ {inner}; return }} else {{ 0 }})");
        let text = format!(
            "class Item(val n: Int)\nfun edit(inout value: Item, next: Int): Unit {{}}\nfun run(flag: Boolean): Unit {{ var a = Item(1); {call} }}"
        );
        assert_events(&text, &[expected(&text, &call, "a", "return", 0)]);
    }
}

#[test]
fn partial_loop_exit_does_not_replay_completed_or_unvisited_prefixes() {
    for jump in ["break", "continue"] {
        let call = format!("edit(&a, if (flag) {{ {jump} }} else {{ 0 }}, &b)");
        let text = format!(
            "fun edit(inout first: Int, next: Int, inout last: Int): Unit {{}}\nfun run(flag: Boolean): Unit {{ loop {{ var a = 0; var b = 1; {call}; break }} }}"
        );
        // There are two break expressions in the break case; identify the first
        // through its unique branch source, independently of observed endpoints.
        let branch = span(&text, &format!("{{ {jump} }}"));
        let mut event = expected(&text, &call, "a", &format!("{{ {jump} }}"), 0);
        event.2 = (branch.0 + 2, branch.1 - 2);
        let observed = assert_events(&text, &[event]);
        for unit in &observed.units {
            assert!(
                unit.loans().iter().any(|loan| {
                    let node = observed
                        .parsed
                        .ast()
                        .expressions()
                        .get(loan.argument().expression())
                        .unwrap();
                    loan.kind() == LoanKind::Exclusive
                        && observed.sources.slice(node.span()) == Ok("b")
                }),
                "continuing branch really establishes b, but its normal return is not an unwind"
            );
        }
    }
}

#[test]
fn projected_and_borrowed_sources_do_not_gain_loop_exit_observations() {
    for jump in ["break", "continue"] {
        for (index, text) in [
            format!(
                "class Item(var n: Int)\nfun run(): Unit {{ loop {{ val a = Item(1); replace(&a.n, {jump}) }} }}"
            ),
            format!(
                "fun edit(inout value: Int, next: Unit): Unit {{}}\nfun run(inout a: Int): Unit {{ loop {{ edit(&a, {jump}) }} }}"
            ),
            format!(
                "fun edit(next: Unit, inout value: Int): Unit {{}}\nfun run(): Unit {{ loop {{ var a = 0; edit({jump}, &a) }} }}"
            ),
        ].into_iter().enumerate() {
            let observed = assert_events(&text, &[]);
            for unit in &observed.units {
                let actual = unit.loans().iter().filter(|loan| loan.kind() == LoanKind::Exclusive).count();
                assert_eq!(actual, usize::from(index != 2), "excluded roots still establish a loan; the unvisited argument does not");
            }
        }
    }
}

#[test]
fn iteration_exit_keeps_the_real_token_before_owner_and_element_cleanup() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
    for jump in ["break", "continue"] {
        let call = format!("replace(&a, {jump})");
        let text = format!(
            "class Item(val n: Int)\nfun run(): Unit {{ for (_ in listOf(1)) {{ var a = Item(1); {call} }} }}"
        );
        let observed = assert_events(&text, &[expected(&text, &call, "a", jump, 0)]);
        for unit in &observed.units {
            let [event] = unit.exclusive_call_prefix_ends() else {
                panic!("one established Exclusive token");
            };
            let actions = unit
                .iteration_cleanup_at(event.point())
                .expect("real iteration exit");
            let ends: Vec<_> = actions
                .iter()
                .enumerate()
                .filter_map(|(index, action)| {
                    matches!(action, Action::EndCallLoan(loan) if loan == event.loan())
                        .then_some(index)
                })
                .collect();
            let [end] = ends.as_slice() else {
                panic!("the same token ends exactly once");
            };
            let UnitLoanTarget::Place(place) = event.loan().target() else {
                panic!("root owner");
            };
            let drop = actions.iter().position(|action| matches!(action, Action::Drop(drop) if drop.target() == UnitDropTarget::Named(place.root()))).unwrap();
            let element = actions
                .iter()
                .position(|action| matches!(action, Action::EndElement(_)))
                .unwrap();
            assert!(
                *end < drop && drop < element,
                "loan end, owner drop, then provider element cleanup"
            );
        }
    }
}
