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
        });
    }
}
