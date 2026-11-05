//! Actual body evidence and the separately sealed whole-function Choice capability.
use super::super::super::{
    DropExpressionUse, DropPlanner, PlannerDropPoint, PlannerDropTarget, ValueState,
};
use super::*;
use crate::ownership_checking::{
    UnitDropTarget, UnitOwnerObservationStatus, UnitOwnerOrigin, UnitOwnerTransportEvent,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::parser::Expression;

#[test]
fn every_real_tail_group_forwards_the_same_branch_definition_in_order() {
    let control = "if (flag) { (((\"left\"))) } else (((\"right\")))";
    let text = format!("fun run(flag: Boolean): Unit {{ val chosen = {control} }}");
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let graph = owned.owner_observations()[0]
        .value_observation_v1()
        .unwrap();
    let choice = graph.choice().unwrap();
    assert_eq!(graph.producers().definitions().len(), 2);
    for (edge, literal) in ["\"left\"", "\"right\""].iter().enumerate() {
        let mut texts = vec![(*literal).to_owned()];
        for _ in 0..3 {
            texts.push(format!("({})", texts.last().unwrap()));
        }
        let expressions: Vec<_> = texts
            .iter()
            .map(|text| UnitExpressionId::new(fixture.source(0), fixture.expression(0, text)))
            .collect();
        let input = &choice.inputs()[edge];
        let owner = input.owner();
        assert_eq!(input.tail(), expressions[3]);
        assert_eq!(
            graph
                .producers()
                .definition(owner.clone())
                .unwrap()
                .origin(),
            UnitOwnerOrigin::String(expressions[0])
        );
        let events = &choice.events()[edge];
        assert_eq!(
            events.len(),
            4,
            "one constructor and exactly three real Group forwards"
        );
        assert!(
            matches!(&events[0], Event::Create(expression, actual) if *expression == expressions[0] && *actual == owner)
        );
        for index in 1..4 {
            assert!(
                matches!(&events[index], Event::Forward(from, to, Value::Definition(actual)) if *from == expressions[index - 1] && *to == expressions[index] && *actual == owner)
            );
        }
    }
    let parts = fixture.parts(&owned);
    compare(&fixture, &parts);
}

#[test]
fn branch_local_tail_preserves_real_body_evidence_and_publishes_choice_capability() {
    let control = "if (flag) { val left_local = \"left\"; ((left_local)) } else { val right_local = \"right\"; ((right_local)) }";
    let text = format!("fun run(flag: Boolean): Unit {{ val chosen = {control} }}");
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let legacy = &owned.owner_observations()[0];
    assert!(matches!(
        legacy.status(),
        UnitOwnerObservationStatus::Unsupported { .. }
    ));
    assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
    let graph = legacy.value_observation_v1().expect("branch-local Choice");
    super::choice_branch_local::assert_ast_graph(&fixture, control, &graph);
    for local in ["left_local", "right_local"] {
        assert!(
            !owned
                .drops()
                .iter()
                .any(|drop| drop.target() == UnitDropTarget::Named(fixture.symbol(0, local))),
            "consumed branch local must not also be cleaned by name"
        );
    }
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(fixture.symbol(0, "chosen")))
            .count(),
        1
    );
    let parts = fixture.parts(&owned);
    compare_and_inspect(&fixture, &parts, |checker| {
        let Expression::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } = checker
            .parsed
            .ast()
            .expressions()
            .get(fixture.expression(0, control))
            .unwrap()
            .payload()
        else {
            panic!("explicit If");
        };
        for (body, local, literal) in [
            (*then_branch, "left_local", "\"left\""),
            (*else_branch, "right_local", "\"right\""),
        ] {
            // Exercise each actual body adapter independently. This is not a new If join.
            let mut planner = DropPlanner::new(checker, liveness::build(checker, true).unwrap());
            planner.begin_owner_function(checker.parsed.roots()[0]);
            let mut state = ValueState::default();
            let tail_text = format!("(({local}))");
            let (tail, result) = planner
                .evaluate_control_body(body, DropExpressionUse::Consume, &mut state)
                .unwrap();
            assert!(result.continues);
            assert_eq!(tail, Some(fixture.expression(0, &tail_text)));
            let owner = result
                .owner
                .expect("real consumed local owner from the actual frame");
            let observation = planner.owner_observation.as_ref().unwrap();
            assert_eq!(observation.definitions.len(), 1);
            assert_eq!(
                observation.definitions[0].origin(),
                UnitOwnerOrigin::String(checker.unit_expression(fixture.expression(0, literal)))
            );
            assert_eq!(observation.definitions[0].id(), owner);
            let symbol = fixture.symbol(0, local);
            let use_expression = checker.unit_expression(fixture.expression(0, local));
            let tail_expression = checker.unit_expression(fixture.expression(0, &tail_text));
            let binding = observation.events.iter().position(|event| matches!(event, UnitOwnerTransportEvent::Binding { symbol: actual, owner: value, .. } if *actual == symbol && *value == owner)).unwrap();
            let moved = observation.events.iter().position(|event| matches!(event, UnitOwnerTransportEvent::Move { expression, symbol: actual, owner: value } if *expression == use_expression && *actual == symbol && *value == owner)).unwrap();
            let delivered = observation.events.iter().position(|event| matches!(event, UnitOwnerTransportEvent::Result { expression, owner: value } if *expression == tail_expression && *value == owner)).unwrap();
            assert!(binding < moved && moved < delivered);
            assert!(
                !planner
                    .facts
                    .iter()
                    .any(|fact| fact.target() == PlannerDropTarget::Named(symbol))
            );
            assert!(state.values.is_empty());
            assert_eq!(planner.scope_depth, 0);
        }
    });
}

