//! A real Continue must independently witness its surviving input owner.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerValueEventV1 as Event,
};

fn assert_continues(text: &str, producers: usize, replacements: usize) {
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok(), "{text}");
    assert_eq!(owned.owner_observations().len(), 1);
    let jump = fixture.expression(0, "continue");
    super::invariant_while::check(&fixture, 0, &owned, Some(jump));
    let observed = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
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
            .all(|edge| !matches!(edge, Edge::Backedge(..) | Edge::BreakExit(..)))
    );
    compare(&fixture, &fixture.parts(&owned));
}

#[test]
fn actual_continue_retains_the_nonempty_input_owner() {
    // Preserve the exact former §152 legal Continue rejection as a new behavior witness.
    let former = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner"; continue }; val retired = held }"#;
    assert_continues(former, 1, 0);
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
                    r#"fun run(flag: Boolean){result} {{ val held = "outer"; while ({condition}) {{ {body}; continue }}; val retired = held }}"#
                );
                assert_continues(&text, producers, replacements);
            }
        }
    }
}

#[test]
fn invariant_continue_rejects_unwitnessed_control_and_open_inputs() {
    for body in [
        r#"val local = "inner"; break"#,
        r#"val local = "inner"; (continue)"#,
        r#"val local = "inner"; continue; val tail = "tail""#,
        r#"val local = "inner"; return"#,
        r#"val local = "inner"; if(flag) { continue } else { continue }"#,
        r#"val local = "inner"; if(flag) { } else { continue }"#,
        r#"val local = "inner"; while(flag) { continue }; continue"#,
        r#"val local = "inner"; { continue }"#,
        r#"continue"#,
        r#"val local = held + "inner"; continue"#,
        r#"val local: String? = "inner"; continue"#,
        r#"val local = "inner"; "temporary"; continue"#,
    ] {
        let text = format!(
            r#"fun run(flag: Boolean) {{ val held = "outer"; while(flag) {{ {body} }}; val retired = held }}"#
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "valid negative: {text}");
        assert_eq!(owned.owner_observations().len(), 1);
        let legacy = &owned.owner_observations()[0];
        assert!(legacy.invariant_loop_observation_v1().is_none(), "{text}");
        assert!(legacy.value_observation_v1().is_none() && legacy.loop_observation_v1().is_none());
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn invariant_continue_keeps_sources_analyses_and_failures_separate() {
    let first = r#"fun first(flag: Boolean) { val held = "outer"; while(flag) { var root = "inner"; val old = replace(&root, "next"); continue }; val retired = held }"#;
    let second = first.replace("first", "second");
    let fixture = Fixture::new(&[("a.ko", first), ("b.ko", &second)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    assert!(owned.clone().validate().is_ok() && again.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 2);
    for index in 0..2 {
        super::invariant_while::check(
            &fixture,
            index,
            &owned,
            Some(fixture.expression(index, "continue")),
        );
        let observed = owned.owner_observations()[index]
            .invariant_loop_observation_v1()
            .unwrap();
        for foreign in [
            owned.owner_observations()[1 - index]
                .invariant_loop_observation_v1()
                .unwrap(),
            again.owner_observations()[index]
                .invariant_loop_observation_v1()
                .unwrap(),
        ] {
            let (_, binding) = foreign.continue_boundary().unwrap();
            assert!(
                observed
                    .input_producers()
                    .definition(binding.owner())
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
        r#"val local = held; continue"#,
        r#"borrow val alias = held; continue"#,
        r#"var root = "inner"; val old = replace(&root, held); continue"#,
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
                .all(|o| o.invariant_loop_observation_v1().is_none())
        );
    }
    let bad = format!("{first}\nfun bad(input: String) {{ val moved = input }}");
    let fixture = Fixture::new(&[("sibling-error.ko", &bad)]);
    let failed = fixture.shared();
    assert!(failed.clone().validate().is_err());
    assert!(failed.owner_observations().is_empty());
}

// Synthetic damage guards are compiled locally, not substitutes for ordinary source tests.
#[test]
fn invariant_continue_seal_requires_its_real_jump_snapshot() {
    let text = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner"; continue }; val retired = held }"#;
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    let original = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    let foreign = again.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    let mut changed = original.clone();
    changed.evidence.transfer = None;
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.evidence.transfer = foreign.evidence.transfer.clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.evidence.transfer.as_mut().unwrap().0 =
        foreign.evidence.transfer.as_ref().unwrap().0.clone();
    assert!(
        !changed.is_consistent(),
        "only the fresh body identity changed"
    );
    let mut changed = original.clone();
    changed.evidence.transfer.as_mut().unwrap().1 = original.condition();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.evidence.transfer.as_mut().unwrap().2 = foreign.entry().clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.evidence.closed.completion =
        crate::ownership_checking::compilation_unit::owner_transport::LoopBodyCompletion::Break(
            original.continue_boundary().unwrap().0,
        );
    assert!(!changed.is_consistent());
    let normal = text.replace("; continue", "");
    let normal_fixture = Fixture::new(&[("normal.ko", &normal)]);
    let normal_owned = normal_fixture.shared();
    let mut changed = normal_owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap()
        .clone();
    changed.evidence.transfer = original.evidence.transfer.clone();
    assert!(!changed.is_consistent());
    let incomplete =
        crate::ownership_checking::compilation_unit::owner_transport::publish_observations(
            owned.owner_observations().to_vec(),
            true,
            false,
        );
    assert!(incomplete[0].invariant_loop_observation_v1().is_none());
}
