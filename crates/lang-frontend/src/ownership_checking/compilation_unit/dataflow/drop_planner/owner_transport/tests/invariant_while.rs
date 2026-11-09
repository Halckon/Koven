//! Real nonempty owner boundaries; no synthesized input owner or repeated trace.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerObservationStatus,
    UnitOwnerOrigin, UnitOwnerUnsupportedReason, UnitOwnerValueEventV1 as Event,
    UnitOwnerValueV1 as Value,
};
use crate::parser::Statement;
use crate::type_checking::UnitStatementId;

pub(super) fn check(
    fixture: &Fixture,
    index: usize,
    owned: &crate::ownership_checking::CompilationUnitOwnership,
    expected_jump: Option<ExpressionId>,
) {
    let source = fixture.source(index);
    let legacy = &owned.owner_observations()[index];
    let (statement, condition, body) = fixture.parsed[index]
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| {
            if let Statement::While {
                condition, body, ..
            } = node.payload()
            {
                Some((id, *condition, *body))
            } else {
                None
            }
        })
        .unwrap();
    let statement = UnitStatementId::new(source, statement);
    let condition = UnitExpressionId::new(source, condition);
    let body = UnitStatementId::new(source, body);
    assert_eq!(
        legacy.status(),
        UnitOwnerObservationStatus::Unsupported {
            point: UnitDropPoint::AfterStatement(statement),
            reason: UnitOwnerUnsupportedReason::Syntax
        }
    );
    assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
    assert!(legacy.value_observation_v1().is_none());
    assert!(legacy.loop_observation_v1().is_none());
    assert!(legacy.invariant_break_observation_v1().is_none());
    let observed = legacy
        .invariant_loop_observation_v1()
        .expect("actual nonempty loop boundaries");
    assert!(observed.is_consistent());
    assert_eq!(
        (observed.statement(), observed.condition(), observed.body()),
        (statement, condition, body)
    );
    let backedge = if let Some(jump) = expected_jump {
        let jump = UnitExpressionId::new(source, jump);
        assert_eq!(observed.continue_boundary(), Some((jump, observed.entry())));
        assert!(matches!(
            fixture.parsed[index]
                .ast()
                .expressions()
                .get(jump.expression())
                .unwrap()
                .payload(),
            crate::parser::Expression::Continue { .. }
        ));
        Edge::ContinueBackedge(UnitDropPoint::ControlTransfer(jump), condition)
    } else {
        assert!(observed.continue_boundary().is_none());
        Edge::Backedge(UnitDropPoint::AfterStatement(body), condition)
    };
    assert_eq!(
        observed.edges(),
        &[
            Edge::EnterBody(condition, body),
            backedge,
            Edge::ConditionFalseExit(condition, UnitDropPoint::LoopExit(statement))
        ]
    );
    let held = fixture.local(index, "held");
    let retired = fixture.local(index, "retired");
    assert_eq!(observed.entry(), observed.backedge());
    assert_eq!(observed.entry(), observed.exit());
    assert_eq!(observed.entry().symbol(), held);
    let catalog = observed.input_producers();
    assert_eq!(catalog.definitions().len(), 1);
    let owner = observed.entry().owner();
    assert_eq!(
        catalog.definition(owner.clone()).unwrap().origin(),
        UnitOwnerOrigin::String(UnitExpressionId::new(
            source,
            fixture.expression(index, "\"outer\"")
        ))
    );
    let transport = observed.body_transport();
    assert!(transport.is_consistent());
    assert!(transport.producers().definition(owner.clone()).is_none());
    assert!(transport.choice().is_none() && transport.suffix().is_empty());
    for definition in transport.producers().definitions() {
        assert!(catalog.definition(definition.id()).is_none());
        let UnitOwnerOrigin::String(expression) = definition.origin() else {
            panic!("real body String");
        };
        assert_eq!(expression.source_unit(), source);
        assert!(transport.prefix().iter().any(|e| matches!(e, Event::Create(id, token) if *id == expression && *token == definition.id())));
        let drops = transport
            .prefix()
            .iter()
            .filter_map(|e| match e {
                Event::Drop(point, symbol, Value::Definition(token))
                    if *token == definition.id() =>
                {
                    Some((*point, *symbol))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        for (point, symbol) in drops {
            assert_ne!(symbol, held);
            assert!(
                owned
                    .drops()
                    .iter()
                    .any(|d| d.point() == point && d.target() == UnitDropTarget::Named(symbol))
            );
        }
    }
    assert!(
        owned
            .drops()
            .iter()
            .all(|d| d.target() != UnitDropTarget::Named(held))
    );
    let retirement = fixture.parsed[index]
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| {
            fixture
                .sources
                .slice(node.span())
                .unwrap()
                .starts_with("val retired")
                .then_some(UnitStatementId::new(source, id))
        })
        .unwrap();
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(retired))
            .count(),
        1
    );
    assert!(
        owned
            .drops()
            .iter()
            .any(|d| d.target() == UnitDropTarget::Named(retired)
                && d.point() == UnitDropPoint::AfterStatement(retirement))
    );
    // These legacy suffix drops are compatibility witnesses, not new-capability suffix events.
}

