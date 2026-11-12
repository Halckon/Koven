//! One outer borrowed result is transported unchanged across a bounded syntax while.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};

#[test]
fn ordinary_invariant_outer_while_preserves_one_lease() {
    for own in [false, true] {
        for condition in ["flag", "false", "true"] {
            let mode = if own { "own " } else { "" };
            let text = format!(
                "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run({mode}source: String, flag: Boolean): Unit {{ borrow val item = view(source); while ({condition}) {{ consume(item) }} }}"
            );
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "invariant-while.ko", &text);
            let (environment, types) = standard_environments();
            for (layout, input) in [
                (
                    "rooted",
                    SourceUnitInput::new("root", "invariant-while.ko", source, &parsed),
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
                    let (lowered, verified) =
                        crate::ssa::unit_lower::observe_final_verifier(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        });
                    let (program, _) = lowered.unwrap_or_else(|error| {
                        panic!("own={own}/condition={condition}/{layout}/{setting}: {error:?}")
                    });
                    assert_eq!(verified.len(), 1);
                    verify_program(&program).unwrap();
                    let run = program.modules[0]
                        .functions
                        .iter()
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
                            .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                            .count(),
                        1
                    );
                    assert_transport(run, own, condition);
                    let text = format!(
                        "{verified:?}\n{}",
                        crate::llvm::render_verified_program(&program).unwrap()
                    );
                    if let Some(previous) = previous.as_ref() {
                        assert_eq!(&text, previous);
                    }
                    previous = Some(text);
                }
            }
        }
    }
}

fn edges(block: &Block) -> Vec<&Edge> {
    match &block.terminator.as_ref().unwrap().kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::Return { .. } => vec![],
        _ => panic!("only ordinary while CFG"),
    }
}

