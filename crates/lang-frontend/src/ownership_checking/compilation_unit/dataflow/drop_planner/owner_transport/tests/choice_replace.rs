//! Actual checked root replacement of a selected owner, with separate cleanup.
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
fn actual_choice_root_replace_returns_selected_owner_and_drops_installed_value() {
    for condition in ["flag", "!(flag)"] {
        for arms in [
            r#""left" else "right""#,
            r#"{ val local = "left"; ((local)) } else "right""#,
            r#""left" else { val local = "right"; (local) }"#,
            r#"{ val local = "left"; local } else { val local = "right"; local }"#,
        ] {
            for grouped in [false, true] {
                for depth in [0, 2] {
                    let control = format!("if ({condition}) {arms}");
                    let initial = if grouped {
                        format!("({control})")
                    } else {
                        control.clone()
                    };
                    let mut incoming = vec!["\"next\"".to_owned()];
                    for _ in 0..depth {
                        incoming.push(format!("({})", incoming.last().unwrap()));
                    }
                    let call_text = format!("replace(&chosen, {})", incoming.last().unwrap());
                    let source = format!(
                        "fun run(flag: Boolean): Unit {{ var chosen = {initial}; val previous = {call_text} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}"
                    );
                    let fixture = Fixture::new(&[("root.ko", &source)]);
                    let id = |text: &str| {
                        UnitExpressionId::new(fixture.source(0), fixture.expression(0, text))
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
                    let graph = legacy
                        .value_observation_v1()
                        .unwrap_or_else(|| panic!("checked Choice replace: {source}"));
                    assert_eq!(graph.producers().definitions().len(), 3);
                    choice_branch_local::assert_ast_branch_events(&fixture, &control, &graph);
                    let choice = graph.choice().unwrap().id();
                    let definitions: Vec<_> = graph
                        .producers()
                        .definitions()
                        .iter()
                        .filter(|definition| {
                            definition.origin() == UnitOwnerOrigin::String(id(&incoming[0]))
                        })
                        .collect();
                    assert_eq!(definitions.len(), 1);
                    let installed = definitions[0].id();
                    let root = fixture.symbol(0, "chosen");
                    let previous = fixture.symbol(0, "previous");
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
                        matches!(events.next(), Some(Event::Create(expression, value)) if *expression == id(&incoming[0]) && *value == installed)
                    );
                    for pair in incoming.windows(2) {
                        assert!(
                            matches!(events.next(), Some(Event::Forward(from, to, Value::Definition(value))) if *from == id(&pair[0]) && *to == id(&pair[1]) && *value == installed)
                        );
                    }
                    let argument = id(incoming.last().unwrap());
                    assert!(
                        matches!(events.next(), Some(Event::Pending(actual_call, actual_argument, Value::Definition(value))) if *actual_call == call && *actual_argument == argument && *value == installed)
                    );
                    assert!(
                        matches!(events.next(), Some(Event::Replace { call: actual_call, argument: actual_argument, root: actual_root, previous: Value::Choice(old), incoming: Value::Definition(new) }) if *actual_call == call && *actual_argument == argument && *actual_root == root && old == choice && *new == installed)
                    );
                    assert!(
                        matches!(events.next(), Some(Event::Drop(UnitDropPoint::CallReturn(actual_call), symbol, Value::Definition(value))) if *actual_call == call && *symbol == root && *value == installed)
                    );
                    assert!(
                        matches!(events.next(), Some(Event::Bind(symbol, expression, Value::Choice(value))) if *symbol == previous && *expression == call && value == choice)
                    );
                    let point = declaration(&fixture, call);
                    assert!(
                        matches!(events.next(), Some(Event::Drop(actual_point, symbol, Value::Choice(value))) if *actual_point == point && *symbol == previous && value == choice)
                    );
                    assert!(events.next().is_none());
                    let cleanup: Vec<_> = owned
                        .drops()
                        .iter()
                        .filter(|drop| {
                            [UnitDropTarget::Named(root), UnitDropTarget::Named(previous)]
                                .contains(&drop.target())
                        })
                        .map(|drop| (drop.point(), drop.target()))
                        .collect();
                    assert_eq!(
                        cleanup,
                        vec![
                            (UnitDropPoint::CallReturn(call), UnitDropTarget::Named(root)),
                            (point, UnitDropTarget::Named(previous))
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

#[test]
fn choice_root_replace_keeps_unproven_consumers_closed() {
    for body in [
        r#"var chosen = if (flag) "left" else "right""#,
        r#"var chosen = if (flag) "left" else "right"; val previous = chosen"#,
        r#"var chosen = if (flag) "left" else "right"; val previous = (replace(&chosen, "next"))"#,
        r#"var chosen = if (flag) "left" else "right"; val previous = replace(&chosen, make())"#,
        r#"var chosen = if (flag) "left" else "right"; val previous = replace(&chosen, "${flag}")"#,
        r#"var chosen = if (flag) "left" else "right"; val previous = replace(&chosen, if (flag) "next" else "other")"#,
        r#"var chosen = if (flag) "left" else "right"; val previous = replace(&chosen, "next"); val again = replace(&chosen, "last")"#,
        r#"val incoming = "next"; var chosen = if (flag) "left" else "right"; val previous = replace(&chosen, incoming)"#,
        r#"var chosen: String? = if (flag) "left" else "right"; val previous = replace(&chosen, "next")"#,
        r#"var chosen = if (flag) "left" else "right"; var previous = replace(&chosen, "next")"#,
        r#"while (flag) { var chosen = if (flag) "left" else "right"; val previous = replace(&chosen, "next"); break }"#,
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
fn choice_replace_does_not_erase_immutable_root_diagnostic() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(flag: Boolean): Unit { val chosen = if (flag) "left" else "right"; val previous = replace(&chosen, "next") }"#,
    )]);
    let owned = fixture.shared();
    let code = codes::catalog()
        .unwrap()
        .resolve(codes::IMMUTABLE_INOUT_PLACE)
        .unwrap();
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == code)
    );
    assert!(
        owned
            .owner_observations()
            .iter()
            .all(|observation| observation.value_observation_v1().is_none())
    );
}

#[test]
fn replacement_frames_require_actual_pending_commit_and_ordered_cleanup() {
    let control = r#"if (flag) "left" else "right""#;
    let call = r#"replace(&chosen, ("next"))"#;
    let source =
        format!("fun run(flag: Boolean): Unit {{ var chosen = {control}; val previous = {call} }}");
    let fixture = Fixture::new(&[("root.ko", &source)]);
    let owned = fixture.shared();
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let id = |text| checker.unit_expression(fixture.expression(0, text));
        let control = id(control);
        let call = id(call);
        let left = id("\"left\"");
        let right = id("\"right\"");
        let next = id("\"next\"");
        let argument = id("(\"next\")");
        let root = fixture.symbol(0, "chosen");
        let previous = fixture.symbol(0, "previous");
        // Supplemental hand traces; ordinary source tests above are the behavior evidence.
        for mutation in 0..20 {
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
            tx.expression_completed(next, mutation != 1, true);
            if mutation != 2 {
                tx.expression_completed(argument, true, true);
            }
            if mutation != 3 {
                tx.replacement_pending(call, argument, mutation != 4);
            }
            if mutation == 5 {
                tx.replacement_pending(call, argument, true);
            }
            if mutation != 6 {
                tx.replacement_committed(
                    call,
                    (mutation != 7).then_some(root),
                    Some(argument),
                    mutation != 8,
                );
            }
            if mutation == 9 {
                tx.replacement_committed(call, Some(root), Some(argument), true);
            }
            if mutation == 10 {
                tx.expression_completed(call, true, true);
            }
            tx.dropped(
                if mutation == 11 {
                    UnitDropPoint::AfterExpression(call)
                } else {
                    UnitDropPoint::CallReturn(call)
                },
                if mutation == 12 { previous } else { root },
            );
            if mutation == 13 {
                tx.dropped(UnitDropPoint::CallReturn(call), root);
            }
            if mutation != 14 {
                tx.expression_completed(call, mutation != 15, mutation != 16);
            }
            tx.bound(previous, if mutation == 17 { argument } else { call });
            tx.dropped(declaration(&fixture, call), previous);
            if mutation == 18 {
                tx.dropped(declaration(&fixture, call), previous);
            }
            if mutation == 19 {
                tx.string_completed(next);
            }
            assert_eq!(tx.finish().is_some(), mutation == 0, "mutation {mutation}");
        }
    });
}
