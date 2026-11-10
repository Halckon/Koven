//! A real Continue backedge does not replace the condition-false suffix consumer.
use super::*;

const SOURCE: &str = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner"; continue }; val retired = held }"#;

#[test]
fn actual_continue_suffix_consumes_only_after_condition_false_exit() {
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let jump = fixture.expression(0, "continue");
    super::invariant_while::check(&fixture, 0, &owned, Some(jump));
    let boundaries = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    assert_eq!(
        boundaries.continue_boundary(),
        Some((
            UnitExpressionId::new(fixture.source(0), jump),
            boundaries.entry()
        ))
    );
    let graph = boundaries
        .post_loop_transport_v1()
        .expect("actual suffix consumes the unchanged input after condition-false exit");
    assert!(graph.is_consistent());
    assert_eq!(graph.prefix().len(), 5);
    assert_eq!(graph.producers().definitions().len(), 1);
    super::invariant_suffix::check(
        &fixture,
        0,
        &owned,
        Some(UnitExpressionId::new(fixture.source(0), jump)),
    );
    compare(&fixture, &fixture.parts(&owned));
}

#[test]
fn continue_suffix_preserves_real_sources_and_closed_boundaries() {
    for condition in ["flag", "(flag)", "!flag", "(!flag)", "!(!flag)"] {
        for result in ["", ": Unit"] {
            for body in [
                r#"val local = "inner""#,
                r#"val local = "inner"; val alias = (local)"#,
                r#"var root = "old"; val incoming = "next"; val old = replace(&root, incoming)"#,
                r#"var left = "left"; var right = "right"; val old = replace(&left, replace(&right, "next"))"#,
            ] {
                let text = format!(
                    r#"fun run(flag: Boolean){result} {{ val held = "outer"; while({condition}) {{ {body}; continue }}; val retired = held }}"#
                );
                let fixture = Fixture::new(&[("root.ko", &text)]);
                let owned = fixture.shared();
                assert!(owned.clone().validate().is_ok());
                let jump = fixture.expression(0, "continue");
                super::invariant_while::check(&fixture, 0, &owned, Some(jump));
                super::invariant_suffix::check(
                    &fixture,
                    0,
                    &owned,
                    Some(UnitExpressionId::new(fixture.source(0), jump)),
                );
                compare(&fixture, &fixture.parts(&owned));
            }
        }
    }
    let second = SOURCE.replace("run", "second");
    let fixture = Fixture::new(&[("a.ko", SOURCE), ("b.ko", &second)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    assert!(owned.clone().validate().is_ok() && again.clone().validate().is_ok());
    for index in 0..2 {
        let local = owned.owner_observations()[index]
            .invariant_loop_observation_v1()
            .unwrap();
        super::invariant_suffix::check(
            &fixture,
            index,
            &owned,
            Some(UnitExpressionId::new(
                fixture.source(index),
                fixture.expression(index, "continue"),
            )),
        );
        for foreign in [
            owned.owner_observations()[1 - index]
                .invariant_loop_observation_v1()
                .unwrap(),
            again.owner_observations()[index]
                .invariant_loop_observation_v1()
                .unwrap(),
        ] {
            assert!(
                local
                    .post_loop_transport_v1()
                    .unwrap()
                    .producers()
                    .definition(foreign.entry().owner())
                    .is_none()
            );
        }
    }
}

#[test]
fn continue_suffix_retains_exact_syntax_and_failure_rejections() {
    for text in [
        SOURCE.replace("val held", "var held"),
        SOURCE.replace("val retired", "var retired"),
        SOURCE.replace("= held", "= (held)"),
        SOURCE.replace("= \"outer\"", "= (\"outer\")"),
        SOURCE.replace("val held =", "val held: String? ="),
        SOURCE.replace("val retired =", "val retired: String? ="),
        SOURCE.replace(
            "val retired = held",
            "val retired = held; val extra = \"extra\"",
        ),
        SOURCE.replace("val local = \"inner\"; ", ""),
        SOURCE.replace("while(flag)", "while(false)"),
        SOURCE.replace("; continue", "; (continue)"),
        SOURCE.replace("; continue", "; continue; val dead = \"dead\""),
        SOURCE.replace("; continue", "; while(flag) { continue }; continue"),
    ] {
        let fixture = Fixture::new(&[("negative.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{text}");
        assert!(
            owned.owner_observations()[0]
                .invariant_loop_observation_v1()
                .is_none_or(|b| b.post_loop_transport_v1().is_none()),
            "{text}"
        );
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
        let text = SOURCE
            .replace("flag: Boolean", "flag: Boolean, other: Boolean")
            .replace(
                "; continue",
                &format!("; if(other) {{ {left} }} else {{ {right} }}"),
            );
        let fixture = Fixture::new(&[("conditional.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let observed = &owned.owner_observations()[0];
        assert!(observed.invariant_loop_observation_v1().is_none());
        assert!(
            observed.invariant_normal_join_observation_v1().is_some()
                || observed.invariant_mixed_join_observation_v1().is_some()
                || observed.invariant_split_continue_observation_v1().is_some()
                || observed.invariant_split_exit_observation_v1().is_some()
        );
        compare(&fixture, &fixture.parts(&owned));
    }
    for text in [
        SOURCE.replace("val local = \"inner\"", "val local = held"),
        format!("{SOURCE}\nfun bad(input: String) {{ val moved = input }}"),
    ] {
        let fixture = Fixture::new(&[("failed.ko", &text)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err());
        assert!(failed.owner_observations().is_empty());
    }
}

// Deliberately damaged seals compile locally; execute only in formal CI.
#[test]
fn continue_suffix_seal_rejects_control_relabeling_and_foreign_body_identity() {
    use crate::ownership_checking::compilation_unit::owner_transport::LoopBodyCompletion as Completion;
    let second = SOURCE.replace("run", "second");
    let broken = SOURCE.replace("run", "broken").replace("continue", "break");
    let fixture = Fixture::new(&[("a.ko", &format!("{SOURCE}\n{second}\n{broken}"))]);
    let owned = fixture.shared();
    let again = fixture.shared();
    assert!(owned.clone().validate().is_ok() && again.clone().validate().is_ok());
    let original = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    let helper = owned.owner_observations()[1]
        .invariant_loop_observation_v1()
        .unwrap();
    let foreign = again.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    let (jump, _) = helper.continue_boundary().unwrap();
    for completion in [Completion::Fallthrough, Completion::Continue(jump)] {
        let mut damaged = original.clone();
        damaged.evidence.closed.completion = completion;
        damaged.evidence.closed.edges =
            completion.edges(damaged.statement(), damaged.condition(), damaged.body());
        damaged.evidence.transfer = match completion {
            Completion::Continue(jump) => Some((
                damaged.evidence.closed.body_identity.clone(),
                jump,
                damaged.entry().clone(),
            )),
            _ => None,
        };
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
    let broken = owned.owner_observations()[2]
        .invariant_break_observation_v1()
        .unwrap();
    let (jump, _) = broken.break_boundary().unwrap();
    let mut relabeled = crate::ownership_checking::UnitOwnerInvariantBreakObservationV1 {
        evidence: original.evidence.clone(),
        post_loop: original.post_loop.clone(),
    };
    let completion = Completion::Break(jump);
    relabeled.evidence.closed.completion = completion;
    relabeled.evidence.closed.edges = completion.edges(
        relabeled.statement(),
        relabeled.condition(),
        relabeled.body(),
    );
    relabeled.evidence.transfer = Some((
        relabeled.evidence.closed.body_identity.clone(),
        jump,
        relabeled.entry().clone(),
    ));
    assert!(relabeled.is_consistent());
    assert!(
        relabeled
            .post_loop
            .as_ref()
            .unwrap()
            .transport
            .is_consistent()
    );
    assert!(relabeled.post_loop_transport_v1().is_none());
    let mut damaged = original.clone();
    damaged.evidence.closed.body_identity = foreign.evidence.closed.body_identity.clone();
    damaged.evidence.closed.transport = foreign.evidence.closed.transport.clone();
    damaged.evidence.transfer.as_mut().unwrap().0 = foreign.evidence.closed.body_identity.clone();
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
    let mut damaged = original.clone();
    damaged.evidence.transfer = None;
    assert!(!damaged.is_consistent() && damaged.post_loop_transport_v1().is_none());
}
