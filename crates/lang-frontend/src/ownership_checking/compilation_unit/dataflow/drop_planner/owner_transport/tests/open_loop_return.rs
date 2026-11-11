//! Ordinary source requires two independently delivered terminal endpoints.
use super::*;
use crate::ownership_checking::{
    CompilationUnitOwnership, UnitDropTarget, UnitOwnerCallable, UnitOwnerOpenLoopObservationV1,
    UnitOwnerOpenLoopReturnV1, UnitOwnerOrigin, UnitOwnerReturnActionV1 as A,
    UnitOwnerTransportEvent as E,
};
use crate::parser::{Expression, FunctionBody, FunctionForm, Item, Statement};
use crate::{ownership_checking::UnitDropPoint, type_checking::UnitStatementId};

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); return held }
    return held
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn loop_owned_return_requires_open_transport() {
    let fixture = Fixture::new(&[("open-loop-return.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    let held = fixture.local(0, "held");
    let previous = fixture.local(0, "previous");
    assert!(
        owned
            .drops()
            .iter()
            .all(|drop| drop.target() != UnitDropTarget::Named(held))
    );
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(previous))
            .count(),
        1
    );
    compare(&fixture, &fixture.parts(&owned));
    assert_eq!(
        owned
            .owner_observations()
            .iter()
            .filter(|observation| observation.open_loop_value.is_some())
            .count(),
        1,
        "the real loop body Return and separate false-path Return need an open transport product"
    );
    check_product(&fixture, 0, &owned);
}

fn product(
    owned: &CompilationUnitOwnership,
    source: crate::name_resolution::SourceUnitId,
) -> &UnitOwnerOpenLoopReturnV1 {
    let observations: Vec<_> = owned.owner_observations().iter().filter(|observation| matches!(observation.callable(), UnitOwnerCallable::Function(function) if function.source_unit() == source)).filter_map(|observation| {
        let product = observation.open_loop_observation_v1()?;
        assert!(observation.replace_break_observation_v1().is_none());
        assert!(observation.loop_observation_v1().is_none());
        assert!(observation.value_observation_v1().is_none());
        match product { UnitOwnerOpenLoopObservationV1::ReturnDelivered(value) => Some(value), _ => panic!("Return must not become live Break") }
    }).collect();
    assert_eq!(observations.len(), 1);
    observations[0]
}

fn check_product(fixture: &Fixture, file: usize, owned: &CompilationUnitOwnership) {
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    let product = product(owned, fixture.source(file));
    assert!(product.is_consistent());
    let ast = fixture.parsed[file].ast();
    let Item::Function {
        form:
            FunctionForm::Explicit {
                body: FunctionBody::Block(block),
                ..
            },
        ..
    } = ast
        .items()
        .get(fixture.parsed[file].roots()[0])
        .unwrap()
        .payload()
    else {
        panic!("explicit String function")
    };
    let Statement::Block { elements } = ast.statements().get(*block).unwrap().payload() else {
        panic!("function block")
    };
    let [prefix, loop_statement, tail] = elements.as_slice() else {
        panic!("prefix, loop and distinct tail")
    };
    let Statement::While {
        condition, body, ..
    } = ast.statements().get(*loop_statement).unwrap().payload()
    else {
        panic!("actual While")
    };
    let Statement::Block {
        elements: body_elements,
    } = ast.statements().get(*body).unwrap().payload()
    else {
        panic!("body block")
    };
    let [previous, body_tail] = body_elements.as_slice() else {
        panic!("Replace and terminal Return")
    };
    let expression = |id| UnitExpressionId::new(fixture.source(file), id);
    let statement = |id| UnitStatementId::new(fixture.source(file), id);
    let initializer = |statement| {
        let Statement::LocalVariable { declaration } =
            ast.statements().get(statement).unwrap().payload()
        else {
            panic!("actual binding")
        };
        let Item::Variable { initializer, .. } = ast.items().get(*declaration).unwrap().payload()
        else {
            panic!("actual initializer")
        };
        expression(*initializer)
    };
    let actual_return = |statement| {
        let Statement::Expression {
            expression: returned,
        } = ast.statements().get(statement).unwrap().payload()
        else {
            panic!("terminal expression")
        };
        let Expression::Return {
            value: Some(payload),
            ..
        } = ast.expressions().get(*returned).unwrap().payload()
        else {
            panic!("actual Return with payload")
        };
        (expression(*returned), expression(*payload))
    };
    let (body_return, body_payload) = actual_return(*body_tail);
    let (false_return, false_payload) = actual_return(*tail);
    let root = fixture.local(file, "held");
    let detached = fixture.local(file, "previous");
    assert_eq!(
        (product.statement(), product.condition(), product.body()),
        (
            statement(*loop_statement),
            expression(*condition),
            statement(*body)
        )
    );
    assert_eq!(
        (
            product.root(),
            product.previous(),
            product.previous_statement()
        ),
        (root, detached, statement(*previous))
    );
    assert_eq!(
        (product.initial(), product.call()),
        (initializer(*prefix), initializer(*previous))
    );
    let Expression::Call { arguments, .. } = ast
        .expressions()
        .get(product.call().expression())
        .unwrap()
        .payload()
    else {
        panic!("checked Replace")
    };
    assert_eq!(product.argument(), expression(arguments[1].value));
    let p = product.entry().owner();
    let q = product.body_return().owner();
    assert_ne!(p, q);
    assert_eq!(
        product
            .input_producers()
            .definition(p.clone())
            .unwrap()
            .origin(),
        UnitOwnerOrigin::String(product.initial())
    );
    assert_eq!(
        product
            .body_producers()
            .definition(q.clone())
            .unwrap()
            .origin(),
        UnitOwnerOrigin::String(product.argument())
    );
    assert_eq!(product.entry(), product.condition_false_exit());
    for (endpoint, returned, payload, owner) in [
        (product.body_return(), body_return, body_payload, q.clone()),
        (
            product.condition_false_return(),
            false_return,
            false_payload,
            p.clone(),
        ),
    ] {
        assert!(endpoint.is_consistent());
        assert_eq!(
            (
                endpoint.return_expression(),
                endpoint.payload(),
                endpoint.root(),
                endpoint.owner()
            ),
            (returned, payload, root, owner.clone())
        );
        assert_eq!(
            endpoint.actions(),
            [
                A::Take {
                    expression: payload,
                    root,
                    owner: owner.clone()
                },
                A::Return {
                    expression: returned,
                    payload,
                    owner
                }
            ]
        );
        assert!(owned.drops().iter().all(|drop| {
            ![
                UnitDropTarget::Named(root),
                UnitDropTarget::Temporary(payload),
            ]
            .contains(&drop.target())
        }));
    }
    assert_ne!(body_return, false_return);
    assert_ne!(body_payload, false_payload);
    assert_eq!(
        product.prefix_events(),
        [
            E::Result {
                expression: product.initial(),
                owner: p.clone()
            },
            E::Binding {
                symbol: root,
                initializer: product.initial(),
                owner: p.clone()
            }
        ]
    );
    assert_eq!(
        product.body_events(),
        [
            E::Result {
                expression: product.argument(),
                owner: q.clone()
            },
            E::Pending {
                call: product.call(),
                argument: product.argument(),
                owner: q.clone()
            },
            E::Replace {
                call: product.call(),
                root,
                argument: product.argument(),
                previous: p.clone(),
                incoming: q.clone()
            },
            E::Result {
                expression: product.call(),
                owner: p.clone()
            },
            E::Binding {
                symbol: detached,
                initializer: product.call(),
                owner: p.clone()
            },
            E::Drop {
                point: UnitDropPoint::AfterStatement(statement(*previous)),
                symbol: detached,
                owner: p
            },
            E::Move {
                expression: body_payload,
                symbol: root,
                owner: q.clone()
            },
            E::Result {
                expression: body_payload,
                owner: q
            },
        ]
    );
    let previous_drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(detached))
        .collect();
    assert_eq!(previous_drops.len(), 1);
    assert_eq!(
        previous_drops[0].point(),
        UnitDropPoint::AfterStatement(statement(*previous))
    );
}

