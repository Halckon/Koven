use super::super::loop_generation::Generation;
use super::*;

#[test]
fn ordinary_loop_generation_uses_symbolic_header_not_last_static_producer() {
    for (body, accepted) in [
        ("val old = replace(&a, \"next\"); n -= 1", true),
        ("val old = replace(&a, \"next\"); break", false),
        ("val old = replace(&a, \"next\"); n += 1", false),
        ("val old = replace(&a, \"next\"); n -= 2", false),
        (
            "val old = replace(&a, \"next\"); val extra = n; n -= 1",
            false,
        ),
        (
            "val old = replace(&a, \"next\"); println(old); n -= 1",
            false,
        ),
        (
            "val old = replace(&a, \"next\"); val other = replace(&a, \"third\"); n -= 1",
            false,
        ),
    ] {
        let text = format!(
            "fun run(count: Int) {{ var a = \"first\"; var n = count; while(n > 0) {{ {body} }} }}"
        );
        let fixture = Fixture::new(&[("generation.ko", &text)]);
        let owned = fixture.shared();
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.origin_probe = Some(Vec::new());
            let observed = planner.run().unwrap();
            let candidate = observed.loop_generation_candidate();
            assert_eq!(candidate.is_some(), accepted);
            if let Some(candidate) = candidate {
                assert!(candidate.matches(checker.typed));
                assert_eq!(candidate.coverage.counter, fixture.local(0, "n"));
                assert_eq!(
                    candidate.coverage.parameter,
                    checker
                        .reference_symbol(
                            fixture.parsed[0]
                                .ast()
                                .expressions()
                                .get(fixture.expression(0, "count"))
                                .unwrap()
                                .span()
                        )
                        .unwrap()
                );
                assert_eq!(
                    candidate.coverage.callable.source_unit(),
                    checker.source_unit
                );
                assert_eq!(
                    candidate.coverage.condition,
                    checker.unit_expression(fixture.expression(0, "n > 0"))
                );
                assert_eq!(
                    candidate.coverage.step,
                    checker.unit_expression(fixture.expression(0, "n -= 1"))
                );
                assert_eq!(candidate.root, fixture.local(0, "a"));
                assert_eq!(
                    candidate.header,
                    Generation::Header {
                        control: candidate.control,
                        root: candidate.root
                    }
                );
                assert_eq!(candidate.exit, candidate.header);
                assert_ne!(candidate.exit, candidate.backedge);
                let initial = checker.unit_expression(fixture.expression(0, "\"first\""));
                let next = checker.unit_expression(fixture.expression(0, "\"next\""));
                let call = checker.unit_expression(fixture.expression(0, "replace(&a, \"next\")"));
                assert_eq!(candidate.entry, Generation::EntrySite(initial));
                assert_eq!(candidate.backedge, Generation::IterationSite(next));
                assert_eq!(
                    candidate.replacement,
                    (
                        call,
                        candidate.header,
                        candidate.backedge,
                        Generation::Detached { call }
                    )
                );
                assert_eq!(candidate.cleanup[0].0, Generation::Detached { call });
                assert_eq!(candidate.cleanup[1].0, candidate.header);
                assert_eq!(
                    candidate
                        .cleanup
                        .iter()
                        .map(|(_, f)| *f)
                        .collect::<Vec<_>>(),
                    observed
                        .facts
                        .iter()
                        .map(|f| f.into_unit(checker.source_unit))
                        .collect::<Vec<_>>()
                );
            }
        });
    }
}

#[test]
fn ordinary_two_root_loop_binds_real_retained_suffix_facts() {
    let text = "fun run(count: Int) { var a = \"first\"; var b = \"kept\"; var n = count; while(n > 0) { val old = replace(&a, \"next\"); n -= 1 }; println(b) }";
    let fixture = Fixture::new(&[("generation-two.ko", text)]);
    let owned = fixture.shared();
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.origin_probe = Some(Vec::new());
        let observed = planner.run().unwrap();
        let candidate = observed
            .loop_generation_candidate()
            .expect("closed two-root observation");
        assert!(candidate.matches(checker.typed));
        let kept = candidate.retained.expect("actual retained root");
        assert_eq!(kept.root, fixture.local(0, "b"));
        assert_eq!(
            kept.initial,
            checker.unit_expression(fixture.expression(0, "\"kept\""))
        );
        assert_eq!(
            kept.call,
            checker.unit_expression(fixture.expression(0, "println(b)"))
        );
        assert_eq!(
            kept.cleanup.target(),
            crate::ownership_checking::UnitDropTarget::Named(kept.root)
        );
        assert_eq!(
            kept.cleanup.point(),
            crate::ownership_checking::UnitDropPoint::CallReturn(kept.call)
        );
        assert_eq!(
            owned.drops(),
            &[candidate.cleanup[0].1, candidate.cleanup[1].1, kept.cleanup]
        );
        assert_eq!(owned.loans().last(), Some(&kept.loan));
    });
}

