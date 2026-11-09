//! Real suffix traversal closes the invariant input without rewriting the old observer.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerOrigin, UnitOwnerValueEventV1 as Event,
    UnitOwnerValueV1 as Value,
};
use crate::parser::{Item, Statement};
use crate::type_checking::UnitStatementId;

const SOURCE: &str = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; val retired = held }"#;

fn check(
    fixture: &Fixture,
    index: usize,
    owned: &crate::ownership_checking::CompilationUnitOwnership,
) {
    let legacy = &owned.owner_observations()[index];
    assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
    assert!(legacy.value_observation_v1().is_none() && legacy.loop_observation_v1().is_none());
    let boundaries = legacy.invariant_loop_observation_v1().unwrap();
    assert!(boundaries.is_consistent());
    let outer = boundaries
        .post_loop_transport_v1()
        .expect("real post-loop input consumption");
    assert!(outer.is_consistent());
    assert_eq!(outer.producers().definitions().len(), 1);
    assert_eq!(outer.prefix().len(), 5);
    assert!(outer.choice().is_none() && outer.suffix().is_empty());
    let source = fixture.source(index);
    let held = fixture.local(index, "held");
    let retired = fixture.local(index, "retired");
    let initializer = UnitExpressionId::new(source, fixture.expression(index, "\"outer\""));
    let use_expression = UnitExpressionId::new(source, fixture.expression(index, "held"));
    let ast = fixture.parsed[index].ast();
    let suffix = ast
        .statements()
        .iter()
        .find_map(|(id, node)| {
            let Statement::LocalVariable { declaration } = node.payload() else {
                return None;
            };
            let Item::Variable { initializer, .. } =
                ast.items().get(*declaration).unwrap().payload()
            else {
                return None;
            };
            (*initializer == use_expression.expression())
                .then_some(UnitStatementId::new(source, id))
        })
        .unwrap();
    let (while_id, condition, body) = ast
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
    assert_eq!(
        (
            boundaries.statement(),
            boundaries.condition(),
            boundaries.body()
        ),
        (
            UnitStatementId::new(source, while_id),
            UnitExpressionId::new(source, condition),
            UnitStatementId::new(source, body)
        )
    );
    assert_eq!(
        legacy.status(),
        crate::ownership_checking::UnitOwnerObservationStatus::Unsupported {
            point: UnitDropPoint::AfterStatement(boundaries.statement()),
            reason: crate::ownership_checking::UnitOwnerUnsupportedReason::Syntax
        }
    );
    let [
        Event::Create(created_at, owner),
        Event::Bind(input, bound, Value::Definition(bound_owner)),
        Event::Move(moved_at, moved_from, Value::Definition(moved_owner)),
        Event::Bind(output, used, Value::Definition(installed)),
        Event::Drop(point, target, Value::Definition(dropped)),
    ] = outer.prefix()
    else {
        panic!("exact outer trace")
    };
    assert_eq!(
        (*created_at, *input, *bound),
        (initializer, held, initializer)
    );
    assert_eq!(
        (*moved_at, *moved_from, *output, *used),
        (use_expression, held, retired, use_expression)
    );
    assert_eq!(
        (*point, *target),
        (UnitDropPoint::AfterStatement(suffix), retired)
    );
    for id in [owner, bound_owner, moved_owner, installed, dropped] {
        assert_eq!(*id, boundaries.entry().owner());
        assert_eq!(
            outer.producers().definition(id.clone()).unwrap().origin(),
            UnitOwnerOrigin::String(initializer)
        );
        assert!(
            boundaries
                .input_producers()
                .definition(id.clone())
                .is_some()
        );
        assert!(
            boundaries
                .body_transport()
                .producers()
                .definition(id.clone())
                .is_none()
        );
    }
    for definition in boundaries.body_transport().producers().definitions() {
        assert!(outer.producers().definition(definition.id()).is_none());
    }
    assert_eq!(boundaries.entry(), boundaries.backedge());
    assert_eq!(boundaries.entry(), boundaries.exit());
    assert!(boundaries.continue_boundary().is_none());
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(retired))
            .count(),
        1
    );
    assert!(
        owned
            .drops()
            .iter()
            .any(|drop| drop.target() == UnitDropTarget::Named(retired) && drop.point() == *point)
    );
    assert!(
        !owned
            .drops()
            .iter()
            .any(|drop| drop.target() == UnitDropTarget::Named(held))
    );
}
fn assert_source(text: &str, producers: usize, replacements: usize) {
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok(), "{text}");
    check(&fixture, 0, &owned);
    let body = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap()
        .body_transport();
    assert_eq!(body.producers().definitions().len(), producers);
    assert_eq!(
        body.prefix()
            .iter()
            .filter(|e| matches!(e, Event::Replace { .. }))
            .count(),
        replacements
    );
    compare(&fixture, &fixture.parts(&owned));
}
#[test]
fn actual_suffix_closes_the_input_owner_after_normal_while() {
    // Keep the original runtime red unchanged, then vary the admitted body and condition.
    assert_source(SOURCE, 1, 0);
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
                    r#"fun run(flag: Boolean){result} {{ val held = "outer"; while({condition}) {{ {body} }}; val retired = held }}"#
                );
                if text != SOURCE {
                    assert_source(&text, producers, replacements);
                }
            }
        }
    }
}