#[test]
fn loop_owned_return_preserves_source_and_analysis_identity() {
    let second = SOURCE
        .replace("fun run(", "fun second(")
        .replace("fun entry()", "fun next_entry()")
        .replace("= run(", "= second(")
        .replace("\"next\"", "\"outer\"");
    let fixture = Fixture::new(&[("first.ko", SOURCE), ("second.ko", &second)]);
    let first = fixture.shared();
    let repeated = fixture.shared();
    for file in 0..2 {
        check_product(&fixture, file, &first);
        check_product(&fixture, file, &repeated);
    }
    assert_ne!(
        product(&first, fixture.source(0)).entry().owner(),
        product(&first, fixture.source(1)).entry().owner()
    );
    assert_ne!(
        product(&first, fixture.source(0)).body_return().owner(),
        product(&repeated, fixture.source(0)).body_return().owner()
    );
    compare(&fixture, &fixture.parts(&first));
}

#[test]
fn loop_owned_return_declines_legal_neighbor_shapes() {
    for text in [
        SOURCE.replace("while(flag)", "while((flag))"),
        SOURCE.replace("while(flag)", "while(!flag)"),
        SOURCE.replace("return held", "return (held)"),
        SOURCE.replace("return held", "return \"other\""),
        SOURCE.replace(
            "val previous = replace(&held, \"next\"); return held",
            "val previous = replace(&held, \"next\"); val moved = held; return moved",
        ),
    ] {
        let fixture = Fixture::new(&[("neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.clone().validate().is_ok());
        assert!(
            owned
                .owner_observations()
                .iter()
                .all(|observation| observation.open_loop_observation_v1().is_none())
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn ci_only_open_loop_return_rejects_damaged_and_withheld_products() {
    let fixture = Fixture::new(&[("guard.ko", SOURCE)]);
    let owned = fixture.shared();
    let original = product(&owned, fixture.source(0));
    for damage in 0..8 {
        let mut value = original.clone();
        match damage {
            0 => value.body_return.cleaned = false,
            1 => value.condition_false_return.cleaned = false,
            2 => value.body_stopped = false,
            3 => value.condition_false_return = value.body_return.clone(),
            4 => {
                value.body_return.actions.pop();
            }
            5 => {
                value.prefix.events.pop();
            }
            6 => value.prefix.installed.owner = value.prefix.entry.owner(),
            _ => value.body_return.target.payload = value.condition_false_return.payload(),
        }
        assert!(!value.is_consistent());
    }
    use crate::ownership_checking::compilation_unit::owner_transport::publish_observations;
    assert!(publish_observations(owned.owner_observations().to_vec(), false, true).is_empty());
    let withheld = publish_observations(owned.owner_observations().to_vec(), true, false);
    assert!(
        withheld
            .iter()
            .all(|observation| observation.open_loop_value.is_none()
                && observation.open_loop_observation_v1().is_none())
    );
}
