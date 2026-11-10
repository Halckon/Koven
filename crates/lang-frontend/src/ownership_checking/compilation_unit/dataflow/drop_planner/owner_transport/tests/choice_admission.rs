//! Typed admission and actual planner events, not synthetic Choice construction.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerObservationStatus, UnitOwnerOrigin,
    UnitOwnerUnsupportedReason, UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::type_checking::UnitStatementId;

#[test]
fn actual_choice_is_delivered_bound_and_dropped_while_legacy_stays_unsupported() {
    for arms in [
        "\"left\" else \"right\"",
        "((\"left\")) else { (\"right\") }",
        "{ val local = \"left\"; local } else \"right\"",
    ] {
        let text = format!(
            "fun run(flag: Boolean): Unit {{ val chosen = if (flag) {arms} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}"
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        // Expected identities come from the source AST, not either ownership product.
        let control = UnitExpressionId::new(
            fixture.source(0),
            fixture.expression(0, &format!("if (flag) {arms}")),
        );
        let statements: Vec<_> = fixture.parsed[0]
            .ast()
            .statements()
            .iter()
            .filter_map(|(id, node)| match node.payload() {
                crate::parser::Statement::LocalVariable { declaration }
                    if matches!(
                        fixture.parsed[0].ast().items().get(*declaration).unwrap().payload(),
                        crate::parser::Item::Variable { initializer, .. }
                            if *initializer == control.expression()
                    ) =>
                {
                    Some(UnitStatementId::new(fixture.source(0), id))
                }
                _ => None,
            })
            .collect();
        let [chosen_statement] = statements.as_slice() else {
            panic!("exactly one local declaration initialized by this If");
        };
        let expected_drop = UnitDropPoint::AfterStatement(*chosen_statement);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.clone().validate().is_ok());
        let legacy = &owned.owner_observations()[0];
        assert_eq!(
            legacy.status(),
            UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterExpression(control),
                reason: UnitOwnerUnsupportedReason::Syntax,
            }
        );
        assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
        let graph = legacy
            .value_observation_v1()
            .expect("actual narrow Choice collection");
        let mut without_capability = legacy.clone();
        without_capability.choice_value = None;
        assert_eq!(legacy, &without_capability);
        assert!(without_capability.value_observation_v1().is_none());
        assert!(graph.is_consistent());
        let choice = graph.choice().unwrap();
        assert_eq!(graph.producers().definitions().len(), 2);
        assert_ne!(choice.inputs()[0].owner(), choice.inputs()[1].owner());
        for (edge, literal) in ["\"left\"", "\"right\""].iter().enumerate() {
            let id = choice.inputs()[edge].owner();
            assert!(
                legacy.definition(id.clone()).is_none(),
                "new transaction must not repopulate legacy Unsupported"
            );
            assert_eq!(
                graph.producers().definition(id).unwrap().origin(),
                UnitOwnerOrigin::String(UnitExpressionId::new(
                    fixture.source(0),
                    fixture.expression(0, literal)
                ))
            );
        }
        let chosen = fixture.symbol(0, "chosen");
        let [
            Event::Bind(symbol, initializer, Value::Choice(bound)),
            Event::Drop(point, dropped, Value::Choice(released)),
        ] = graph.suffix()
        else {
            panic!("one real binding followed by its actual cleanup");
        };
        assert_eq!(*symbol, chosen);
        assert_eq!(*dropped, chosen);
        assert_eq!(choice.id().control(), control);
        assert_eq!(*initializer, control);
        assert_eq!(bound, choice.id());
        assert_eq!(released, choice.id());
        assert_eq!(*point, expected_drop);
        let chosen_drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(chosen))
            .collect();
        assert_eq!(chosen_drops.len(), 1, "chosen is cleaned exactly once");
        assert_eq!(chosen_drops[0].point(), expected_drop);
        assert_eq!(
            owned.drops().len(),
            2,
            "chosen and sibling only; no arm temporary cleanup"
        );
        assert_eq!(
            owned
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(fixture.symbol(0, "safe")))
                .count(),
            1,
            "the independent sibling retains its own cleanup"
        );
        assert!(
            owned.owner_observations()[1]
                .value_observation_v1()
                .unwrap()
                .choice()
                .is_none()
        );
        let parts = fixture.parts(&owned);
        compare(&fixture, &parts);
    }
}