#[test]
fn actual_outer_owner_survives_three_while_boundaries() {
    for condition in ["flag", "(flag)", "((flag))"] {
        for result in ["", ": Unit"] {
            for (body, producers, replacements) in [
                (r#"val local = "inner""#, 1, 0),
                (r#"val local = "inner"; val alias = (local)"#, 1, 0),
                (
                    r#"var root = "old"; val incoming = "next"; val previous = replace(&root, incoming)"#,
                    2,
                    1,
                ),
                (
                    r#"var left = "left"; var right = "right"; val previous = replace(&left, replace(&right, "next"))"#,
                    3,
                    2,
                ),
            ] {
                let text = format!(
                    r#"fun run(flag: Boolean){result} {{ val held = "outer"; while ({condition}) {{ {body} }}; val retired = held }}"#
                );
                let fixture = Fixture::new(&[("root.ko", &text)]);
                let owned = fixture.shared();
                assert!(owned.clone().validate().is_ok(), "{text}");
                check(&fixture, 0, &owned, None);
                let graph = owned.owner_observations()[0]
                    .invariant_loop_observation_v1()
                    .unwrap()
                    .body_transport();
                assert_eq!(graph.producers().definitions().len(), producers);
                assert_eq!(
                    graph
                        .prefix()
                        .iter()
                        .filter(|event| matches!(event, Event::Replace { .. }))
                        .count(),
                    replacements
                );
                compare(&fixture, &fixture.parts(&owned));
            }
        }
    }
}

#[test]
fn nonempty_loop_rejects_unproved_carriers_and_control_atomically() {
    for text in [
        r#"fun run(flag: Boolean) { val held = ("outer"); while(flag) { val local = "inner" }; val retired = held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; val retired = (held) }"#,
        r#"fun run(flag: Boolean) { var held = "outer"; while(flag) { val local = "inner" }; val retired = held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; var retired = held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; val retired: String? = held }"#,
        r#"fun run(flag: Boolean) { val held: String? = "outer"; while(flag) { val local = "inner" }; val retired = held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; val retired = "other" }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; val second = "second"; while(flag) { val local = "inner" }; val retired = held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; val retired = held; val extra = "extra" }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner"; break }; val retired = held }"#,
        r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) {} else {} }; val retired = held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { while(flag) { val local = "inner" } }; val retired = held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; while(flag) { val local = "second" }; val retired = held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) {}; val retired = held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(false) { val local = "inner" }; val retired = held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = held + "inner" }; val retired = held }"#,
        r#"fun run(flag: Boolean, own held: String) { while(flag) { val local = "inner" }; val retired = held }"#,
    ] {
        let fixture = Fixture::new(&[("root.ko", text)]);
        let owned = fixture.shared();
        assert!(
            owned.clone().validate().is_ok(),
            "negative must be valid source: {text}"
        );
        assert_eq!(owned.owner_observations().len(), 1);
        assert!(
            owned.owner_observations()[0]
                .loop_observation_v1()
                .is_none()
        );
        assert!(
            owned.owner_observations()[0]
                .value_observation_v1()
                .is_none()
        );
        assert!(
            owned
                .owner_observations()
                .iter()
                .all(|o| o.invariant_loop_observation_v1().is_none()),
            "{text}"
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn invariant_loop_sources_and_analyses_do_not_share_owner_tokens() {
    let first = r#"fun first(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; val retired = held }"#;
    let second = first.replace("first", "second");
    let fixture = Fixture::new(&[("a.ko", first), ("b.ko", &second)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    assert!(owned.clone().validate().is_ok() && again.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 2);
    for index in 0..2 {
        check(&fixture, index, &owned, None);
        let observed = owned.owner_observations()[index]
            .invariant_loop_observation_v1()
            .unwrap();
        for foreign in [
            again.owner_observations()[index]
                .invariant_loop_observation_v1()
                .unwrap(),
            owned.owner_observations()[1 - index]
                .invariant_loop_observation_v1()
                .unwrap(),
        ] {
            assert!(
                observed
                    .input_producers()
                    .definition(foreign.entry().owner())
                    .is_none()
            );
            assert!(
                observed
                    .body_transport()
                    .producers()
                    .definition(foreign.body_transport().producers().definitions()[0].id())
                    .is_none()
            );
        }
    }
    let invalid = format!("{first}\nfun bad(input: String) {{ val moved = input }}");
    let fixture = Fixture::new(&[("error.ko", &invalid)]);
    let failed = fixture.shared();
    assert!(failed.clone().validate().is_err());
    assert!(failed.owner_observations().is_empty());
    for body in [
        r#"val local = held"#,
        r#"borrow val alias = held"#,
        r#"var root = "inner"; val old = replace(&root, held)"#,
    ] {
        let text = format!(
            r#"fun run(flag: Boolean) {{ val held = "outer"; while(flag) {{ {body} }}; val retired = held }}"#
        );
        let fixture = Fixture::new(&[("body-error.ko", &text)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err(), "{text}");
        assert!(
            failed
                .owner_observations()
                .iter()
                .all(|o| o.invariant_loop_observation_v1().is_none())
        );
    }
}

// Deliberate damaged-product guards compile locally; execution belongs to formal CI.
#[test]
fn invariant_loop_seal_rejects_foreign_snapshots_and_deferred_publication() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; val retired = held }"#,
    )]);
    let owned = fixture.shared();
    let again = fixture.shared();
    let original = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    let foreign = again.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    for slot in 0..3 {
        let mut damaged = original.clone();
        match slot {
            0 => damaged.evidence.entry = foreign.evidence.entry.clone(),
            1 => damaged.evidence.body_completion = foreign.evidence.body_completion.clone(),
            _ => damaged.evidence.exit = foreign.evidence.exit.clone(),
        }
        assert!(!damaged.is_consistent());
    }
    let mut damaged = original.clone();
    damaged.evidence.inputs = foreign.evidence.inputs.clone();
    assert!(!damaged.is_consistent());
    let mut damaged = original.clone();
    damaged.evidence.closed.transport = foreign.evidence.closed.transport.clone();
    assert!(!damaged.is_consistent());
    let mut damaged = original.clone();
    damaged.evidence.closed.edges.pop();
    assert!(!damaged.is_consistent());
    let mut damaged = owned.owner_observations()[0].clone();
    damaged.invariant_loop_value = again.owner_observations()[0].invariant_loop_value.clone();
    assert!(damaged.invariant_loop_observation_v1().is_none());
    let incomplete =
        crate::ownership_checking::compilation_unit::owner_transport::publish_observations(
            owned.owner_observations().to_vec(),
            true,
            false,
        );
    assert!(incomplete[0].invariant_loop_observation_v1().is_none());
}
