//! Exclusive prefix observations keep the existing Single witnesses intact.
use super::*;

fn assert_prefixes(text: &str, calls_and_roots: &[(&str, &str)]) -> Observed {
    let observed = observe(text);
    assert_units_admitted(&observed);
    for (entry, (unit, names)) in observed.units.iter().zip(&observed.unit_names).enumerate() {
        let [source] = names.names().index().source_units() else {
            panic!("one actual source");
        };
        assert_eq!(source.source_id(), observed.parsed.source_id());
        assert!(matches!(
            (entry, source.key()),
            (0, SourceUnitKey::Rooted { .. }) | (1, SourceUnitKey::MemoryDocument)
        ));
        let [local_names] = names.names().source_units() else {
            panic!("one local resolution");
        };
        let events = unit.exclusive_call_prefix_ends();
        assert_eq!(events.len(), calls_and_roots.len(), "{text}");
        for (event, &(expected_call, expected_root)) in events.iter().zip(calls_and_roots) {
            let loan = event.loan();
            assert!(unit.loans().contains(loan));
            assert_eq!(loan.kind(), LoanKind::Exclusive);
            assert_eq!(loan.call().source_unit(), source.id());
            assert_eq!(loan.argument().source_unit(), source.id());
            let call = observed
                .parsed
                .ast()
                .expressions()
                .get(loan.call().expression())
                .unwrap();
            let argument = observed
                .parsed
                .ast()
                .expressions()
                .get(loan.argument().expression())
                .unwrap();
            assert_eq!(observed.sources.slice(call.span()), Ok(expected_call));
            assert_eq!(observed.sources.slice(argument.span()), Ok(expected_root));
            assert_eq!(loan.end_span(), call.span());
            let Expression::Call { arguments, .. } = call.payload() else {
                panic!("actual call");
            };
            let call_argument = arguments
                .iter()
                .find(|arg| arg.value == loan.argument().expression())
                .expect("original call argument identity");
            let Some(lang_frontend::parser::ParameterModeMarker::Inout(marker)) =
                call_argument.mode_marker
            else {
                panic!("exclusive root has an explicit inout marker");
            };
            // Inout begins at its source `&`, not at the following value expression.
            assert_eq!(loan.begin_span(), marker);
            assert_eq!(observed.sources.slice(marker), Ok("&"));
            assert_ne!(marker, argument.span());
            let UnitLoanTarget::Place(place) = loan.target() else {
                panic!("owned root place");
            };
            assert!(place.is_root());
            assert_eq!(place.root().source_unit(), source.id());
            assert!(unit.formed_owned_locals().contains(&place.root()));
            let symbol = &local_names.resolution().symbols()[place.root().symbol().index()];
            assert_eq!(symbol.name(), expected_root);
            assert_eq!(observed.sources.slice(symbol.span()), Ok(expected_root));
            let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
                panic!("actual transfer, not call source end");
            };
            assert_eq!(transfer.source_unit(), source.id());
            let transfer = observed
                .parsed
                .ast()
                .expressions()
                .get(transfer.expression())
                .unwrap();
            assert!(matches!(transfer.payload(), Expression::Return { .. }));
            assert_eq!(observed.sources.slice(transfer.span()), Ok("return"));
            assert_ne!(transfer.span(), loan.end_span());
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.point() == event.point())
                .collect();
            let position = drops
                .iter()
                .position(|drop| drop.target() == UnitDropTarget::Named(place.root()))
                .unwrap();
            assert_eq!(event.preceding_drops(), 0);
            assert!(
                event.preceding_drops() <= position,
                "loan ends before original owner drop"
            );
        }
    }
    observed
}

#[test]
fn replace_return_exposes_actual_exclusive_prefix_end_in_both_entries() {
    let observed = assert_prefixes(
        "class Item(val n: Int)\nfun run(): Unit {\nvar a = Item(1)\nreplace(&a, return)\n}",
        &[("replace(&a, return)", "a")],
    );
    for (unit, typed) in observed.units.iter().zip(&observed.unit_types) {
        assert!(unit.is_compatible_with(typed));
        let [event] = unit.exclusive_call_prefix_ends() else {
            panic!("exactly one actual replace-prefix event");
        };
        let call = typed
            .types()
            .call(event.loan().call())
            .expect("same typed call");
        let UnitCallTarget::External(external) = call.target() else {
            panic!("builtin replace must use the External target branch");
        };
        assert_eq!(
            observed.environment.symbol(external).unwrap().name(),
            "replace"
        );
        assert!(
            unit.ownership_primitives().is_empty(),
            "no committed replacement"
        );
        assert!(
            unit.direct_call_loan_ends().is_empty(),
            "Shared slice unchanged"
        );
        assert_eq!(unit.drops().len(), 1);
    }
}

#[test]
fn nested_prefixes_end_in_reverse_evaluation_order_before_owner_drops() {
    let observed = assert_prefixes(
        "class Item(val n: Int)\nfun run(): Unit {\nvar a = Item(1)\nvar b = Item(2)\nreplace(&a, replace(&b, return))\n}",
        &[
            ("replace(&b, return)", "b"),
            ("replace(&a, replace(&b, return))", "a"),
        ],
    );
    for unit in observed.units {
        let events = unit.exclusive_call_prefix_ends();
        assert_eq!(events[0].point(), events[1].point());
        assert_eq!(unit.drops().len(), 2);
        assert!(unit.ownership_primitives().is_empty());
    }
}