fn assert_transport(run: &Function, own: bool, literal: &str) {
    use std::collections::BTreeMap;
    let acquisition = run
        .instructions
        .iter()
        .find(|i| matches!(i.operation, Operation::BorrowCall { .. }))
        .unwrap();
    let Operation::BorrowCall {
        source,
        ref arguments,
        ..
    } = acquisition.operation
    else {
        unreachable!()
    };
    assert_eq!(arguments.as_slice(), &[EntityId::Loan(source)]);
    let [EntityId::Loan(result)] = acquisition.results.as_slice() else {
        panic!("one result");
    };
    assert_ne!(source, *result);
    let mut lineage = BTreeMap::from([
        (EntityId::Loan(source), 1u8),
        (EntityId::Loan(*result), 2u8),
    ]);
    if own {
        let Definition::InstructionResult { instruction, .. } =
            run.entity(EntityId::Loan(source)).unwrap().definition
        else {
            panic!("source acquired once");
        };
        let Operation::BorrowBegin { place, .. } = run.instruction(instruction).unwrap().operation
        else {
            panic!("real source Begin");
        };
        let Definition::InstructionResult { instruction, .. } =
            run.entity(EntityId::Place(place)).unwrap().definition
        else {
            panic!("root place");
        };
        let Operation::RootPlace { owner } = run.instruction(instruction).unwrap().operation else {
            panic!("source root");
        };
        lineage.insert(EntityId::Value(owner), 4);
    }
    // Follow every actual edge argument at its actual block-parameter position, including the backedge.
    for _ in 0..=run.loans.len() {
        let before = lineage.len();
        for block in &run.blocks {
            for edge in edges(block) {
                let target = run.blocks.iter().find(|b| b.id == edge.target).unwrap();
                assert_eq!(edge.arguments.len(), target.parameters.len());
                for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                    if let Some(kind) = lineage.get(argument).copied()
                        && let Some(old) = lineage.insert(*parameter, kind)
                    {
                        assert_eq!(old, kind, "source/result identity never swaps");
                    }
                }
            }
        }
        if before == lineage.len() {
            break;
        }
    }
    let header = run
        .blocks
        .iter()
        .find(|b| {
            matches!(
                b.terminator.as_ref().unwrap().kind,
                TerminatorKind::Conditional { .. }
            )
        })
        .unwrap();
    let TerminatorKind::Conditional {
        condition,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    if literal != "flag" {
        let Definition::InstructionResult { instruction, .. } =
            run.entity(EntityId::Value(*condition)).unwrap().definition
        else {
            panic!("literal condition must retain its actual constant");
        };
        let actual = run
            .instructions
            .iter()
            .find(|i| i.id == instruction)
            .unwrap();
        assert_eq!(
            actual.operation,
            Operation::Constant(ScalarConstant::Boolean(literal == "true"))
        );
    }
    let incoming = run
        .blocks
        .iter()
        .flat_map(|b| edges(b).into_iter().map(move |edge| (b, edge)))
        .filter(|(_, edge)| edge.target == header.id)
        .collect::<Vec<_>>();
    assert_eq!(incoming.len(), 2, "entry and real backedge");
    for (_, edge) in &incoming {
        assert_eq!(
            edge.arguments
                .iter()
                .filter(|arg| lineage.get(arg) == Some(&2))
                .count(),
            1
        );
        if own {
            assert_eq!(
                edge.arguments
                    .iter()
                    .filter(|arg| lineage.get(arg) == Some(&1))
                    .count(),
                1
            );
        }
    }
    for edge in edges(header) {
        assert_eq!(
            edge.arguments
                .iter()
                .filter(|arg| lineage.get(arg) == Some(&2))
                .count(),
            1,
            "true and false carry result"
        );
        if own {
            assert_eq!(
                edge.arguments
                    .iter()
                    .filter(|arg| lineage.get(arg) == Some(&1))
                    .count(),
                1
            );
        }
    }
    let consumer = run
        .instructions
        .iter()
        .find(|i| matches!(i.operation, Operation::DirectCall { .. }))
        .unwrap();
    let Operation::DirectCall { ref arguments, .. } = consumer.operation else {
        unreachable!()
    };
    assert_eq!(arguments.len(), 1);
    assert_eq!(lineage.get(&arguments[0]), Some(&2));
    let ends = run
        .instructions
        .iter()
        .filter_map(|i| match i.operation {
            Operation::BorrowEnd { loan } => Some((i, lineage.get(&EntityId::Loan(loan)).copied())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), if own { 2 } else { 1 });
    let result_end = ends.iter().find(|(_, kind)| *kind == Some(2)).unwrap().0;
    assert_ne!(
        result_end.block, consumer.block,
        "never end on body/backedge"
    );
    assert_ne!(result_end.block, acquisition.block);
    assert_eq!(
        result_end.block, when_false.target,
        "cleanup is only on the false edge, even when the literal makes it dynamically unreachable"
    );
    if own {
        let source_end = ends.iter().find(|(_, kind)| *kind == Some(1)).unwrap().0;
        assert_eq!(source_end.block, result_end.block);
        let drops = run
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::Drop { owner } => Some((instruction, owner)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(drop, owner)] = drops.as_slice() else {
            panic!("one source owner drop");
        };
        assert_eq!(lineage.get(&EntityId::Value(*owner)), Some(&4));
        assert_eq!(drop.block, source_end.block);
        let block = run
            .blocks
            .iter()
            .find(|block| block.id == drop.block)
            .unwrap();
        assert!(
            block
                .instructions
                .iter()
                .position(|id| *id == source_end.id)
                .unwrap()
                < block
                    .instructions
                    .iter()
                    .position(|id| *id == drop.id)
                    .unwrap()
        );

        let block = run
            .blocks
            .iter()
            .find(|b| b.id == result_end.block)
            .unwrap();
        assert!(
            block
                .instructions
                .iter()
                .position(|id| *id == result_end.id)
                .unwrap()
                < block
                    .instructions
                    .iter()
                    .position(|id| *id == source_end.id)
                    .unwrap()
        );
    } else {
        assert!(matches!(
            run.entity(EntityId::Loan(source)).unwrap().definition,
            Definition::BlockParameter { .. }
        ));
        assert!(
            ends.iter().all(|(_, kind)| *kind != Some(1)),
            "caller lease not locally ended"
        );
    }
}
