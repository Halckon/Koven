//! A Break exit is independently proved, never fabricated as a loop backedge.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerObservationStatus,
    UnitOwnerOrigin, UnitOwnerUnsupportedReason, UnitOwnerValueEventV1 as Event,
    UnitOwnerValueV1 as Value,
};
use crate::parser::Statement;
use crate::type_checking::UnitStatementId;

pub(super) fn check_break(
    fixture: &Fixture,
    index: usize,
    owned: &crate::ownership_checking::CompilationUnitOwnership,
) {
    let source = fixture.source(index);
    let legacy = &owned.owner_observations()[index];
    assert!(legacy.invariant_mixed_join_observation_v1().is_none());
    assert!(legacy.invariant_normal_join_observation_v1().is_none());
    assert!(legacy.invariant_split_exit_observation_v1().is_none());
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
    assert!(legacy.invariant_split_continue_observation_v1().is_none());
    assert!(legacy.loop_observation_v1().is_none());
    assert!(legacy.invariant_loop_observation_v1().is_none());
    let observed = legacy
        .invariant_break_observation_v1()
        .expect("actual nonempty Break exit");
    assert!(observed.is_consistent());
    assert_eq!(
        (observed.statement(), observed.condition(), observed.body()),
        (statement, condition, body)
    );
    let jump = UnitExpressionId::new(source, fixture.expression(index, "break"));
    assert!(matches!(
        fixture.parsed[index]
            .ast()
            .expressions()
            .get(jump.expression())
            .unwrap()
            .payload(),
        crate::parser::Expression::Break { .. }
    ));
    assert_eq!(observed.break_boundary(), Some((jump, observed.entry())));
    let break_exit = Edge::BreakExit(
        UnitDropPoint::ControlTransfer(jump),
        UnitDropPoint::LoopExit(statement),
    );
    assert_eq!(
        observed.edges(),
        &[
            Edge::EnterBody(condition, body),
            break_exit,
            Edge::ConditionFalseExit(condition, UnitDropPoint::LoopExit(statement))
        ]
    );
    let held = fixture.local(index, "held");
    let retired = fixture.local(index, "retired");
    assert_eq!(observed.entry(), observed.body_completion());
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

fn assert_breaks(text: &str, producers: usize, replacements: usize) {
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok(), "{text}");
    assert_eq!(owned.owner_observations().len(), 1);
    let jump = fixture.expression(0, "break");
    check_break(&fixture, 0, &owned);
    let observed = owned.owner_observations()[0]
        .invariant_break_observation_v1()
        .unwrap();
    let graph = observed.body_transport();
    assert_eq!(graph.producers().definitions().len(), producers);
    assert_eq!(
        graph
            .prefix()
            .iter()
            .filter(|e| matches!(e, Event::Replace { .. }))
            .count(),
        replacements
    );
    let control = UnitDropPoint::ControlTransfer(UnitExpressionId::new(fixture.source(0), jump));
    assert!(owned.drops().iter().all(|drop| drop.point() != control));
    assert!(
        graph
            .prefix()
            .iter()
            .all(|event| !matches!(event, Event::Drop(point, _, _) if *point == control))
    );
    assert!(
        observed
            .edges()
            .iter()
            .all(|edge| !matches!(edge, Edge::Backedge(..) | Edge::ContinueBackedge(..)))
    );
    compare(&fixture, &fixture.parts(&owned));
}

#[test]
fn actual_break_preserves_the_owner_through_distinct_exit_states() {
    // Preserve the exact former §152 legal Break rejection as a new behavior witness.
    let former = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner"; break }; val retired = held }"#;
    assert_breaks(former, 1, 0);
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
                    r#"fun run(flag: Boolean){result} {{ val held = "outer"; while ({condition}) {{ {body}; break }}; val retired = held }}"#
                );
                assert_breaks(&text, producers, replacements);
            }
        }
    }
}

