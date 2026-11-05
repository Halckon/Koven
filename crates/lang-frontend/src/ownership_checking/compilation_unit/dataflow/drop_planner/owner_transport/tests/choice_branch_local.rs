//! AST-derived transport and cleanup expectations for the bounded local Choice slice.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerId, UnitOwnerObservationStatus, UnitOwnerOrigin,
    UnitOwnerUnsupportedReason, UnitOwnerValueEventV1 as Event, UnitOwnerValueObservationV1,
    UnitOwnerValueV1 as Value,
};
use crate::parser::{Expression, Item, NameMarker, Statement, VariableKind};
use crate::type_checking::UnitStatementId;

#[derive(Debug, PartialEq, Eq)]
enum Expected {
    Create(UnitExpressionId, UnitOwnerId),
    Forward(UnitExpressionId, UnitExpressionId, UnitOwnerId),
    Bind(UnitSymbolId, UnitExpressionId, UnitOwnerId),
    Move(UnitExpressionId, UnitSymbolId, UnitOwnerId),
}

fn symbol_at(fixture: &Fixture, marker: NameMarker) -> UnitSymbolId {
    let NameMarker::Present(span) = marker else {
        panic!("real local marker");
    };
    let symbols: Vec<_> = fixture.names.names().source_units()[fixture.source(0).index()]
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.span() == span)
        .collect();
    assert_eq!(symbols.len(), 1);
    UnitSymbolId::new(fixture.source(0), symbols[0].id())
}

fn chain(fixture: &Fixture, expression: ExpressionId) -> Vec<UnitExpressionId> {
    let mut ids = vec![UnitExpressionId::new(fixture.source(0), expression)];
    let mut current = expression;
    while let Expression::Group { expression } = fixture.parsed[0]
        .ast()
        .expressions()
        .get(current)
        .unwrap()
        .payload()
    {
        current = *expression;
        ids.push(UnitExpressionId::new(fixture.source(0), current));
    }
    ids.reverse();
    ids
}

fn forward(expected: &mut Vec<Expected>, ids: &[UnitExpressionId], owner: &UnitOwnerId) {
    for pair in ids.windows(2) {
        expected.push(Expected::Forward(pair[0], pair[1], owner.clone()));
    }
}

pub(super) fn assert_ast_graph(
    fixture: &Fixture,
    control_text: &str,
    graph: &UnitOwnerValueObservationV1,
) {
    let ast = fixture.parsed[0].ast();
    let control = fixture.expression(0, control_text);
    let unit_control = UnitExpressionId::new(fixture.source(0), control);
    let Expression::If {
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = ast.expressions().get(control).unwrap().payload()
    else {
        panic!("actual If");
    };
    assert!(graph.is_consistent());
    assert!(graph.prefix().is_empty());
    assert_eq!(graph.producers().definitions().len(), 2);
    let choice = graph.choice().unwrap();
    assert_eq!(choice.id().control(), unit_control);
    for (edge, body) in [*then_branch, *else_branch].into_iter().enumerate() {
        let elements = match ast.statements().get(body).unwrap().payload() {
            Statement::ControlBody { elements } => elements.clone(),
            Statement::Expression { .. } => vec![body],
            _ => panic!("admitted actual body"),
        };
        let (tail_statement, prefix) = elements.split_last().unwrap();
        let Statement::Expression { expression } =
            ast.statements().get(*tail_statement).unwrap().payload()
        else {
            panic!("actual expression tail");
        };
        let tail = chain(fixture, *expression);
        let local = match prefix {
            [] => None,
            [binding] => {
                let Statement::LocalVariable { declaration } =
                    ast.statements().get(*binding).unwrap().payload()
                else {
                    panic!("one actual local");
                };
                let Item::Variable {
                    kind: VariableKind::Val,
                    name,
                    initializer,
                    ..
                } = ast.items().get(*declaration).unwrap().payload()
                else {
                    panic!("immutable local");
                };
                Some((symbol_at(fixture, *name), chain(fixture, *initializer)))
            }
            _ => panic!("one local maximum"),
        };
        let producer = local.as_ref().map_or(&tail, |(_, initializer)| initializer);
        assert!(matches!(
            ast.expressions()
                .get(producer[0].expression())
                .unwrap()
                .payload(),
            Expression::String { .. }
        ));
        let definitions: Vec<_> = graph
            .producers()
            .definitions()
            .iter()
            .filter(|definition| definition.origin() == UnitOwnerOrigin::String(producer[0]))
            .collect();
        assert_eq!(definitions.len(), 1, "one real definition per AST literal");
        let owner = definitions[0].id();
        let mut expected = vec![Expected::Create(producer[0], owner.clone())];
        forward(&mut expected, producer, &owner);
        if let Some((symbol, initializer)) = local {
            assert!(matches!(
                ast.expressions()
                    .get(tail[0].expression())
                    .unwrap()
                    .payload(),
                Expression::Name
            ));
            expected.push(Expected::Bind(
                symbol,
                *initializer.last().unwrap(),
                owner.clone(),
            ));
            expected.push(Expected::Move(tail[0], symbol, owner.clone()));
            forward(&mut expected, &tail, &owner);
        }
        let actual: Vec<_> = choice.events()[edge]
            .iter()
            .map(|event| match event {
                Event::Create(id, owner) => Expected::Create(*id, owner.clone()),
                Event::Forward(from, to, Value::Definition(owner)) => {
                    Expected::Forward(*from, *to, owner.clone())
                }
                Event::Bind(symbol, id, Value::Definition(owner)) => {
                    Expected::Bind(*symbol, *id, owner.clone())
                }
                Event::Move(id, symbol, Value::Definition(owner)) => {
                    Expected::Move(*id, *symbol, owner.clone())
                }
                _ => panic!("unexpected branch event: {event:?}"),
            })
            .collect();
        assert_eq!(actual, expected, "edge {edge}");
        assert_eq!(choice.inputs()[edge].edge(), edge);
        assert_eq!(choice.inputs()[edge].tail(), *tail.last().unwrap());
        assert_eq!(choice.inputs()[edge].owner(), owner);
    }
    let chosen: Vec<_> = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            let Statement::LocalVariable { declaration } = node.payload() else {
                return None;
            };
            let Item::Variable {
                name, initializer, ..
            } = ast.items().get(*declaration).unwrap().payload()
            else {
                return None;
            };
            (*initializer == control).then_some((
                UnitStatementId::new(fixture.source(0), id),
                symbol_at(fixture, *name),
            ))
        })
        .collect();
    let [(statement, symbol)] = chosen.as_slice() else {
        panic!("one chosen declaration from AST");
    };
    assert!(
        matches!(graph.suffix(), [Event::Bind(actual, initializer, Value::Choice(bound)), Event::Drop(point, dropped, Value::Choice(released))]
        if actual == symbol && *initializer == unit_control && bound == choice.id()
        && *point == UnitDropPoint::AfterStatement(*statement) && dropped == symbol && released == choice.id())
    );
}