#[test]
fn prefix_return_never_creates_an_unreachable_tail_or_a_choice_result() {
    let control = "if (flag) { val before = \"before\"; return; \"unreachable\" } else \"right\"";
    let text = format!(
        "fun run(flag: Boolean): Unit {{ val chosen = {control} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}"
    );
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert!(
        owned.owner_observations()[0]
            .value_observation_v1()
            .is_none()
    );
    assert!(owned.owner_observations()[0].definitions().is_empty());
    assert!(owned.owner_observations()[0].events().is_empty());
    assert!(
        owned.owner_observations()[1]
            .value_observation_v1()
            .is_some()
    );
    let parts = fixture.parts(&owned);
    compare_and_inspect(&fixture, &parts, |checker| {
        let Expression::If { then_branch, .. } = checker
            .parsed
            .ast()
            .expressions()
            .get(fixture.expression(0, control))
            .unwrap()
            .payload()
        else {
            panic!("If");
        };
        let mut planner = DropPlanner::new(checker, liveness::build(checker, true).unwrap());
        planner.begin_owner_function(checker.parsed.roots()[0]);
        let mut state = ValueState::default();
        let (tail, result) = planner
            .evaluate_control_body(*then_branch, DropExpressionUse::Consume, &mut state)
            .unwrap();
        assert!(!result.continues && result.owner.is_none() && tail.is_none());
        assert_eq!(planner.scope_depth, 0);
        assert!(state.values.is_empty());
        let observation = planner.owner_observation.as_ref().unwrap();
        assert_eq!(observation.definitions.len(), 1);
        assert_eq!(
            observation.definitions[0].origin(),
            UnitOwnerOrigin::String(checker.unit_expression(fixture.expression(0, "\"before\"")))
        );
        let unreachable = checker.unit_expression(fixture.expression(0, "\"unreachable\""));
        assert!(!observation.events.iter().any(|event| matches!(event, UnitOwnerTransportEvent::Result { expression, .. } if *expression == unreachable)));
        let crate::parser::Statement::ControlBody { elements } = checker
            .parsed
            .ast()
            .statements()
            .get(*then_branch)
            .unwrap()
            .payload()
        else {
            panic!("ControlBody");
        };
        let before_drop = planner
            .facts
            .iter()
            .find(|fact| fact.target() == PlannerDropTarget::Named(fixture.symbol(0, "before")))
            .unwrap();
        assert_eq!(
            before_drop.point(),
            PlannerDropPoint::AfterStatement(elements[0]),
            "unused prefix local is cleaned at declaration, not evidence of Return-specific cleanup"
        );
        assert_eq!(
            planner
                .facts
                .iter()
                .filter(
                    |fact| fact.target() == PlannerDropTarget::Named(fixture.symbol(0, "before"))
                )
                .count(),
            1
        );
    });
}