#[test]
fn break_exit_rejects_unproved_control_and_input_shapes() {
    for (body, old_backedge) in [
        (r#"val local = "inner""#, true),
        (r#"val local = "inner"; continue"#, true),
        (r#"val local = "inner"; (break)"#, false),
        (r#"val local = "inner"; break; val tail = "tail""#, false),
        (r#"val local = "inner"; return"#, false),
        (
            r#"val local = "inner"; if(flag) { break } else { break }"#,
            false,
        ),
        (r#"val local = "inner"; if(flag) {} else { break }"#, false),
        (
            r#"val local = "inner"; while(flag) { break }; break"#,
            false,
        ),
        (r#"val local = "inner"; { break }"#, false),
        (r#"break"#, false),
        (r#"val local = held + "inner"; break"#, false),
        (r#"val local: String? = "inner"; break"#, false),
        (r#"val local = "inner"; "temporary"; break"#, false),
    ] {
        let text = format!(
            r#"fun run(flag: Boolean) {{ val held = "outer"; while(flag) {{ {body} }}; val retired = held }}"#
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "valid negative: {text}");
        assert_eq!(owned.owner_observations().len(), 1);
        let legacy = &owned.owner_observations()[0];
        assert!(legacy.invariant_break_observation_v1().is_none(), "{text}");
        assert_eq!(
            legacy.invariant_loop_observation_v1().is_some(),
            old_backedge,
            "{text}"
        );
        assert!(legacy.value_observation_v1().is_none() && legacy.loop_observation_v1().is_none());
        compare(&fixture, &fixture.parts(&owned));
    }
    for (prefix, suffix) in [
        (r#"var held = "outer""#, "val retired = held"),
        (r#"val held = ("outer")"#, "val retired = held"),
        (r#"val held = "outer""#, "val retired = (held)"),
        (r#"val held = "outer""#, "var retired = held"),
        (r#"val held = "outer""#, "val retired: String? = held"),
        (
            r#"val held = "outer"; val second = "extra""#,
            "val retired = held",
        ),
    ] {
        let text = format!(
            r#"fun run(flag: Boolean) {{ {prefix}; while(flag) {{ val local = "inner"; break }}; {suffix} }}"#
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(
            owned.clone().validate().is_ok(),
            "valid boundary rejection: {text}"
        );
        assert_eq!(owned.owner_observations().len(), 1);
        let legacy = &owned.owner_observations()[0];
        assert!(legacy.invariant_break_observation_v1().is_none());
        assert!(legacy.invariant_loop_observation_v1().is_none());
        compare(&fixture, &fixture.parts(&owned));
    }
    // Independent parameters pass the old split condition gate; the nonempty Conditional
    // boundary itself must still reject rather than relying on same-parameter rejection.
    let text = r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) { break } else { break } }; val retired = held }"#;
    let fixture = Fixture::new(&[("independent-condition.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 1);
    let legacy = &owned.owner_observations()[0];
    assert!(legacy.invariant_break_observation_v1().is_none());
    assert!(legacy.invariant_loop_observation_v1().is_none());
    assert!(legacy.loop_observation_v1().is_none() && legacy.value_observation_v1().is_none());
    compare(&fixture, &fixture.parts(&owned));
}

#[test]
fn break_exit_catalogs_keep_source_analysis_and_error_identity() {
    let first = r#"fun first(flag: Boolean) { val held = "outer"; while(flag) { var root = "inner"; val old = replace(&root, "next"); break }; val retired = held }"#;
    let second = first.replace("first", "second");
    let fixture = Fixture::new(&[("a.ko", first), ("b.ko", &second)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    assert!(owned.clone().validate().is_ok() && again.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 2);
    for index in 0..2 {
        check_break(&fixture, index, &owned);
        let observed = owned.owner_observations()[index]
            .invariant_break_observation_v1()
            .unwrap();
        for foreign in [
            owned.owner_observations()[1 - index]
                .invariant_break_observation_v1()
                .unwrap(),
            again.owner_observations()[index]
                .invariant_break_observation_v1()
                .unwrap(),
        ] {
            assert!(
                observed
                    .input_producers()
                    .definition(foreign.break_boundary().unwrap().1.owner())
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
    for body in [
        r#"val local = held; break"#,
        r#"borrow val alias = held; break"#,
        r#"var root = "inner"; val old = replace(&root, held); break"#,
    ] {
        let text = format!(
            r#"fun run(flag: Boolean) {{ val held = "outer"; while(flag) {{ {body} }}; val retired = held }}"#
        );
        let fixture = Fixture::new(&[("error.ko", &text)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err(), "{text}");
        assert!(
            failed
                .owner_observations()
                .iter()
                .all(|o| o.invariant_break_observation_v1().is_none()
                    && o.invariant_loop_observation_v1().is_none())
        );
    }
    let bad = format!("{first}\nfun bad(input: String) {{ val moved = input }}");
    let fixture = Fixture::new(&[("sibling-error.ko", &bad)]);
    let failed = fixture.shared();
    assert!(failed.clone().validate().is_err());
    assert!(failed.owner_observations().is_empty());
}

// Synthetic damage guards compile locally; ordinary source evidence remains mandatory.
#[test]
fn break_exit_seal_rejects_wrong_snapshots_variants_and_deferred_products() {
    use crate::ownership_checking::compilation_unit::owner_transport::{
        InvariantLoopValueV1, LoopBodyCompletion,
    };
    let text = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner"; break }; val retired = held }"#;
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    let original = owned.owner_observations()[0]
        .invariant_break_observation_v1()
        .unwrap();
    let foreign = again.owner_observations()[0]
        .invariant_break_observation_v1()
        .unwrap();
    let mut changed = original.clone();
    changed.evidence.transfer = None;
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.evidence.transfer.as_mut().unwrap().0 =
        foreign.evidence.transfer.as_ref().unwrap().0.clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.evidence.transfer.as_mut().unwrap().1 = original.condition();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.evidence.transfer.as_mut().unwrap().2 = foreign.entry().clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.evidence.body_completion = foreign.body_completion().clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.evidence.inputs = foreign.evidence.inputs.clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.evidence.closed.edges[1] = Edge::Backedge(
        UnitDropPoint::AfterStatement(original.body()),
        original.condition(),
    );
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.evidence.closed.completion =
        LoopBodyCompletion::Continue(original.break_boundary().unwrap().0);
    assert!(!changed.is_consistent());
    assert!(original.evidence.closed.is_consistent());
    let continuation = text.replace("; break", "; continue");
    let continuation_fixture = Fixture::new(&[("continue.ko", &continuation)]);
    let continuation_owned = continuation_fixture.shared();
    let continued = continuation_owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    assert!(continued.is_consistent() && continued.evidence.closed.is_consistent());
    let wrong_wrapper = crate::ownership_checking::UnitOwnerInvariantBreakObservationV1 {
        evidence: continued.evidence.clone(),
        post_loop: None,
    };
    assert!(
        !wrong_wrapper.is_consistent(),
        "self-consistent Continue is not a Break capability"
    );
    let mut legacy = owned.owner_observations()[0].clone();
    legacy.invariant_loop_value = Some(Box::new(InvariantLoopValueV1::Backedge(
        crate::ownership_checking::UnitOwnerInvariantLoopObservationV1 {
            evidence: original.evidence.clone(),
            post_loop: None,
        },
    )));
    assert!(
        legacy.invariant_loop_observation_v1().is_none()
            && legacy.invariant_break_observation_v1().is_none()
    );
    let mut legacy = owned.owner_observations()[0].clone();
    legacy.invariant_loop_value = again.owner_observations()[0].invariant_loop_value.clone();
    assert!(legacy.invariant_break_observation_v1().is_none());
    let incomplete =
        crate::ownership_checking::compilation_unit::owner_transport::publish_observations(
            owned.owner_observations().to_vec(),
            true,
            false,
        );
    assert!(incomplete[0].invariant_break_observation_v1().is_none());
}
