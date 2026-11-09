//! Whole-If Group results must be transported before the actual chosen binding.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerCallable, UnitOwnerObservationStatus,
    UnitOwnerUnsupportedReason, UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::parser::{Item, Statement};
use crate::type_checking::{UnitItemId, UnitStatementId};

fn declaration_point(fixture: &Fixture, initializer: UnitExpressionId) -> UnitDropPoint {
    let ast = fixture.parsed[0].ast();
    let matches: Vec<_> = ast
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
    let [point] = matches.as_slice() else {
        panic!("one real declaration for this initializer");
    };
    *point
}

#[test]
fn actual_if_result_groups_forward_choice_before_binding_and_optional_transfer() {
    for condition in ["flag", "!flag"] {
        for arms in [
            r#""left" else "right""#,
            r#"{ val local = "left"; ((local)) } else "right""#,
            r#""left" else { val local = "right"; (local) }"#,
            r#"{ val local = "left"; local } else { val local = "right"; local }"#,
        ] {
            for depth in [1, 3] {
                for transfer in [false, true] {
                    let control = format!("if ({condition}) {arms}");
                    let mut texts = vec![control.clone()];
                    for _ in 0..depth {
                        texts.push(format!("({})", texts.last().unwrap()));
                    }
                    let suffix = if transfer {
                        "; val forwarded = (chosen)"
                    } else {
                        ""
                    };
                    let source = format!(
                        "fun run(flag: Boolean): Unit {{ val chosen = {}{suffix} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}",
                        texts.last().unwrap()
                    );
                    let fixture = Fixture::new(&[("root.ko", &source)]);
                    let ids: Vec<_> = texts
                        .iter()
                        .map(|text| {
                            UnitExpressionId::new(fixture.source(0), fixture.expression(0, text))
                        })
                        .collect();
                    let owned = fixture.shared();
                    assert!(owned.diagnostics().is_empty(), "{source}");
                    assert!(owned.clone().validate().is_ok(), "{source}");
                    let legacy = &owned.owner_observations()[0];
                    assert_eq!(
                        legacy.status(),
                        UnitOwnerObservationStatus::Unsupported {
                            point: UnitDropPoint::AfterExpression(ids[0]),
                            reason: UnitOwnerUnsupportedReason::Syntax,
                        }
                    );
                    assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
                    let graph = legacy
                        .value_observation_v1()
                        .unwrap_or_else(|| panic!("whole-If Group Choice: {source}"));
                    choice_branch_local::assert_ast_edges(&fixture, &control, &graph);
                    let choice = graph.choice().unwrap();
                    let events = graph.suffix();
                    assert_eq!(events.len(), depth + if transfer { 5 } else { 2 });
                    for (index, pair) in ids.windows(2).enumerate() {
                        assert!(
                            matches!(&events[index], Event::Forward(from, to, Value::Choice(value))
                            if *from == pair[0] && *to == pair[1] && value == choice.id())
                        );
                    }
                    let chosen = fixture.symbol(0, "chosen");
                    let initializer = *ids.last().unwrap();
                    assert!(
                        matches!(&events[depth], Event::Bind(symbol, expression, Value::Choice(value))
                        if *symbol == chosen && *expression == initializer && value == choice.id())
                    );
                    let (final_symbol, final_initializer) = if transfer {
                        let name = UnitExpressionId::new(
                            fixture.source(0),
                            fixture.expression(0, "chosen"),
                        );
                        let group = UnitExpressionId::new(
                            fixture.source(0),
                            fixture.expression(0, "(chosen)"),
                        );
                        let forwarded = fixture.symbol(0, "forwarded");
                        assert!(
                            matches!(&events[depth+1], Event::Move(expression, symbol, Value::Choice(value))
                            if *expression == name && *symbol == chosen && value == choice.id())
                        );
                        assert!(
                            matches!(&events[depth+2], Event::Forward(from, to, Value::Choice(value))
                            if *from == name && *to == group && value == choice.id())
                        );
                        assert!(
                            matches!(&events[depth+3], Event::Bind(symbol, expression, Value::Choice(value))
                            if *symbol == forwarded && *expression == group && value == choice.id())
                        );
                        assert!(
                            !owned
                                .drops()
                                .iter()
                                .any(|drop| drop.target() == UnitDropTarget::Named(chosen))
                        );
                        (forwarded, group)
                    } else {
                        (chosen, initializer)
                    };
                    let point = declaration_point(&fixture, final_initializer);
                    assert!(
                        matches!(events.last().unwrap(), Event::Drop(actual, symbol, Value::Choice(value))
                        if *actual == point && *symbol == final_symbol && value == choice.id())
                    );
                    assert_eq!(owned.drops().len(), 2);
                    let drops: Vec<_> = owned
                        .drops()
                        .iter()
                        .filter(|drop| drop.target() == UnitDropTarget::Named(final_symbol))
                        .collect();
                    assert_eq!(drops.len(), 1);
                    assert_eq!(drops[0].point(), point);
                    assert_eq!(
                        owned.owner_observations()[1].status(),
                        UnitOwnerObservationStatus::Complete
                    );
                    assert!(
                        owned.owner_observations()[1]
                            .value_observation_v1()
                            .unwrap()
                            .choice()
                            .is_none()
                    );
                    compare(&fixture, &fixture.parts(&owned));
                }
            }
        }
    }
}

