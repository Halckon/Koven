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