#[test]
fn abort_and_normal_return_do_not_invent_prefix_unwind_events() {
    for text in [
        "class Item(val n: Int)\nfun run(): Unit {\nvar a = Item(1)\nreplace(&a, error(\"stop\"))\n}",
        "class Item(val n: Int)\nfun run(): Unit {\nvar a = Item(1)\nval old = replace(&a, Item(2))\n}",
        "class Item(val n: Int)\nfun run(): Unit {\nvar a = Item(1)\nval old = replace(&a, Item(2))\nreturn\n}",
    ] {
        let observed = assert_prefixes(text, &[]);
        for unit in observed.units {
            assert!(
                unit.loans()
                    .iter()
                    .any(|loan| loan.kind() == LoanKind::Exclusive)
            );
            if text.contains("error") {
                assert!(unit.ownership_primitives().is_empty());
                assert!(
                    unit.drops()
                        .iter()
                        .all(|drop| !matches!(drop.target(), UnitDropTarget::Named(_)))
                );
            }
        }
    }
}

#[test]
fn unvisited_operand_borrowed_root_and_projection_are_not_observed() {
    for text in [
        "class Item(val n: Int)\nfun edit(first: Unit, inout item: Item): Unit {}\nfun run(): Unit {\nvar a = Item(1)\nedit(return, &a)\n}",
        "fun edit(inout item: Int, next: Unit): Unit {}\nfun run(inout a: Int): Unit { edit(&a, return) }",
        "class Item(var n: Int)\nfun run(): Unit {\nval a = Item(1)\nreplace(&a.n, return)\n}",
        "fun read(item: String, next: Unit): Unit {}\nfun run(): Unit { val a = \"value\"; read(a, return) }",
    ] {
        assert_prefixes(text, &[]);
    }
}

#[test]
fn declaration_return_exposes_exclusive_owned_root_prefix() {
    assert_prefixes(
        "class Item(val n: Int)\nfun edit(inout item: Item, next: Unit): Unit {}\nfun run(): Unit {\nvar a = Item(1)\nedit(&a, return)\n}",
        &[("edit(&a, return)", "a")],
    );
}

#[test]
fn established_exclusive_prefixes_end_at_the_actual_loop_exit() {
    for jump in ["break", "continue"] {
        let call_text = format!("replace(&a, {jump})");
        let text = format!(
            "class Item(val n: Int)\nfun run(): Unit {{ loop {{ var a = Item(1); {call_text} }} }}"
        );
        let observed = observe(&text);
        assert_units_admitted(&observed);
        for ((unit, typed), names) in observed
            .units
            .iter()
            .zip(&observed.unit_types)
            .zip(&observed.unit_names)
        {
            assert!(unit.is_compatible_with(typed));
            let [source] = names.names().index().source_units() else {
                panic!("one original source");
            };
            let loans: Vec<_> = unit
                .loans()
                .iter()
                .filter(|loan| loan.kind() == LoanKind::Exclusive)
                .collect();
            let [loan] = loans.as_slice() else {
                panic!("the original call really established one Exclusive loan");
            };
            assert_eq!(loan.call().source_unit(), source.id());
            assert_eq!(loan.argument().source_unit(), source.id());
            let call = observed
                .parsed
                .ast()
                .expressions()
                .get(loan.call().expression())
                .unwrap();
            assert_eq!(observed.sources.slice(call.span()), Ok(call_text.as_str()));
            assert!(matches!(
                typed.types().call(loan.call()).unwrap().target(),
                UnitCallTarget::External(_)
            ));
            let UnitLoanTarget::Place(place) = loan.target() else {
                panic!("owned root");
            };
            assert_eq!(place.root().source_unit(), source.id());
            assert!(place.is_root());
            assert!(unit.formed_owned_locals().contains(&place.root()));
            let (transfer, _) = observed
                .parsed
                .ast()
                .expressions()
                .iter()
                .find(|(_, node)| {
                    matches!(
                        (jump, node.payload()),
                        ("break", Expression::Break { .. })
                            | ("continue", Expression::Continue { .. })
                    )
                })
                .expect("original loop control transfer");
            let point = UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                loan.call().source_unit(),
                transfer,
            ));
            assert!(
                unit.drops().iter().any(|drop| {
                    drop.point() == point && drop.target() == UnitDropTarget::Named(place.root())
                }),
                "the established owner is actually cleaned at the loop exit"
            );
            assert!(
                unit.ownership_primitives().is_empty(),
                "replacement never commits"
            );
            let [event] = unit.exclusive_call_prefix_ends() else {
                panic!("the original loop source now publishes its actual end");
            };
            assert_eq!(event.loan(), *loan);
            assert_eq!(event.point(), point);
            assert_eq!(event.preceding_call_loan_ends(), 0);
            assert_eq!(event.preceding_drops(), 0);
        }
    }
}