#[test]
fn choice_admission_rejects_exits_uses_calls_and_non_unit_functions_atomically() {
    for text in [
        "fun run(flag: Boolean): Unit { val chosen: String? = if (flag) \"left\" else \"right\" }",
        "fun run(own flag: Boolean): Unit { val chosen = if (flag) \"left\" else \"right\" }",
        "fun run(flag: Boolean): Unit { var chosen = if (flag) \"left\" else \"right\" }",
        "fun run(flag: Boolean): Unit { val before = \"prefix\"; val chosen = if (flag) \"left\" else \"right\" }",
        "fun run(flag: Boolean): Unit { val chosen = if (flag) { return } else \"right\" }",
        "fun run(flag: Boolean): String = if (flag) \"left\" else \"right\"",
        "fun run(flag: Boolean): Unit { val chosen = if (flag) (if (flag) \"a\" else \"b\") else \"right\" }",
        "fun make(): String = \"made\"\nfun run(flag: Boolean): Unit { val chosen = if (flag) make() else \"right\" }",
    ] {
        let fixture = Fixture::new(&[("root.ko", text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{text}");
        let legacy = owned.owner_observations().last().unwrap();
        assert!(legacy.value_observation_v1().is_none(), "{text}");
        let parts = fixture.parts(&owned);
        compare(&fixture, &parts);
    }
}

#[test]
fn choice_transaction_requires_unique_frame_and_control_completion() {
    let text = "fun run(flag: Boolean): Unit { val chosen = if (flag) \"left\" else \"right\" }";
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    let parts = fixture.parts(&owned);
    compare_and_inspect(&fixture, &parts, |checker| {
        let unit = |text| checker.unit_expression(fixture.expression(0, text));
        let control = unit("if (flag) \"left\" else \"right\"");
        let left = unit("\"left\"");
        let right = unit("\"right\"");
        let chosen = fixture.symbol(0, "chosen");
        let graph = owned.owner_observations()[0]
            .value_observation_v1()
            .unwrap();
        let Event::Drop(drop_point, _, _) = graph.suffix()[1] else {
            panic!("actual drop");
        };
        // Mutation traces exercise transaction guards, not AST execution evidence.
        for mutation in 0..9 {
            let mut tx = super::super::choice::ChoiceTransaction::begin(
                checker,
                checker.parsed.roots()[0],
                std::sync::Arc::new(()),
            )
            .unwrap();
            tx.string_completed(left);
            if mutation != 1 {
                tx.expression_completed(left, mutation != 7, true);
            }
            if mutation == 2 {
                tx.expression_completed(left, true, true);
            }
            if mutation == 3 {
                tx.expression_completed(left, true, false);
            }
            tx.edge_completed(control, 0, Some(left), true);
            tx.string_completed(right);
            tx.expression_completed(right, true, true);
            tx.edge_completed(control, 1, Some(right), true);
            if mutation != 4 {
                tx.expression_completed(control, true, true);
            }
            if mutation == 5 {
                tx.expression_completed(control, true, true);
            }
            tx.bound(chosen, control);
            tx.dropped(
                if mutation == 8 {
                    crate::ownership_checking::UnitDropPoint::AfterExpression(control)
                } else {
                    drop_point
                },
                chosen,
            );
            if mutation == 6 {
                tx.string_completed(left);
            }
            assert_eq!(tx.finish().is_some(), mutation == 0, "mutation {mutation}");
        }
    });
}

#[test]
fn choice_capability_cannot_be_attached_to_another_product_or_callable() {
    let text = "fun run(flag: Boolean): Unit { val chosen = if (flag) \"left\" else \"right\" }\nfun sibling(): Unit {}";
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    let graph = owned.owner_observations()[0]
        .value_observation_v1()
        .unwrap();
    let independent = fixture.shared();
    let mut foreign = independent.owner_observations()[0].clone();
    foreign.choice_value = Some(Box::new(graph.clone()));
    assert!(foreign.value_observation_v1().is_none());
    let mut sibling = owned.owner_observations()[1].clone();
    sibling.choice_value = Some(Box::new(graph));
    assert!(sibling.value_observation_v1().is_none());
}