#[test]
fn suffix_admission_preserves_prior_control_capabilities() {
    for jump in ["continue", "break"] {
        let text = SOURCE.replace(
            "val local = \"inner\"",
            &format!("val local = \"inner\"; {jump}"),
        );
        let fixture = Fixture::new(&[("jump.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let observed = &owned.owner_observations()[0];
        if jump == "continue" {
            let original = observed.invariant_loop_observation_v1().unwrap();
            assert!(original.continue_boundary().is_some());
            assert!(original.post_loop_transport_v1().is_none());
        } else {
            assert!(observed.invariant_break_observation_v1().is_some());
        }
        compare(&fixture, &fixture.parts(&owned));
    }
    for (left, right) in [
        ("", ""),
        ("continue", "continue"),
        ("continue", "break"),
        ("break", "continue"),
        ("break", "break"),
        ("", "continue"),
        ("continue", ""),
        ("", "break"),
        ("break", ""),
    ] {
        let text = format!(
            r#"fun run(flag: Boolean, other: Boolean) {{ val held = "outer"; while(flag) {{ val local = "inner"; if(other) {{ {left} }} else {{ {right} }} }}; val retired = held }}"#
        );
        let fixture = Fixture::new(&[("control.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let original = &owned.owner_observations()[0];
        assert!(original.invariant_loop_observation_v1().is_none());
        assert!(
            original.invariant_normal_join_observation_v1().is_some()
                || original.invariant_mixed_join_observation_v1().is_some()
                || original.invariant_split_continue_observation_v1().is_some()
                || original.invariant_split_exit_observation_v1().is_some()
        );
        compare(&fixture, &fixture.parts(&owned));
    }
    for text in [
        SOURCE.replace("val held", "var held"),
        SOURCE.replace("val retired", "var retired"),
        SOURCE.replace("= held", "= (held)"),
        SOURCE.replace("= \"outer\"", "= (\"outer\")"),
        SOURCE.replace("val held =", "val held: String? ="),
        SOURCE.replace("val retired =", "val retired: String? ="),
        SOURCE.replace("val retired = held", "val retired = \"other\""),
        SOURCE.replace(
            "val retired = held",
            "val retired = held; val extra = \"extra\"",
        ),
        SOURCE.replace("val local = \"inner\"", ""),
        SOURCE.replace("val local = \"inner\"", "val local = held + \"inner\""),
        SOURCE.replace("while(flag)", "while(false)"),
        SOURCE.replace("val local = \"inner\"", "val local: String? = \"inner\""),
    ] {
        let fixture = Fixture::new(&[("negative.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{text}");
        assert_eq!(owned.owner_observations().len(), 1);
        assert!(
            owned.owner_observations().iter().all(|o| o
                .invariant_loop_observation_v1()
                .is_none_or(|b| b.post_loop_transport_v1().is_none())),
            "{text}"
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn suffix_sources_and_failed_analyses_do_not_leak_transport() {
    let second = SOURCE.replace("run", "second");
    let fixture = Fixture::new(&[("a.ko", SOURCE), ("b.ko", &second)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    assert!(owned.clone().validate().is_ok() && again.clone().validate().is_ok());
    for index in 0..2 {
        check(&fixture, index, &owned);
        let original = owned.owner_observations()[index]
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
                original
                    .post_loop_transport_v1()
                    .unwrap()
                    .producers()
                    .definition(foreign.entry().owner())
                    .is_none()
            );
            assert!(
                foreign
                    .post_loop_transport_v1()
                    .unwrap()
                    .producers()
                    .definition(original.entry().owner())
                    .is_none()
            );
        }
    }
    for text in [
        SOURCE.replace(
            "val retired = held",
            "val retired = held; val duplicate = held",
        ),
        SOURCE.replace("val local = \"inner\"", "val local = held"),
        format!("{SOURCE}\nfun bad(input: String) {{ val moved = input }}"),
    ] {
        let fixture = Fixture::new(&[("failed.ko", &text)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err(), "{text}");
        assert!(failed.owner_observations().is_empty());
    }
}

// Deliberately damaged seals compile locally; execute only in formal CI.
#[test]
fn suffix_seal_rejects_foreign_or_incomplete_consumption() {
    let second = SOURCE.replace("run", "second");
    let fixture = Fixture::new(&[("a.ko", SOURCE), ("b.ko", &second)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    let original = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    for foreign in [
        again.owner_observations()[0]
            .invariant_loop_observation_v1()
            .unwrap(),
        owned.owner_observations()[1]
            .invariant_loop_observation_v1()
            .unwrap(),
    ] {
        let mut damaged = original.clone();
        damaged.post_loop = foreign.post_loop.clone();
        assert!(damaged.is_consistent());
        assert!(damaged.post_loop_transport_v1().is_none());
    }
    for mutation in 0..8 {
        let mut damaged = original.clone();
        let suffix = damaged.post_loop.as_mut().unwrap();
        match mutation {
            0 => {
                suffix.transport.prefix.pop();
            }
            1 => suffix
                .transport
                .prefix
                .push(suffix.transport.prefix[4].clone()),
            2 => suffix.transport.prefix.swap(2, 3),
            3 => suffix.statement = original.body(),
            4 => suffix.expression = original.condition(),
            5 => suffix.retired = original.entry().symbol(),
            6 => {
                suffix.entry = owned.owner_observations()[1]
                    .invariant_loop_observation_v1()
                    .unwrap()
                    .entry()
                    .clone()
            }
            _ => {
                if let Event::Drop(point, ..) = &mut suffix.transport.prefix[4] {
                    *point = UnitDropPoint::AfterStatement(original.body());
                }
                assert!(
                    suffix.transport.is_consistent(),
                    "only the new exact suffix seal rejects this valid generic drop"
                );
            }
        }
        assert!(damaged.is_consistent());
        assert!(damaged.post_loop_transport_v1().is_none());
    }
    let incomplete =
        crate::ownership_checking::compilation_unit::owner_transport::publish_observations(
            owned.owner_observations().to_vec(),
            true,
            false,
        );
    assert!(
        incomplete
            .iter()
            .all(|o| o.invariant_loop_observation_v1().is_none())
    );
    let failed = crate::ownership_checking::compilation_unit::owner_transport::publish_observations(
        owned.owner_observations().to_vec(),
        false,
        true,
    );
    assert!(failed.is_empty());
    // Keep the original catalog/graph identities; change only the control evidence
    // to a self-consistent Continue using a real same-source helper jump.
    let continued_source = SOURCE
        .replace("run", "continued")
        .replace("val local = \"inner\"", "val local = \"inner\"; continue");
    let text = format!("{SOURCE}\n{continued_source}");
    let fixture = Fixture::new(&[("control.ko", &text)]);
    let owned = fixture.shared();
    let original = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    let continued = owned.owner_observations()[1]
        .invariant_loop_observation_v1()
        .unwrap();
    let (jump, _) = continued.continue_boundary().unwrap();
    let mut damaged = original.clone();
    let completion =
        crate::ownership_checking::compilation_unit::owner_transport::LoopBodyCompletion::Continue(
            jump,
        );
    damaged.evidence.closed.completion = completion;
    damaged.evidence.closed.edges =
        completion.edges(damaged.statement(), damaged.condition(), damaged.body());
    damaged.evidence.transfer = Some((
        damaged.evidence.closed.body_identity.clone(),
        jump,
        damaged.entry().clone(),
    ));
    assert!(damaged.is_consistent());
    assert!(
        damaged
            .post_loop
            .as_ref()
            .unwrap()
            .transport
            .is_consistent()
    );
    assert!(damaged.post_loop_transport_v1().is_none());
}
