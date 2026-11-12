//! A local owner and actual FE loop lineage must lower without synthesizing a new loan per iteration.
use super::*;
use lang_frontend::ownership_checking::{
    UnitLoanActionCoverage, check_compilation_unit_ownership, legacy_ownership_eq,
    with_loan_observation,
};

#[test]
fn ordinary_local_loop_lineage_requires_actual_lowering() {
    for count in [1, 2, 3, 8, 17] {
        let consumers = (0..count)
            .map(|i| {
                if i % 2 == 0 {
                    "consume(item)"
                } else {
                    "consume_other(item)"
                }
            })
            .collect::<Vec<_>>()
            .join(";");
        let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun consume_other(item: String): Unit {}\nfun run(flag: Boolean): Unit { val source=\"value\"; borrow val item=view(source); while(flag){BODY} }".replace("BODY",&consumers);
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "local-loop.ko", &text);
        let (environment, types) = standard_environments();
        let mut failures = Vec::new();
        for (layout, input) in [
            (
                "rooted",
                SourceUnitInput::new("root", "local-loop.ko", source, &parsed),
            ),
            ("memory", SourceUnitInput::memory_document(source, &parsed)),
        ] {
            let inputs = [input];
            let (names, typed, on) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .validate()
                    .unwrap()
            });
            assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
            let catalogs = on.ownership().loan_action_observations();
            let lineages = catalogs
                .iter()
                .flat_map(|catalog| catalog.loop_invariant_lineages())
                .collect::<Vec<_>>();
            assert_eq!(lineages.len(), 1);
            let lineage = lineages[0];
            let catalog = catalogs
                .iter()
                .find(|catalog| catalog.owns_loop_invariant_lineage(lineage))
                .unwrap();
            assert_eq!(lineage.coverage(), UnitLoanActionCoverage::Partial);
            assert_eq!(lineage.uses().len(), count);
            for returned in lineage.uses() {
                assert!(
                    matches!(catalog.action(&returned.begin()), Some(lang_frontend::ownership_checking::UnitLoanAction::Begin(loan)) if loan == returned.loan())
                );
                assert_eq!(
                    returned.return_point(),
                    lang_frontend::ownership_checking::UnitDropPoint::CallReturn(
                        returned.loan().call()
                    )
                );
            }
            for pair in lineage.uses().windows(2) {
                assert_ne!(pair[0].begin(), pair[1].begin());
            }
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let mut previous = None;
            for (setting, owned) in [("on", &on), ("off", &off)] {
                let ((result, verified), receipts) =
                    crate::ssa::refine::loop_receipt::observe(|| {
                        crate::ssa::unit_lower::observe_final_verifier(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        })
                    });
                match result {
                    Ok((program, _)) => {
                        assert_eq!(verified.len(), 1);
                        verify_program(&program).unwrap();
                        let run = program
                            .modules
                            .iter()
                            .flat_map(|m| &m.functions)
                            .find(|f| f.name.contains(".run."))
                            .unwrap();
                        assert_eq!(
                            run.instructions
                                .iter()
                                .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                                .count(),
                            1
                        );
                        assert_eq!(
                            run.instructions
                                .iter()
                                .filter(|i| matches!(i.operation, Operation::BorrowBegin { .. }))
                                .count(),
                            1,
                            "acquired once, not per iteration or consumer"
                        );
                        assert_eq!(
                            run.instructions
                                .iter()
                                .filter(|i| matches!(i.operation, Operation::RootPlace { .. }))
                                .count(),
                            1
                        );
                        assert_eq!(receipts.len(), usize::from(setting == "on"));
                        if let Some(receipt) = receipts.first() {
                            use crate::ssa::refine::loop_receipt::Tag;
                            assert_eq!(receipt.caller, run.id);
                            assert!(catalog.owns_loop_invariant_lineage(&receipt.lineage));
                            assert_eq!(&receipt.lineage, lineage);
                            let expected = [
                                Tag::Owner,
                                Tag::Place,
                                Tag::Source,
                                Tag::Result,
                                Tag::Condition,
                            ]
                            .into_iter()
                            .chain((0..count).map(Tag::Consumer))
                            .chain([Tag::ResultEnd, Tag::SourceEnd, Tag::OwnerDrop])
                            .collect::<Vec<_>>();
                            assert_eq!(receipt.instructions.len(), expected.len());
                            for tag in expected {
                                let selected = receipt
                                    .instructions
                                    .iter()
                                    .filter(|(actual, _)| *actual == tag)
                                    .collect::<Vec<_>>();
                                assert_eq!(selected.len(), 1);
                                let instruction = run.instruction(selected[0].1).unwrap();
                                match tag {
                                    Tag::Consumer(index) => {
                                        assert_eq!(
                                            instruction.origin.span(),
                                            lineage.uses()[index].loan().end_span()
                                        );
                                        assert!(matches!(
                                            instruction.operation,
                                            Operation::DirectCall { .. }
                                        ));
                                    }
                                    Tag::Owner => assert!(matches!(
                                        instruction.operation,
                                        Operation::StringLiteral { .. }
                                    )),
                                    Tag::Place => assert!(matches!(
                                        instruction.operation,
                                        Operation::RootPlace { .. }
                                    )),
                                    Tag::Source => assert!(matches!(
                                        instruction.operation,
                                        Operation::BorrowBegin { .. }
                                    )),
                                    Tag::Result => assert!(matches!(
                                        instruction.operation,
                                        Operation::BorrowCall { .. }
                                    )),
                                    Tag::Condition => assert!(matches!(
                                        instruction.operation,
                                        Operation::Read { .. }
                                    )),
                                    Tag::ResultEnd | Tag::SourceEnd => assert!(matches!(
                                        instruction.operation,
                                        Operation::BorrowEnd { .. }
                                    )),
                                    Tag::OwnerDrop => assert!(matches!(
                                        instruction.operation,
                                        Operation::Drop { .. }
                                    )),
                                }
                            }
                            let ids = receipt
                                .instructions
                                .iter()
                                .map(|(_, id)| *id)
                                .collect::<std::collections::BTreeSet<_>>();
                            assert_eq!(ids.len(), run.instructions.len());
                            assert!(run.instructions.iter().all(|i| ids.contains(&i.id)));
                            let actual_edges = run
                                .blocks
                                .iter()
                                .flat_map(|b| match &b.terminator.as_ref().unwrap().kind {
                                    TerminatorKind::Branch(edge) => vec![(b.id, edge.clone())],
                                    TerminatorKind::Conditional {
                                        when_true,
                                        when_false,
                                        ..
                                    } => {
                                        vec![(b.id, when_true.clone()), (b.id, when_false.clone())]
                                    }
                                    TerminatorKind::Return { .. } => vec![],
                                    _ => panic!("unexpected loop terminator"),
                                })
                                .collect::<Vec<_>>();
                            assert_eq!(actual_edges.len(), 4);
                            assert_eq!(receipt.edges.len(), actual_edges.len());
                            assert!(actual_edges.iter().all(|e| {
                                receipt.edges.iter().filter(|actual| *actual == e).count() == 1
                            }));
                            for block in &run.blocks {
                                assert_eq!(
                                    receipt.entities.get(&block.id),
                                    Some(&block.parameters)
                                );
                                assert_eq!(
                                    receipt.parameter_count(block.id),
                                    Some(block.parameters.len())
                                );
                            }
                        }
                        let calls = run
                            .instructions
                            .iter()
                            .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                            .collect::<Vec<_>>();
                        assert_eq!(calls.len(), count);
                        for (index, call) in calls.iter().enumerate() {
                            let Operation::DirectCall { callee, .. } = call.operation else {
                                unreachable!()
                            };
                            let target = program
                                .modules
                                .iter()
                                .flat_map(|m| &m.functions)
                                .find(|f| f.id == callee)
                                .unwrap();
                            assert!(target.name.contains(if index % 2 == 0 {
                                ".consume."
                            } else {
                                ".consume_other."
                            }));
                            assert_eq!(call.block, calls[0].block);
                            assert_eq!(call.origin.span(), lineage.uses()[index].loan().end_span());
                        }
                        super::invariant_outer_while_tests::assert_transport(
                            run, true, "flag", None, 0,
                        );
                        let rendered = format!(
                            "{verified:?}\n{}",
                            crate::llvm::render_verified_program(&program).unwrap()
                        );
                        if let Some(previous) = previous.as_ref() {
                            assert_eq!(&rendered, previous);
                        }
                        previous = Some(rendered);
                    }
                    Err(error) => {
                        assert!(verified.is_empty());
                        failures.push(format!("{layout}/{setting}: {error:?}"));
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "local loop lowering is missing: {failures:#?}"
        );
    }
}
