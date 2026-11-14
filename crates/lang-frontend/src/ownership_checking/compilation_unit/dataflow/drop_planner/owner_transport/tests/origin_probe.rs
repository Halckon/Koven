use super::super::origin_probe::Event;
use super::*;

#[test]
fn ordinary_literal_binding_and_applied_replace_trace_has_no_legacy_token_dependency() {
    let source = r#"fun run(count: Int) {
var a = "initial"
var b = "kept"
var counter = count
while (counter > 0) {
val old = replace(&a, "next")
counter = counter - 1
}
println(b)
}"#;
    let fixture = Fixture::new(&[("origins.ko", source)]);
    let owned = fixture.shared();
    let a = fixture.local(0, "a");
    let b = fixture.local(0, "b");
    let old = fixture.local(0, "old");
    let expression = |text| {
        crate::type_checking::UnitExpressionId::new(fixture.source(0), fixture.expression(0, text))
    };
    let initial = expression("\"initial\"");
    let kept = expression("\"kept\"");
    let next = expression("\"next\"");
    let call = expression("replace(&a, \"next\")");
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let make = || {
            super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            )
        };
        let baseline = make().run().unwrap();
        let mut planner = make();
        planner.origin_probe = Some(Vec::new());
        let observed = planner.run().unwrap();
        assert_eq!(baseline.facts, observed.facts);
        assert_eq!(
            baseline.conditional_receiver_facts,
            observed.conditional_receiver_facts
        );
        assert_eq!(baseline.owner_observations, observed.owner_observations);
        assert_eq!(
            observed
                .origin_probe
                .as_ref()
                .unwrap()
                .iter()
                .filter_map(|event| match event {
                    Event::Dropped(fact) => Some(*fact),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            baseline
                .facts
                .iter()
                .map(|fact| fact.into_unit(fixture.source(0)))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            observed
                .origin_probe
                .as_ref()
                .unwrap()
                .iter()
                .filter(|event| !matches!(event, Event::Dropped(_) | Event::UncoveredCall(_)))
                .cloned()
                .collect::<Vec<_>>(),
            vec![
                Event::Literal(initial),
                Event::Bound(a, initial),
                Event::Literal(kept),
                Event::Bound(b, kept),
                Event::Literal(next),
                Event::Applied {
                    call,
                    root: a,
                    incoming: next
                },
                Event::Bound(old, call),
            ]
        );
    });
}

#[test]
fn ordinary_move_and_drop_trace_records_actual_removal_and_publication() {
    let fixture = Fixture::new(&[("move.ko", "fun run() { val a = \"source\"; val b = a }")]);
    let owned = fixture.shared();
    let a = fixture.local(0, "a");
    let b = fixture.local(0, "b");
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.origin_probe = Some(Vec::new());
        let observed = planner.run().unwrap();
        let events = observed.origin_probe.unwrap();
        let [
            Event::Literal(literal),
            Event::Bound(first, initial),
            Event::Moved { expression, source },
            Event::Bound(second, value),
            Event::Dropped(drop),
        ] = events.as_slice()
        else {
            panic!("exact real move/bind/drop sequence: {events:?}")
        };
        assert_eq!(*first, a);
        assert_eq!(*source, a);
        assert_eq!(*second, b);
        assert_eq!(literal, initial);
        assert_eq!(expression, value);
        assert_eq!(
            drop.target(),
            crate::ownership_checking::UnitDropTarget::Named(b)
        );
        assert_eq!(owned.drops(), &[*drop]);
    });
}

#[test]
fn ordinary_branch_roles_keep_alternatives_separate_from_join_inputs() {
    use super::super::origin_probe::BranchRole;
    for (body, continues, inputs) in [
        (
            "if (flag) { val a = \"left\" } else { val b = \"right\" }",
            [true, true],
            2,
        ),
        (
            "if (flag) { return } else { val b = \"right\" }",
            [false, true],
            1,
        ),
        ("if (flag) { return } else { return }", [false, false], 0),
        ("if (flag) { val a = \"left\" }", [true, true], 2),
    ] {
        let text = format!("fun run(flag: Boolean) {{ {body} }}");
        let fixture = Fixture::new(&[("branches.ko", &text)]);
        let owned = fixture.shared();
        let control = crate::type_checking::UnitExpressionId::new(
            fixture.source(0),
            fixture.expression(0, body),
        );
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let make = || {
                super::super::super::DropPlanner::new(
                    checker,
                    super::super::super::liveness::build(checker, true).unwrap(),
                )
            };
            let baseline = make().run().unwrap();
            let mut planner = make();
            planner.origin_probe = Some(Vec::new());
            let observed = planner.run().unwrap();
            assert_eq!(baseline.facts, observed.facts);
            assert_eq!(baseline.owner_observations, observed.owner_observations);
            let events = observed.origin_probe.unwrap();
            let partition = super::super::origin_probe::partition_if(&events).unwrap();
            assert_eq!(partition.continues, continues);
            for arm in partition.arms {
                for event in arm {
                    if let Event::Bound(root, _) = event {
                        assert_eq!(arm.iter().filter(|other| matches!(other, Event::Dropped(drop) if drop.target() == crate::ownership_checking::UnitDropTarget::Named(*root))).count(), 1);
                    }
                }
            }
            let branches = events
                .into_iter()
                .filter(|event| matches!(event, Event::Branch { .. }))
                .collect::<Vec<_>>();
            assert_eq!(
                branches,
                vec![
                    Event::Branch {
                        control,
                        role: BranchRole::Enter(0)
                    },
                    Event::Branch {
                        control,
                        role: BranchRole::Exit(0, continues[0])
                    },
                    Event::Branch {
                        control,
                        role: BranchRole::Enter(1)
                    },
                    Event::Branch {
                        control,
                        role: BranchRole::Exit(1, continues[1])
                    },
                    Event::Branch {
                        control,
                        role: BranchRole::JoinInputs(inputs)
                    },
                ]
            );
        });
    }
}

#[test]
fn ordinary_partition_rejects_uncovered_call_and_nested_control() {
    for body in [
        r#"if (flag) { val a = "read"; println(a) } else { val b = "other"; println(b) }"#,
        "if (flag) { if (flag) { val x = \"x\" } } else { val y = \"y\" }",
    ] {
        let text = format!("fun run(flag: Boolean) {{ {body} }}");
        let fixture = Fixture::new(&[("gap.ko", &text)]);
        let owned = fixture.shared();
        if body.contains("println") {
            assert!(
                owned
                    .loans()
                    .iter()
                    .any(|loan| loan.kind() == crate::ownership_checking::LoanKind::Shared)
            );
        }
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.origin_probe = Some(Vec::new());
            let observed = planner.run().unwrap();
            if body.contains("println") {
                let call = crate::type_checking::UnitExpressionId::new(
                    fixture.source(0),
                    fixture.expression(0, "println(a)"),
                );
                assert!(
                    observed
                        .origin_probe
                        .as_ref()
                        .unwrap()
                        .contains(&Event::UncoveredCall(call))
                );
            }
            assert!(
                super::super::origin_probe::partition_if(observed.origin_probe.as_ref().unwrap())
                    .is_none()
            );
        });
    }
}
