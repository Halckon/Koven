//! Both checked replacement outputs move once through separate, ordered binding lifetimes.
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
fn actual_replace_outputs_transfer_once_and_retire_in_statement_order() {
    for condition in ["flag", "!(flag)"] {
        for arms in [
            r#""left" else "right""#,
            r#"{ val local = "left"; ((local)) } else "right""#,
            r#""left" else { val local = "right"; (local) }"#,
            r#"{ val local = "left"; local } else { val local = "right"; local }"#,
        ] {
            for grouped in [false, true] {
                for incoming_grouped in [false, true] {
                    for result_depth in [0, 2] {
                        for depth in [0, 2] {
                            for old_depth in [0, 2] {
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
                                let mut returned = vec![call_text.clone()];
                                for _ in 0..result_depth {
                                    returned.push(format!("({})", returned.last().unwrap()));
                                }
                                let result_text = returned.last().unwrap();
                                let mut transfer = vec!["chosen".to_owned()];
                                for _ in 0..depth {
                                    transfer.push(format!("({})", transfer.last().unwrap()));
                                }
                                let mut old_transfer = vec!["previous".to_owned()];
                                for _ in 0..old_depth {
                                    old_transfer
                                        .push(format!("({})", old_transfer.last().unwrap()));
                                }
                                let old_initializer = old_transfer.last().unwrap();
                                let source = format!(
                                    "fun run(flag: Boolean): Unit {{ var chosen = {initial}; val previous = {result_text}; val old_forwarded = {old_initializer}; val forwarded = {} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}",
                                    transfer.last().unwrap()
                                );
                                let fixture = Fixture::new(&[("root.ko", &source)]);
                                let id = |text: &str| {
                                    UnitExpressionId::new(
                                        fixture.source(0),
                                        if text == "chosen" {
                                            fixture.parsed[0]
                                                .ast()
                                                .expressions()
                                                .iter()
                                                .find(|(_, node)| {
                                                    node.span().start()
                                                        == source.rfind(text).unwrap()
                                                        && fixture
                                                            .sources
                                                            .slice(node.span())
                                                            .unwrap()
                                                            == text
                                                })
                                                .map(|(id, _)| id)
                                                .unwrap()
                                        } else {
                                            fixture.expression(0, text)
                                        },
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
                                assert!(
                                    legacy.definitions().is_empty() && legacy.events().is_empty()
                                );
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
                                        definition.origin()
                                            == UnitOwnerOrigin::String(id("\"next\""))
                                    })
                                    .collect();
                                assert_eq!(definitions.len(), 1);
                                let installed = definitions[0].id();
                                let root = fixture.symbol(0, "chosen");
                                let previous = fixture.symbol(0, "previous");
                                let forwarded = fixture.symbol(0, "forwarded");
                                let old_forwarded = fixture.symbol(0, "old_forwarded");
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
                                for pair in returned.windows(2) {
                                    assert!(
                                        matches!(events.next(), Some(Event::Forward(from, to, Value::Choice(value))) if *from == id(&pair[0]) && *to == id(&pair[1]) && value == choice)
                                    );
                                }
                                assert!(
                                    matches!(events.next(), Some(Event::Bind(symbol, expression, Value::Choice(value))) if *symbol == previous && *expression == id(result_text) && value == choice)
                                );
                                assert!(
                                    matches!(events.next(), Some(Event::Move(expression, symbol, Value::Choice(value))) if *expression == id("previous") && *symbol == previous && value == choice)
                                );
                                for pair in old_transfer.windows(2) {
                                    assert!(
                                        matches!(events.next(), Some(Event::Forward(from, to, Value::Choice(value))) if *from == id(&pair[0]) && *to == id(&pair[1]) && value == choice)
                                    );
                                }
                                assert!(
                                    matches!(events.next(), Some(Event::Bind(symbol, expression, Value::Choice(value))) if *symbol == old_forwarded && *expression == id(old_initializer) && value == choice)
                                );
                                let old_point = declaration(&fixture, id(old_initializer));
                                assert!(
                                    matches!(events.next(), Some(Event::Drop(point, symbol, Value::Choice(value))) if *point == old_point && *symbol == old_forwarded && value == choice)
                                );
                                assert!(
                                    matches!(events.next(), Some(Event::Move(expression, symbol, Value::Definition(value))) if *expression == id("chosen") && *symbol == root && *value == installed)
                                );
                                for pair in transfer.windows(2) {
                                    assert!(
                                        matches!(events.next(), Some(Event::Forward(from, to, Value::Definition(value))) if *from == id(&pair[0]) && *to == id(&pair[1]) && *value == installed)
                                    );
                                }
                                let final_initializer = id(transfer.last().unwrap());
                                assert!(
                                    matches!(events.next(), Some(Event::Bind(symbol, expression, Value::Definition(value))) if *symbol == forwarded && *expression == final_initializer && *value == installed)
                                );
                                let point = declaration(&fixture, final_initializer);
                                assert!(
                                    matches!(events.next(), Some(Event::Drop(actual_point, symbol, Value::Definition(value))) if *actual_point == point && *symbol == forwarded && *value == installed)
                                );
                                assert!(events.next().is_none());
                                let cleanup: Vec<_> = owned
                                    .drops()
                                    .iter()
                                    .filter(|drop| {
                                        [
                                            UnitDropTarget::Named(root),
                                            UnitDropTarget::Named(previous),
                                            UnitDropTarget::Named(old_forwarded),
                                            UnitDropTarget::Named(forwarded),
                                        ]
                                        .contains(&drop.target())
                                    })
                                    .map(|drop| (drop.point(), drop.target()))
                                    .collect();
                                assert_eq!(
                                    cleanup,
                                    vec![
                                        (old_point, UnitDropTarget::Named(old_forwarded)),
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
}

#[test]
fn dual_output_transfer_rejects_reordering_chains_and_extra_consumers() {
    let prefix =
        r#"var chosen = if (flag) "left" else "right"; val previous = replace(&chosen, "next");"#;
    for suffix in [
        "val installed = chosen; val old = previous",
        "val old = previous; val again = old",
        "val old = previous; val installed = chosen + \"suffix\"",
        "var old = previous; val installed = chosen",
        "val old = previous; var installed = chosen",
        "val old = previous; val installed: String? = chosen",
        "val old = previous; val installed = make(chosen)",
        "val old = previous; val installed = chosen; val extra = installed",
    ] {
        let source = format!(
            "fun run(flag: Boolean): Unit {{ {prefix} {suffix} }}\nfun make(own value: String): String = value\nfun sibling(): Unit {{ val safe = \"safe\" }}"
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
    let source = format!(
        "fun run(flag: Boolean): Unit {{ while (flag) {{ {prefix} val old = previous; val installed = chosen; break }} }}"
    );
    let fixture = Fixture::new(&[("root.ko", &source)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.clone().validate().is_ok());
    assert!(
        owned.owner_observations()[0]
            .value_observation_v1()
            .is_none()
    );
    compare(&fixture, &fixture.parts(&owned));
}

#[test]
fn dual_transfer_requires_first_retirement_before_reset_and_second_take() {
    let control = r#"if (flag) "left" else "right""#;
    let call = r#"replace(&chosen, "next")"#;
    let source = format!(
        "fun run(flag: Boolean): Unit {{ var chosen = {control}; val previous = {call}; val old = (previous); val installed = (chosen) }}"
    );
    let fixture = Fixture::new(&[("root.ko", &source)]);
    let owned = fixture.shared();
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let id = |text| checker.unit_expression(fixture.expression(0, text));
        let control = id(control);
        let call = id(call);
        let old_leaf = id("previous");
        let old_group = id("(previous)");
        let new_group = id("(chosen)");
        let crate::parser::Expression::Group { expression } = fixture.parsed[0]
            .ast()
            .expressions()
            .get(new_group.expression())
            .unwrap()
            .payload()
        else {
            panic!("real new Group")
        };
        let new_leaf = checker.unit_expression(*expression);
        let left = id("\"left\"");
        let right = id("\"right\"");
        let next = id("\"next\"");
        let root = fixture.symbol(0, "chosen");
        let previous = fixture.symbol(0, "previous");
        let old = fixture.symbol(0, "old");
        let installed = fixture.symbol(0, "installed");
        let old_point = declaration(&fixture, old_group);
        let new_point = declaration(&fixture, new_group);
        // Supplemental hand traces; the ordinary source tests prove actual behavior.
        for mutation in 0..22 {
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
                tx.dropped(UnitDropPoint::CallReturn(call), root);
            }
            tx.expression_completed(call, true, true);
            tx.bound(previous, call);
            if mutation == 21 {
                tx.expression_completed(new_group, true, true);
            }
            if mutation == 2 {
                tx.dropped(declaration(&fixture, call), previous);
            }
            if mutation == 3 {
                tx.moved(new_leaf, Some(root));
            }
            tx.moved(
                old_leaf,
                if mutation == 4 {
                    Some(root)
                } else {
                    Some(previous)
                },
            );
            tx.expression_completed(old_leaf, mutation != 5, true);
            if mutation != 6 {
                tx.expression_completed(old_group, true, true);
            }
            if mutation == 7 {
                tx.dropped(old_point, old);
            }
            tx.bound(old, old_group);
            if mutation == 8 {
                tx.moved(new_leaf, Some(root));
            }
            if mutation != 9 {
                tx.dropped(if mutation == 10 { new_point } else { old_point }, old);
            }
            if mutation == 11 {
                tx.dropped(old_point, old);
            }
            if mutation == 12 {
                tx.expression_completed(old_group, true, true);
            }
            tx.moved(
                new_leaf,
                if mutation == 13 {
                    Some(previous)
                } else {
                    Some(root)
                },
            );
            if mutation != 14 {
                tx.expression_completed(new_leaf, true, true);
            }
            tx.expression_completed(new_group, mutation != 15, true);
            if mutation == 16 {
                tx.dropped(new_point, installed);
            }
            tx.bound(
                installed,
                if mutation == 17 { old_group } else { new_group },
            );
            tx.dropped(
                if mutation == 18 { old_point } else { new_point },
                installed,
            );
            if mutation == 19 {
                tx.dropped(new_point, installed);
            }
            if mutation == 20 {
                tx.moved(old_leaf, Some(previous));
            }
            assert_eq!(tx.finish().is_some(), mutation == 0, "mutation {mutation}");
        }
    });
}
