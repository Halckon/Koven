//! Whole-replace Groups must forward the selected result through actual Consume frames.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerCallable, UnitOwnerObservationStatus, UnitOwnerOrigin,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::parser::{Item, Statement};
use crate::type_checking::{UnitItemId, UnitStatementId};

fn declaration(fixture: &Fixture, initializer: UnitExpressionId) -> UnitDropPoint {
    let ast = fixture.parsed[0].ast();
    let points: Vec<_> = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            let Statement::LocalVariable { declaration } = node.payload() else {
                return None;
            };
            let Item::Variable {
                initializer: actual,
                ..
            } = ast.items().get(*declaration).unwrap().payload()
            else {
                return None;
            };
            (*actual == initializer.expression()).then_some(UnitDropPoint::AfterStatement(
                UnitStatementId::new(fixture.source(0), id),
            ))
        })
        .collect();
    assert_eq!(points.len(), 1);
    points[0]
}

#[test]
fn actual_replaced_choice_groups_preserve_value_and_call_return_identity() {
    for condition in ["flag", "!(flag)"] {
        for arms in [
            r#""left" else "right""#,
            r#"{ val local = "left"; ((local)) } else "right""#,
            r#""left" else { val local = "right"; (local) }"#,
            r#"{ val local = "left"; local } else { val local = "right"; local }"#,
        ] {
            for grouped in [false, true] {
                for incoming_grouped in [false, true] {
                    for result_depth in [1, 3] {
                        for with_transfer in [false, true] {
                            let depth = 2;
                            let control = format!("if ({condition}) {arms}");
                            let initial = if grouped {
                                format!("({control})")
                            } else {
                                control.clone()
                            };
                            let incoming = if incoming_grouped {
                                "(\"next\")"
                            } else {
                                "\"next\""
                            };
                            let call_text = format!("replace(&chosen, {incoming})");
                            let mut results = vec![call_text.clone()];
                            for _ in 0..result_depth {
                                results.push(format!("({})", results.last().unwrap()));
                            }
                            let returned = results.last().unwrap();
                            let mut transfer = vec!["previous".to_owned()];
                            for _ in 0..depth {
                                transfer.push(format!("({})", transfer.last().unwrap()));
                            }
                            let suffix = if with_transfer {
                                format!("; val forwarded = {}", transfer.last().unwrap())
                            } else {
                                String::new()
                            };
                            let source = format!(
                                "fun run(flag: Boolean): Unit {{ var chosen = {initial}; val previous = {returned}{suffix} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}",
                            );
                            let fixture = Fixture::new(&[("root.ko", &source)]);
                            let id = |text: &str| {
                                UnitExpressionId::new(
                                    fixture.source(0),
                                    fixture.expression(0, text),
                                )
                            };
                            let call = id(&call_text);
                            let owned = fixture.shared();
                            assert!(owned.diagnostics().is_empty(), "{source}");
                            assert!(owned.clone().validate().is_ok(), "{source}");
                            let legacy = &owned.owner_observations()[0];
                            assert!(matches!(
                                legacy.status(),
                                UnitOwnerObservationStatus::Unsupported { .. }
                            ));
                            assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
                            let graph = legacy.value_observation_v1().unwrap_or_else(|| {
                                panic!("selected replacement transfer: {source}")
                            });
                            assert_eq!(graph.producers().definitions().len(), 3);
                            choice_branch_local::assert_ast_branch_events(
                                &fixture, &control, &graph,
                            );
                            let choice = graph.choice().unwrap().id();
                            let definitions: Vec<_> = graph
                                .producers()
                                .definitions()
                                .iter()
                                .filter(|definition| {
                                    definition.origin() == UnitOwnerOrigin::String(id("\"next\""))
                                })
                                .collect();
                            assert_eq!(definitions.len(), 1);
                            let installed = definitions[0].id();
                            let root = fixture.symbol(0, "chosen");
                            let previous = fixture.symbol(0, "previous");
                            let forwarded = if with_transfer {
                                fixture.symbol(0, "forwarded")
                            } else {
                                previous
                            };
                            let mut events = graph.suffix().iter();
                            if grouped {
                                assert!(
                                    matches!(events.next(), Some(Event::Forward(from, to, Value::Choice(value))) if *from == id(&control) && *to == id(&initial) && value == choice)
                                );
                            }
                            assert!(
                                matches!(events.next(), Some(Event::Bind(symbol, expression, Value::Choice(value))) if *symbol == root && *expression == id(&initial) && value == choice)
                            );
                            assert!(
                                matches!(events.next(), Some(Event::Create(expression, value)) if *expression == id("\"next\"") && *value == installed)
                            );
                            if incoming_grouped {
                                assert!(
                                    matches!(events.next(), Some(Event::Forward(from, to, Value::Definition(value))) if *from == id("\"next\"") && *to == id(incoming) && *value == installed)
                                );
                            }
                            assert!(
                                matches!(events.next(), Some(Event::Pending(actual_call, argument, Value::Definition(value))) if *actual_call == call && *argument == id(incoming) && *value == installed)
                            );
                            assert!(
                                matches!(events.next(), Some(Event::Replace { call: actual_call, argument, root: actual_root, previous: Value::Choice(old), incoming: Value::Definition(new) }) if *actual_call == call && *argument == id(incoming) && *actual_root == root && old == choice && *new == installed)
                            );
                            assert!(
                                matches!(events.next(), Some(Event::Drop(UnitDropPoint::CallReturn(actual_call), symbol, Value::Definition(value))) if *actual_call == call && *symbol == root && *value == installed)
                            );
                            for pair in results.windows(2) {
                                assert!(
                                    matches!(events.next(), Some(Event::Forward(from, to, Value::Choice(value))) if *from == id(&pair[0]) && *to == id(&pair[1]) && value == choice)
                                );
                            }
                            assert!(
                                matches!(events.next(), Some(Event::Bind(symbol, expression, Value::Choice(value))) if *symbol == previous && *expression == id(returned) && value == choice)
                            );
                            let final_initializer;
                            if with_transfer {
                                assert!(
                                    matches!(events.next(), Some(Event::Move(expression, symbol, Value::Choice(value))) if *expression == id("previous") && *symbol == previous && value == choice)
                                );
                                for pair in transfer.windows(2) {
                                    assert!(
                                        matches!(events.next(), Some(Event::Forward(from, to, Value::Choice(value))) if *from == id(&pair[0]) && *to == id(&pair[1]) && value == choice)
                                    );
                                }
                                final_initializer = id(transfer.last().unwrap());
                                assert!(
                                    matches!(events.next(), Some(Event::Bind(symbol, expression, Value::Choice(value))) if *symbol == forwarded && *expression == final_initializer && value == choice)
                                );
                            } else {
                                final_initializer = id(returned);
                            }
                            let point = declaration(&fixture, final_initializer);
                            assert!(
                                matches!(events.next(), Some(Event::Drop(actual_point, symbol, Value::Choice(value))) if *actual_point == point && *symbol == forwarded && value == choice)
                            );
                            assert!(events.next().is_none());
                            let cleanup: Vec<_> = owned
                                .drops()
                                .iter()
                                .filter(|drop| {
                                    [
                                        UnitDropTarget::Named(root),
                                        UnitDropTarget::Named(previous),
                                        UnitDropTarget::Named(forwarded),
                                    ]
                                    .contains(&drop.target())
                                })
                                .map(|drop| (drop.point(), drop.target()))
                                .collect();
                            assert_eq!(
                                cleanup,
                                vec![
                                    (UnitDropPoint::CallReturn(call), UnitDropTarget::Named(root)),
                                    (point, UnitDropTarget::Named(forwarded))
                                ]
                            );
                            assert_eq!(owned.drops().len(), 3);
                            assert_eq!(
                                owned.owner_observations()[1].status(),
                                UnitOwnerObservationStatus::Complete
                            );
                            compare(&fixture, &fixture.parts(&owned));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn grouped_replacement_keeps_other_expressions_and_consumers_closed() {
    for body in [
        r#"var chosen = if (flag) "left" else "right"; val previous = (replace(&chosen, "next") + "suffix")"#,
        r#"var chosen = if (flag) "left" else "right"; val previous = (make())"#,
        r#"var chosen = if (flag) "left" else "right"; val previous = (replace(&chosen, make()))"#,
        r#"var chosen = if (flag) "left" else "right"; val previous = (replace(&chosen, if (flag) "next" else "other"))"#,
        r#"var chosen = if (flag) "left" else "right"; val previous = (replace(&chosen, "next")); val forwarded = previous; val again = forwarded"#,
        r#"var chosen = if (flag) "left" else "right"; val previous: String? = (replace(&chosen, "next"))"#,
        r#"while (flag) { var chosen = if (flag) "left" else "right"; val previous = (replace(&chosen, "next")); break }"#,
    ] {
        let source = format!(
            "fun run(flag: Boolean): Unit {{ {body} }}\nfun make(): String = \"made\"\nfun sibling(): Unit {{ val safe = \"safe\" }}"
        );
        let fixture = Fixture::new(&[("root.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{source}");
        assert!(owned.clone().validate().is_ok(), "{source}");
        let rejected = &owned.owner_observations()[0];
        assert!(rejected.value_observation_v1().is_none(), "{source}");
        assert!(rejected.definitions().is_empty() && rejected.events().is_empty());
        let sibling = UnitOwnerCallable::Function(UnitItemId::new(
            fixture.source(0),
            fixture.parsed[0].roots()[2],
        ));
        assert_eq!(
            owned
                .owner_observations()
                .iter()
                .find(|observation| observation.callable() == sibling)
                .unwrap()
                .status(),
            UnitOwnerObservationStatus::Complete
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn replacement_result_frames_reject_missing_repeated_and_wrongly_timed_consumption() {
    let control = r#"if (flag) "left" else "right""#;
    let call = r#"replace(&chosen, "next")"#;
    let inner = format!("({call})");
    let outer = format!("({inner})");
    let source = format!(
        "fun run(flag: Boolean): Unit {{ var chosen = {control}; val previous = {outer} }}"
    );
    let fixture = Fixture::new(&[("root.ko", &source)]);
    let owned = fixture.shared();
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let id = |text| checker.unit_expression(fixture.expression(0, text));
        let control = id(control);
        let call = id(call);
        let inner = id(&inner);
        let outer = id(&outer);
        let left = id("\"left\"");
        let right = id("\"right\"");
        let next = id("\"next\"");
        let root = fixture.symbol(0, "chosen");
        let previous = fixture.symbol(0, "previous");
        // Supplemental hand traces; actual source tests above prove planner behavior.
        for mutation in 0..16 {
            let mut tx = super::super::choice::ChoiceTransaction::begin(
                checker,
                checker.parsed.roots()[0],
                std::sync::Arc::new(()),
            )
            .unwrap();
            tx.string_completed(left);
            tx.expression_completed(left, true, true);
            tx.edge_completed(control, 0, Some(left), true);
            tx.string_completed(right);
            tx.expression_completed(right, true, true);
            tx.edge_completed(control, 1, Some(right), true);
            tx.expression_completed(control, true, true);
            tx.bound(root, control);
            tx.string_completed(next);
            tx.expression_completed(next, true, true);
            tx.replacement_pending(call, next, true);
            tx.replacement_committed(call, Some(root), Some(next), true);
            if mutation == 1 {
                tx.expression_completed(inner, true, true);
            }
            tx.dropped(
                UnitDropPoint::CallReturn(if mutation == 2 { outer } else { call }),
                root,
            );
            if mutation != 3 {
                tx.expression_completed(call, mutation != 4, true);
            }
            if mutation == 5 {
                tx.expression_completed(outer, true, true);
            }
            if mutation != 6 {
                tx.expression_completed(inner, mutation != 7, mutation != 8);
            }
            if mutation == 9 {
                tx.expression_completed(inner, true, true);
            }
            if mutation != 10 {
                tx.expression_completed(outer, mutation != 11, true);
            }
            if mutation == 12 {
                tx.expression_completed(outer, true, true);
            }
            tx.bound(previous, if mutation == 13 { inner } else { outer });
            if mutation == 14 {
                tx.expression_completed(outer, true, true);
            }
            tx.dropped(declaration(&fixture, outer), previous);
            if mutation == 15 {
                tx.expression_completed(outer, true, true);
            }
            assert_eq!(tx.finish().is_some(), mutation == 0, "mutation {mutation}");
        }
    });
}