#[test]
fn ordinary_nested_trace_preserves_parent_exit_cleanup_without_single_loop_admission() {
    let text = "fun run(count: Int) { var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) { var inner = count; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1 }; println(b) }";
    let fixture = Fixture::new(&[("nested-generation.ko", text)]);
    let owned = fixture.shared();
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.origin_probe = Some(Vec::new());
        let observed = planner.run().unwrap();
        use super::super::origin_probe::{Event, WhileRole};
        let events = observed.origin_probe.as_ref().unwrap();
        let [
            Event::Literal(initial),
            Event::Bound(a, first),
            Event::Literal(kept),
            Event::Bound(b, second),
            Event::WhileTraversal {
                control: parent,
                role: WhileRole::Enter,
            },
            Event::WhileTraversal {
                control: pcond,
                role: WhileRole::ConditionEvaluated,
            },
            Event::WhileTraversal {
                control: pbody,
                role: WhileRole::BodyEnter,
            },
            Event::WhileTraversal {
                control: child,
                role: WhileRole::Enter,
            },
            Event::WhileTraversal {
                control: ccond,
                role: WhileRole::ConditionEvaluated,
            },
            Event::WhileTraversal {
                control: cbody,
                role: WhileRole::BodyEnter,
            },
            Event::BorrowRegistered(loan),
            Event::Literal(incoming),
            Event::Applied {
                call,
                root,
                incoming: applied,
            },
            Event::PendingBorrowRemoved {
                loan: ended,
                call: end_call,
            },
            Event::Bound(old, old_call),
            Event::Dropped(old_drop),
            Event::WhileTraversal {
                control: cback,
                role: WhileRole::BodyExit(Some(true)),
            },
            Event::WhileTraversal {
                control: cexit,
                role: WhileRole::Exit(Some(true)),
            },
            Event::WhileTraversal {
                control: pback,
                role: WhileRole::BodyExit(Some(true)),
            },
            Event::Dropped(root_drop),
            Event::WhileTraversal {
                control: pexit,
                role: WhileRole::Exit(Some(true)),
            },
            Event::UncoveredCall(suffix),
            Event::BorrowRegistered(shared),
            Event::PendingBorrowRemoved {
                loan: shared_end,
                call: suffix_end,
            },
            Event::Dropped(kept_drop),
        ] = events.as_slice()
        else {
            panic!("exact actual nested trace")
        };
        assert_eq!(initial, first);
        assert_eq!(kept, second);
        assert_eq!(*a, fixture.local(0, "a"));
        assert_eq!(*b, fixture.local(0, "b"));
        assert_eq!(*old, fixture.local(0, "old"));
        assert_eq!(root, a);
        assert_eq!(incoming, applied);
        assert_eq!(call, old_call);
        assert_eq!(call, end_call);
        assert_eq!(loan, ended);
        assert_eq!([pcond, pbody, pback, pexit], [parent; 4]);
        assert_eq!([ccond, cbody, cback, cexit], [child; 4]);
        assert_ne!(parent, child);
        assert_eq!(shared, shared_end);
        assert_eq!(suffix, suffix_end);
        assert_eq!(shared.call(), *suffix);
        use crate::ownership_checking::{LoanKind, UnitDropTarget, UnitLoanTarget};
        assert_eq!(old_drop.target(), UnitDropTarget::Named(*old));
        assert_eq!(root_drop.target(), UnitDropTarget::Named(*a));
        assert_eq!(kept_drop.target(), UnitDropTarget::Named(*b));
        assert_eq!(loan.kind(), LoanKind::Exclusive);
        assert_eq!(shared.kind(), LoanKind::Shared);
        assert!(matches!(loan.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root()==*a));
        assert!(matches!(shared.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root()==*b));
        for (id, expected) in [(parent, "outer > 0"), (child, "inner > 0")] {
            let crate::parser::Statement::While { condition, .. } = checker
                .parsed
                .ast()
                .statements()
                .get(id.statement())
                .unwrap()
                .payload()
            else {
                panic!("real loop")
            };
            assert_eq!(*condition, fixture.expression(0, expected));
        }
        assert_eq!(owned.drops(), &[*old_drop, *root_drop, *kept_drop]);
        assert_eq!(owned.loans(), &[loan.clone(), shared.clone()]);
        assert_eq!(
            root_drop.point(),
            crate::ownership_checking::UnitDropPoint::LoopExit(*parent)
        );
        assert_ne!(
            root_drop.point(),
            crate::ownership_checking::UnitDropPoint::LoopExit(*child)
        );
        assert_eq!(
            kept_drop.point(),
            crate::ownership_checking::UnitDropPoint::CallReturn(*suffix)
        );
        assert!(
            observed.loop_generation_candidate().is_none(),
            "single-loop contract stays closed"
        );
    });
}

#[cfg(feature = "d10-catalog-experiment")]
#[test]
fn parent_replace_child_counter_preserves_original_nested_observation() {
    let text = "fun run(count: Int) { var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) { val old = replace(&a, \"next\"); var inner = count; while(inner > 0) { inner -= 1 }; outer -= 1 }; println(b) }";
    let fixture = Fixture::new(&[("parent-generation.ko", text)]);
    let owned = fixture.shared();
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.origin_probe = Some(Vec::new());
        let observed = planner.run().unwrap();
        let pending = observed
            .nested_pending()
            .expect("closed parent Replace observation");
        assert_eq!(pending.replacement_control, pending.facts.control);
        assert_ne!(pending.child, pending.facts.control);
        assert_eq!(pending.facts.root, fixture.local(0, "a"));
        assert_eq!(
            pending.facts.cleanup[1].point(),
            crate::ownership_checking::UnitDropPoint::LoopExit(pending.facts.control)
        );
        let retained = pending.facts.retained.unwrap();
        assert_eq!(
            owned.drops(),
            &[
                pending.facts.cleanup[0],
                pending.facts.cleanup[1],
                retained.cleanup()
            ]
        );
        assert!(observed.loop_generation_candidate().is_none());
    });
}
