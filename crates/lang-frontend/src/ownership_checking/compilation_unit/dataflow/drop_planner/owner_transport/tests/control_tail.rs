//! Internal tail handoff is not admission of a result-valued If observation.
use super::super::super::{
    DropExpressionUse, DropPlanner, PlannerDropPoint, PlannerDropTarget, ValueState,
};
use super::*;
use crate::ownership_checking::{
    UnitOwnerObservationStatus, UnitOwnerOrigin, UnitOwnerTransportEvent,
    UnitOwnerUnsupportedReason,
};
use crate::parser::Expression;

#[test]
fn control_body_retains_only_the_actual_consumed_tail_token() {
    for (body, tail) in [
        ("\"left\"", "\"left\""),
        ("{ val local = \"left\"; (local) }", "(local)"),
    ] {
        let control = format!("if (flag) {body} else \"right\"");
        let text = format!("fun run(flag: Boolean): Unit {{ val chosen = {control} }}");
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        let parts = fixture.parts(&shared);
        compare_and_inspect(&fixture, &parts, |checker| {
            let Expression::If { then_branch, .. } = checker
                .parsed
                .ast()
                .expressions()
                .get(fixture.expression(0, &control))
                .unwrap()
                .payload()
            else {
                panic!("If fixture")
            };
            // Exercise only the body adapter, not a new If admission path.
            let mut planner = DropPlanner::new(checker, liveness::build(checker, true).unwrap());
            planner.begin_owner_function(checker.parsed.roots()[0]);
            let mut state = ValueState::default();
            let (expression, result) = planner
                .evaluate_control_body(*then_branch, DropExpressionUse::Consume, &mut state)
                .unwrap();
            assert!(result.continues);
            assert_eq!(expression, Some(fixture.expression(0, tail)));
            let owner = result.owner.expect("real evaluated String owner");
            let observation = planner.owner_observation.as_ref().unwrap();
            assert_eq!(observation.definitions.len(), 1, "no synthetic producer");
            assert_eq!(observation.definitions[0].id(), owner);
            assert_eq!(observation.events.iter().filter(|event| matches!(event,
                UnitOwnerTransportEvent::Result { expression, owner: actual }
                if *expression == checker.unit_expression(fixture.expression(0, tail)) && *actual == owner
            )).count(), 1, "tail evaluated exactly once");
            assert_eq!(planner.scope_depth, 0);
            assert_eq!(
                observation.definitions[0].origin(),
                UnitOwnerOrigin::String(checker.unit_expression(fixture.expression(0, "\"left\"")))
            );
            assert!(
                state.values.is_empty(),
                "consumed local must not survive cleanup"
            );
        });
        assert!(matches!(
            shared.owner_observations()[0].status(),
            UnitOwnerObservationStatus::Unsupported {
                reason: UnitOwnerUnsupportedReason::Syntax,
                ..
            }
        ));
        assert!(shared.owner_observations()[0].definitions().is_empty());
        assert!(shared.owner_observations()[0].events().is_empty());
    }
}

#[test]
fn read_tail_cannot_hand_out_a_cleaned_temporary() {
    let text = "fun run(flag: Boolean): Unit { val chosen = if (flag) \"left\" else \"right\" }";
    let fixture = Fixture::new(&[("root.ko", text)]);
    let shared = fixture.shared();
    let parts = fixture.parts(&shared);
    compare_and_inspect(&fixture, &parts, |checker| {
        let Expression::If { then_branch, .. } = checker
            .parsed
            .ast()
            .expressions()
            .get(fixture.expression(0, "if (flag) \"left\" else \"right\""))
            .unwrap()
            .payload()
        else {
            panic!("If fixture")
        };
        let mut planner = DropPlanner::new(checker, liveness::build(checker, true).unwrap());
        planner.begin_owner_function(checker.parsed.roots()[0]);
        let (expression, result) = planner
            .evaluate_control_body(
                *then_branch,
                DropExpressionUse::Read,
                &mut ValueState::default(),
            )
            .unwrap();
        assert!(result.continues);
        assert_eq!(expression, Some(fixture.expression(0, "\"left\"")));
        assert!(result.owner.is_none());
        assert_eq!(
            planner.facts.len(),
            1,
            "original temporary cleanup preserved"
        );
        let tail = fixture.expression(0, "\"left\"");
        assert_eq!(
            planner.facts[0].target(),
            PlannerDropTarget::Temporary(tail)
        );
        assert_eq!(
            planner.facts[0].point(),
            PlannerDropPoint::AfterExpression(tail)
        );
    });
}

#[test]
fn empty_statement_and_stopped_bodies_have_no_owner_delivery() {
    for (body, continues, has_tail) in [
        ("{}", true, false),
        ("{ val local = \"unused\" }", true, false),
        ("{ return }", false, true),
        ("{ return; val unreachable = \"never\" }", false, false),
    ] {
        let control = format!("if (flag) {body} else {{}}");
        let text = format!("fun run(flag: Boolean): Unit {{ {control} }}");
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        let parts = fixture.parts(&shared);
        compare_and_inspect(&fixture, &parts, |checker| {
            let Expression::If { then_branch, .. } = checker
                .parsed
                .ast()
                .expressions()
                .get(fixture.expression(0, &control))
                .unwrap()
                .payload()
            else {
                panic!("If fixture")
            };
            let mut planner = DropPlanner::new(checker, liveness::build(checker, true).unwrap());
            planner.begin_owner_function(checker.parsed.roots()[0]);
            let (expression, result) = planner
                .evaluate_control_body(
                    *then_branch,
                    DropExpressionUse::Consume,
                    &mut ValueState::default(),
                )
                .unwrap();
            assert_eq!(result.continues, continues, "{body}");
            assert_eq!(expression.is_some(), has_tail, "{body}");
            assert!(result.owner.is_none(), "{body}");
            assert_eq!(planner.scope_depth, 0, "scope restored for {body}");
        });
    }
}

#[test]
fn shared_control_body_keeps_when_and_replacement_cleanup_unchanged() {
    for body in [
        "val selected = when { flag -> { val local = \"left\"; (local) }\nelse -> \"right\" }",
        "val subject = \"tag\"; val selected = when (subject) { \"tag\" -> { val local = \"left\"; (local) }\nelse -> \"right\" }",
        "val selected = when (flag) { true -> \"left\"\nelse -> \"right\" }",
        "var root = \"old\"; val previous = replace(&root, if (flag) { val local = \"left\"; (local) } else \"right\")",
        "var root = if (flag) { val local = \"left\"; local } else (\"right\"); val previous = replace(&root, \"next\")",
    ] {
        let text = format!("fun run(flag: Boolean): Unit {{ {body} }}");
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok(), "{body}");
        let parts = fixture.parts(&shared);
        compare(&fixture, &parts);
        let observation = &shared.owner_observations()[0];
        assert!(matches!(
            observation.status(),
            UnitOwnerObservationStatus::Unsupported { .. }
        ));
        assert!(observation.definitions().is_empty());
        assert!(observation.events().is_empty());
    }
}
