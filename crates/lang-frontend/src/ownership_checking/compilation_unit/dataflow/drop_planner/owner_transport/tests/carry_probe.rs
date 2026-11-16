use super::super::carry_probe::Point;
use super::*;

const SOURCE: &str = r#"fun run(outerCount: Int, innerCount: Int): Unit {
var a = "initial a"
var b = "initial b"
var outer = outerCount
while (outer > 0) {
    var inner = innerCount
    while (inner > 0) {
        val oldChild = replace(&a, "child")
        inner = inner - 1
    }
    val oldParent = replace(&a, "parent")
    outer = outer - 1
}
println(b)
}"#;

#[test]
fn ordinary_carry_probe_reads_actual_traversal_without_changing_cleanup() {
    for text in [SOURCE.to_owned(), SOURCE.replace("println(b)", "")] {
        let fixture = Fixture::new(&[("probe.ko", &text)]);
        let owned = fixture.shared();
        let a = fixture.local(0, "a");
        let b = fixture.local(0, "b");
        let keeps_b = text.contains("println(b)");
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let make = || {
                super::super::super::DropPlanner::new(
                    checker,
                    super::super::super::liveness::build(checker, true).unwrap(),
                )
            };
            let baseline = make().run().unwrap();
            let mut planner = make();
            planner.carry_probe = Some(Vec::new());
            let observed = planner.run().unwrap();
            assert_eq!(baseline.facts, observed.facts);
            assert_eq!(
                baseline.conditional_receiver_facts,
                observed.conditional_receiver_facts
            );
            assert_eq!(baseline.owner_observations, observed.owner_observations);
            let visits = observed.carry_probe.as_ref().unwrap();
            assert_eq!(visits.len(), 6);
            let mut controls = Vec::new();
            let mut calls = Vec::new();
            for visit in visits {
                match visit.point {
                    Point::Enter(control) => {
                        assert_eq!(visit.control, Some(control));
                        controls.push(control);
                    }
                    Point::Leave(control) => {
                        assert_eq!(visit.control, Some(control));
                    }
                    Point::Primitive(call) => {
                        assert!(visit.control.is_some());
                        assert!(owned.ownership_primitive(call).is_some());
                        assert!(visit.roots.iter().any(|(root, _)| *root == a));
                        calls.push(call);
                    }
                }
                assert_eq!(visit.roots.iter().any(|(root, _)| *root == b), keeps_b);
            }
            assert_eq!(controls.len(), 2);
            assert_ne!(controls[0], controls[1]);
            assert_eq!(calls.len(), 2);
            assert_ne!(calls[0], calls[1]);
            assert!(visits.iter().any(|visit| !visit.observation_active));
            assert_eq!(
                visits.iter().map(|visit| visit.point).collect::<Vec<_>>(),
                vec![
                    Point::Enter(controls[0]),
                    Point::Enter(controls[1]),
                    Point::Primitive(calls[0]),
                    Point::Leave(controls[1]),
                    Point::Primitive(calls[1]),
                    Point::Leave(controls[0]),
                ]
            );
            assert_eq!(visits[2].control, Some(controls[1]));
            assert_eq!(visits[4].control, Some(controls[0]));
            for visit in visits {
                assert_eq!(
                    visit.status,
                    Some(
                        crate::ownership_checking::UnitOwnerObservationStatus::Unsupported {
                            point: crate::ownership_checking::UnitDropPoint::AfterStatement(
                                controls[0]
                            ),
                            reason: crate::ownership_checking::UnitOwnerUnsupportedReason::Syntax,
                        }
                    )
                );
                assert!(visit.current_owner(a).is_none());
                assert!(visit.current_owner(b).is_none());
            }
            let initial_a = visits[0]
                .roots
                .iter()
                .find(|(root, _)| *root == a)
                .unwrap()
                .1
                .as_ref()
                .unwrap();
            // The replaced root still has the same residual token after a real
            // dispatch: raw equality is NOT evidence that a root was preserved.
            for visit in &visits[..5] {
                assert_eq!(
                    visit
                        .roots
                        .iter()
                        .find(|(root, _)| *root == a)
                        .unwrap()
                        .1
                        .as_ref(),
                    Some(initial_a)
                );
            }
            assert!(!visits[5].roots.iter().any(|(root, _)| *root == a));
        });
    }
}

#[test]
fn ordinary_active_primitive_probe_retains_current_token() {
    let fixture = Fixture::new(&[(
        "active.ko",
        "fun run() { var a = \"a\"; val old = replace(&a, \"b\"); println(a) }",
    )]);
    let owned = fixture.shared();
    let a = fixture.local(0, "a");
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.carry_probe = Some(Vec::new());
        let observed = planner.run().unwrap();
        let [visit] = observed.carry_probe.as_ref().unwrap().as_slice() else {
            panic!("one actual primitive dispatch")
        };
        assert!(matches!(visit.point, Point::Primitive(_)));
        assert_eq!(visit.control, None);
        assert_eq!(
            visit.status,
            Some(crate::ownership_checking::UnitOwnerObservationStatus::Complete)
        );
        assert!(visit.current_owner(a).is_some());
    });
}