#[test]
fn actual_local_choices_cover_both_mixed_edges_and_distinct_same_spelling_symbols() {
    for control in [
        "if (flag) { val left_local: String = ((\"left\")); (((left_local))) } else { val right_local = (\"right\"); (right_local) }",
        "if (flag) { val local = \"left\"; local } else \"right\"",
        "if (flag) ((\"left\")) else { val local = ((\"right\")); ((local)) }",
        "if (flag) { val local = \"left\"; local } else { val local = \"right\"; local }",
    ] {
        let text = format!(
            "fun run(flag: Boolean): Unit {{ val chosen = {control} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}"
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.clone().validate().is_ok());
        let legacy = &owned.owner_observations()[0];
        assert_eq!(
            legacy.status(),
            UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterExpression(UnitExpressionId::new(
                    fixture.source(0),
                    fixture.expression(0, control)
                )),
                reason: UnitOwnerUnsupportedReason::Syntax,
            }
        );
        assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
        let graph = legacy.value_observation_v1().unwrap();
        assert_ast_graph(&fixture, control, &graph);
        for definition in graph.producers().definitions() {
            assert!(legacy.definition(definition.id()).is_none());
        }
        let Event::Drop(expected_drop, _, _) = graph.suffix()[1] else {
            panic!("checked AST drop")
        };
        let chosen = fixture.symbol(0, "chosen");
        let safe = fixture.symbol(0, "safe");
        assert_eq!(
            owned.drops().len(),
            2,
            "no local or temporary cleanup remains"
        );
        let chosen_drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(chosen))
            .collect();
        assert_eq!(chosen_drops.len(), 1);
        assert_eq!(chosen_drops[0].point(), expected_drop);
        assert_eq!(
            owned
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(safe))
                .count(),
            1
        );
        let sibling = owned.owner_observations()[1]
            .value_observation_v1()
            .unwrap();
        assert!(sibling.choice().is_none());
        assert!(
            sibling
                .producers()
                .definitions()
                .iter()
                .all(|definition| graph.producers().definition(definition.id()).is_none())
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn local_choice_keeps_mutable_nullable_extra_statement_and_nonliteral_boundaries_closed() {
    for (left, right) in [
        ("{ var local = \"left\"; local }", "\"right\""),
        ("{ val local: String? = \"left\"; local }", "\"right\""),
        (
            "{ val extra = \"unused\"; val local = \"left\"; local }",
            "\"right\"",
        ),
        ("{ val local = \"a\" + \"b\"; local }", "\"right\""),
        ("{ val local = \"left\"; \"unrelated\" }", "\"right\""),
        (
            "{ val local = \"left\"; (local); \"unrelated\" }",
            "\"right\"",
        ),
        (
            "{ val local = if (flag) \"a\" else \"b\"; local }",
            "\"right\"",
        ),
        ("{ val local = \"left\"; return; local }", "\"right\""),
        ("\"left\"", "{ var local = \"right\"; local }"),
    ] {
        let text = format!(
            "fun run(flag: Boolean): Unit {{ val chosen = if (flag) {left} else {right} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}"
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{text}");
        assert!(
            owned.owner_observations()[0]
                .value_observation_v1()
                .is_none(),
            "{text}"
        );
        assert!(owned.owner_observations()[0].definitions().is_empty());
        assert!(owned.owner_observations()[0].events().is_empty());
        assert!(
            owned.owner_observations()[1]
                .value_observation_v1()
                .is_some()
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn local_transaction_rejects_missing_or_misordered_real_transition_evidence() {
    let control_text = "if (flag) { val left_local = ((\"left\")); ((left_local)) } else { val right_local = \"right\"; right_local }";
    let text = format!("fun run(flag: Boolean): Unit {{ val chosen = {control_text} }}");
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    let parts = fixture.parts(&owned);
    compare_and_inspect(&fixture, &parts, |checker| {
        let unit = |text| checker.unit_expression(fixture.expression(0, text));
        let control = unit(control_text);
        let left = unit("\"left\"");
        let group = unit("(\"left\")");
        let initializer = unit("((\"left\"))");
        let name = unit("left_local");
        let tail_group = unit("(left_local)");
        let tail = unit("((left_local))");
        let right = unit("\"right\"");
        let right_tail = unit("right_local");
        let local = fixture.symbol(0, "left_local");
        let right_local = fixture.symbol(0, "right_local");
        let chosen = fixture.symbol(0, "chosen");
        let graph = owned.owner_observations()[0]
            .value_observation_v1()
            .unwrap();
        assert_ast_graph(&fixture, control_text, &graph);
        let Event::Drop(point, _, _) = graph.suffix()[1] else {
            panic!("AST checked cleanup")
        };
        // Synthetic mutations check guards only; the tests above exercise real producer hooks.
        for mutation in [
            "valid",
            "no-bind",
            "duplicate-bind",
            "early-bind",
            "wrong-symbol",
            "wrong-initializer",
            "no-move",
            "failed-take",
            "wrong-take",
            "cross-edge-move",
            "duplicate-move",
            "read-name",
            "no-name-completion",
            "no-initializer-group",
            "no-tail-group",
            "early-right",
            "local-at-edge",
            "duplicate-create",
            "wrong-edge-tail",
            "non-continuing-edge",
            "no-control-completion",
            "local-drop",
        ] {
            let mut tx = super::super::choice::ChoiceTransaction::begin(
                checker,
                checker.parsed.roots()[0],
                std::sync::Arc::new(()),
            )
            .unwrap();
            if mutation == "early-right" {
                tx.string_completed(right);
            }
            tx.string_completed(left);
            if mutation == "duplicate-create" {
                tx.string_completed(left);
            }
            if mutation == "early-bind" {
                tx.bound(local, initializer);
            }
            tx.expression_completed(left, true, true);
            if mutation != "no-initializer-group" {
                tx.expression_completed(group, true, true);
            }
            tx.expression_completed(initializer, true, true);
            if mutation != "no-bind" {
                tx.bound(
                    if mutation == "wrong-symbol" {
                        right_local
                    } else {
                        local
                    },
                    if mutation == "wrong-initializer" {
                        left
                    } else {
                        initializer
                    },
                );
            }
            if mutation == "duplicate-bind" {
                tx.bound(local, initializer);
            }
            if mutation == "local-at-edge" {
                tx.edge_completed(control, 0, Some(tail), true);
            }
            if mutation == "local-drop" {
                tx.dropped(point, local);
            }
            if mutation != "no-move" {
                tx.moved(
                    if mutation == "cross-edge-move" {
                        right_tail
                    } else {
                        name
                    },
                    match mutation {
                        "failed-take" => None,
                        "wrong-take" => Some(right_local),
                        _ => Some(local),
                    },
                );
            }
            if mutation == "duplicate-move" {
                tx.moved(name, Some(local));
            }
            if mutation != "no-name-completion" {
                tx.expression_completed(name, mutation != "read-name", true);
            }
            if mutation != "no-tail-group" {
                tx.expression_completed(tail_group, true, true);
            }
            tx.expression_completed(tail, true, true);
            tx.edge_completed(
                control,
                0,
                Some(if mutation == "wrong-edge-tail" {
                    initializer
                } else {
                    tail
                }),
                mutation != "non-continuing-edge",
            );
            tx.string_completed(right);
            tx.expression_completed(right, true, true);
            tx.bound(right_local, right);
            tx.moved(right_tail, Some(right_local));
            tx.expression_completed(right_tail, true, true);
            tx.edge_completed(control, 1, Some(right_tail), true);
            if mutation != "no-control-completion" {
                tx.expression_completed(control, true, true);
            }
            tx.bound(chosen, control);
            tx.dropped(point, chosen);
            assert_eq!(tx.finish().is_some(), mutation == "valid", "{mutation}");
        }
    });
}