#[test]
fn if_result_groups_do_not_admit_other_wrappers_exits_or_extra_uses() {
    for body in [
        r#"val chosen: String? = ((if (flag) "left" else "right"))"#,
        r#"var chosen = ((if (flag) "left" else "right"))"#,
        r#"val chosen = ((if (flag) (if (flag) "a" else "b") else "right"))"#,
        r#"val chosen = ((if (flag) { return } else "right"))"#,
        r#"val chosen = ((if (flag) make() else "right"))"#,
        r#"val chosen = ((if (flag) "left" else "right") + "suffix")"#,
        r#"val chosen = ((if (flag) "left" else "right")); val forwarded = chosen; val again = forwarded"#,
        r#"val chosen = ((if (flag) "left" else "right")); val callback = move { chosen }"#,
        r#"while (flag) { val chosen = ((if (flag) "left" else "right")); break }"#,
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
        let sibling = owned
            .owner_observations()
            .iter()
            .find(|observation| observation.callable() == sibling)
            .unwrap();
        assert_eq!(sibling.status(), UnitOwnerObservationStatus::Complete);
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn whole_if_result_frames_reject_missing_read_or_misordered_completion() {
    let control = "if (flag) \"left\" else \"right\"";
    let group = format!("({control})");
    let initializer = format!("({group})");
    let source = format!("fun run(flag: Boolean): Unit {{ val chosen = {initializer} }}");
    let fixture = Fixture::new(&[("root.ko", &source)]);
    let owned = fixture.shared();
    let parts = fixture.parts(&owned);
    compare_and_inspect(&fixture, &parts, |checker| {
        let unit = |text| checker.unit_expression(fixture.expression(0, text));
        let control = unit(control);
        let group = unit(&group);
        let initializer = unit(&initializer);
        let left = unit("\"left\"");
        let right = unit("\"right\"");
        let chosen = fixture.symbol(0, "chosen");
        let point = declaration_point(&fixture, initializer);
        // Synthetic guard traces supplement, never replace, the real AST matrix.
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
            if mutation != 1 {
                tx.expression_completed(control, mutation != 2, mutation != 3);
            }
            if mutation == 4 {
                tx.expression_completed(control, true, true);
            }
            if mutation == 5 {
                tx.expression_completed(initializer, true, true);
            }
            if mutation != 6 {
                tx.expression_completed(group, mutation != 7, true);
            }
            if mutation == 8 {
                tx.expression_completed(group, true, true);
            }
            if mutation == 9 {
                tx.bound(chosen, initializer);
            }
            if mutation != 10 {
                tx.expression_completed(initializer, true, mutation != 11);
            }
            tx.bound(chosen, if mutation == 12 { control } else { initializer });
            if mutation == 13 {
                tx.bound(chosen, initializer);
            }
            tx.dropped(point, chosen);
            if mutation == 14 {
                tx.expression_completed(group, true, true);
            }
            if mutation == 15 {
                tx.dropped(point, chosen);
            }
            assert_eq!(tx.finish().is_some(), mutation == 0, "mutation {mutation}");
        }
    });
}
