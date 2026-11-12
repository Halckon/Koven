//! One outer borrowed result is transported unchanged across a bounded syntax while.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};

#[test]
fn ordinary_invariant_outer_while_preserves_one_lease() {
    for jump in [
        None,
        Some("continue"),
        Some("break"),
        Some("conditional-break"),
        Some("conditional-continue"),
        Some("return"),
    ] {
        let inner_names: &[&str] =
            if matches!(jump, Some("conditional-break" | "conditional-continue")) {
                &["flag2", "flag"]
            } else {
                &["flag2"]
            };
        for &inner_name in inner_names {
            for count in [1, 2, 3, 8, 17] {
                for own in [false, true] {
                    for condition in ["flag", "false", "true"] {
                        let body = (0..count)
                            .map(|i| {
                                if i % 2 == 0 {
                                    "consume(item)"
                                } else {
                                    "consume_other(item)"
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("; ");
                        let body = if jump == Some("conditional-break") {
                            format!("{body}; if ({inner_name}) {{ break }}")
                        } else if jump == Some("conditional-continue") {
                            format!("{body}; if ({inner_name}) {{ continue }}")
                        } else if let Some(jump) = jump {
                            format!("{body}; {jump}")
                        } else {
                            body
                        };
                        let mode = if own { "own " } else { "" };
                        let text = format!(
                            "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun consume_other(item: String): Unit {{}}\nfun run({mode}source: String, flag: Boolean, flag2: Boolean): Unit {{ borrow val item = view(source); while ({condition}) {{ {body} }} }}"
                        );
                        let mut sources = SourceMap::new();
                        let (source, parsed) = unit_lower_test_support::parsed(
                            &mut sources,
                            "invariant-while.ko",
                            &text,
                        );
                        let (environment, types) = standard_environments();
                        for (layout, input) in [
                            (
                                "rooted",
                                SourceUnitInput::new("root", "invariant-while.ko", source, &parsed),
                            ),
                            ("memory", SourceUnitInput::memory_document(source, &parsed)),
                        ] {
                            let inputs = [input];
                            let (names, typed, on) = unit_lower_test_support::analyze(
                                &sources,
                                &inputs,
                                &environment,
                                &types,
                            );
                            let off = with_loan_observation(false, || {
                                check_compilation_unit_ownership(
                                    &sources, &inputs, &names, &types, &typed,
                                )
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
                        panic!("jump={jump:?}/inner={inner_name}/count={count}/own={own}/condition={condition}/{layout}/{setting}: {error:?}")
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
                                        .filter(|i| matches!(
                                            i.operation,
                                            Operation::BorrowCall { .. }
                                        ))
                                        .count(),
                                    1
                                );
                                assert_eq!(
                                    run.instructions
                                        .iter()
                                        .filter(|i| matches!(
                                            i.operation,
                                            Operation::DirectCall { .. }
                                        ))
                                        .count(),
                                    count
                                );
                                let calls = run
                                    .instructions
                                    .iter()
                                    .filter_map(|i| match i.operation {
                                        Operation::DirectCall { callee, .. } => Some((i, callee)),
                                        _ => None,
                                    })
                                    .collect::<Vec<_>>();
                                for (index, (call, callee)) in calls.iter().enumerate() {
                                    let expected = if index % 2 == 0 {
                                        ".consume."
                                    } else {
                                        ".consume_other."
                                    };
                                    assert!(
                                        program
                                            .modules
                                            .iter()
                                            .flat_map(|module| &module.functions)
                                            .find(|function| function.id == *callee)
                                            .unwrap()
                                            .name
                                            .contains(expected)
                                    );
                                    assert_eq!(call.block, calls[0].0.block);
                                    let block =
                                        run.blocks.iter().find(|b| b.id == call.block).unwrap();
                                    if index > 0 {
                                        assert!(
                                            block
                                                .instructions
                                                .iter()
                                                .position(|id| *id == calls[index - 1].0.id)
                                                .unwrap()
                                                < block
                                                    .instructions
                                                    .iter()
                                                    .position(|id| *id == call.id)
                                                    .unwrap()
                                        );
                                    }
                                }
                                assert_eq!(
                                    run.instructions
                                        .iter()
                                        .filter(|i| matches!(
                                            i.operation,
                                            Operation::BorrowBegin { .. }
                                        ))
                                        .count(),
                                    usize::from(own),
                                    "no per-consumer or per-iteration acquisition"
                                );
                                assert_transport(
                                    run,
                                    own,
                                    condition,
                                    jump.map(|jump| {
                                        (
                                            text.find(if jump == "conditional-break" {
                                                "break"
                                            } else if jump == "conditional-continue" {
                                                "continue"
                                            } else {
                                                jump
                                            })
                                            .unwrap(),
                                            jump,
                                        )
                                    }),
                                    if inner_name == "flag" { 1 } else { 2 },
                                );
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

fn assert_transport(
    run: &Function,
    own: bool,
    literal: &str,
    jump: Option<(usize, &str)>,
    inner_parameter: usize,
) {
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
    let conditional_break = jump.is_some_and(|(_, kind)| kind == "conditional-break");
    let conditional_continue = jump.is_some_and(|(_, kind)| kind == "conditional-continue");
    if conditional_break || conditional_continue {
        assert_eq!(run.blocks[0].parameters.len(), 3);
        lineage.insert(run.blocks[0].parameters[inner_parameter], 8);
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
    if let Some((start, "return")) = jump {
        assert_return_paths(run, own, &lineage, header, acquisition.block, start);
        return;
    }
    let incoming = run
        .blocks
        .iter()
        .flat_map(|b| edges(b).into_iter().map(move |edge| (b, edge)))
        .filter(|(_, edge)| edge.target == header.id)
        .collect::<Vec<_>>();
    let broken = jump.is_some_and(|(_, kind)| kind == "break");
    if broken {
        assert!(
            incoming.is_empty(),
            "break has no backedge after entry/header folding"
        );
        assert_eq!(
            header.id, acquisition.block,
            "actual original acquisition is retained in the folded header"
        );
    } else {
        assert_eq!(
            incoming.len(),
            if conditional_continue { 3 } else { 2 },
            "entry and all real backedges"
        );
    }
    let mut break_target = None;
    let mut continue_block = None;
    if let Some((start, kind)) = jump {
        let jumps = run
            .blocks
            .iter()
            .filter(|block| {
                block.terminator.as_ref().is_some_and(|term| {
                    term.origin.span().start() == start
                        && term.origin.span().end()
                            == start
                                + if kind == "conditional-break" {
                                    "break".len()
                                } else if kind == "conditional-continue" {
                                    "continue".len()
                                } else {
                                    kind.len()
                                }
                })
            })
            .collect::<Vec<_>>();
        let [jump] = jumps.as_slice() else {
            panic!("exactly one actual tail-jump terminator");
        };
        let TerminatorKind::Branch(edge) = &jump.terminator.as_ref().unwrap().kind else {
            panic!("tail jump must be an unconditional Goto");
        };
        if broken || conditional_break {
            assert_ne!(edge.target, header.id);
            break_target = Some((jump.id, edge.target));
        } else {
            assert_eq!(edge.target, header.id);
            continue_block = Some(jump.id);
            assert!(incoming.iter().any(|(block, _)| block.id == jump.id));
        }
        assert_ne!(jump.id, acquisition.block);
        for id in &jump.instructions {
            let instruction = run
                .instructions
                .iter()
                .find(|instruction| instruction.id == *id)
                .unwrap();
            assert!(
                !matches!(
                    instruction.operation,
                    Operation::BorrowEnd { .. }
                        | Operation::Drop { .. }
                        | Operation::BorrowBegin { .. }
                        | Operation::BorrowCall { .. }
                ),
                "jump body does not retire or reacquire the outer lease"
            );
        }
    }

    if conditional_break || conditional_continue {
        let splits = run
            .blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.terminator.as_ref().unwrap().kind,
                    TerminatorKind::Conditional { .. }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(splits.len(), 2);
        let inner = splits
            .into_iter()
            .find(|block| block.id != header.id)
            .unwrap();
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &inner.terminator.as_ref().unwrap().kind
        else {
            unreachable!()
        };
        let Definition::InstructionResult { instruction, .. } =
            run.entity(EntityId::Value(*condition)).unwrap().definition
        else {
            panic!("real inner condition read");
        };
        let Operation::Read {
            source: PlaceAccess::Loan(flag),
        } = run.instruction(instruction).unwrap().operation
        else {
            panic!("second Boolean loan is read");
        };
        assert_eq!(lineage.get(&EntityId::Loan(flag)), Some(&8));
        if conditional_break {
            let (break_block, _) = break_target.unwrap();
            assert_eq!(when_true.target, break_block);
        } else {
            assert_eq!(when_true.target, continue_block.unwrap());
            assert_ne!(when_true.target, when_false.target);
        }
        let false_block = run
            .blocks
            .iter()
            .find(|block| block.id == when_false.target)
            .unwrap();
        let TerminatorKind::Branch(backedge) = &false_block.terminator.as_ref().unwrap().kind
        else {
            panic!("conditional false is the real normal backedge");
        };
        assert_eq!(backedge.target, header.id);
        if conditional_continue {
            let continued = run
                .blocks
                .iter()
                .find(|block| block.id == continue_block.unwrap())
                .unwrap();
            assert_ne!(
                continued.terminator.as_ref().unwrap().origin.span(),
                false_block.terminator.as_ref().unwrap().origin.span(),
                "explicit Continue and normal backedge retain different actual origins"
            );
        }
        assert!(incoming.iter().any(|(block, _)| block.id == false_block.id));
        for edge in [when_true, when_false] {
            assert_eq!(
                edge.arguments
                    .iter()
                    .filter(|argument| lineage.get(argument) == Some(&2))
                    .count(),
                1
            );
            if own {
                assert_eq!(
                    edge.arguments
                        .iter()
                        .filter(|argument| lineage.get(argument) == Some(&1))
                        .count(),
                    1
                );
            }
        }
    }

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
    let consumers = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
        .collect::<Vec<_>>();
    let consumer = consumers[0];
    for consumer in &consumers {
        let Operation::DirectCall { ref arguments, .. } = consumer.operation else {
            unreachable!()
        };
        assert_eq!(arguments.len(), 1);
        assert_eq!(lineage.get(&arguments[0]), Some(&2));
    }
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
    if let Some((break_block, exit)) = break_target {
        assert_eq!(
            result_end.block, exit,
            "retirement only after the common exit"
        );
        let false_block = run
            .blocks
            .iter()
            .find(|block| block.id == when_false.target)
            .unwrap();
        let TerminatorKind::Branch(false_exit) = &false_block.terminator.as_ref().unwrap().kind
        else {
            panic!("false path reaches the common exit");
        };
        assert_eq!(false_exit.target, exit);
        let predecessors = run
            .blocks
            .iter()
            .flat_map(|block| edges(block).into_iter().map(move |edge| (block.id, edge)))
            .filter(|(_, edge)| edge.target == exit)
            .collect::<Vec<_>>();
        assert_eq!(predecessors.len(), 2);
        assert!(predecessors.iter().any(|(block, _)| *block == break_block));
        assert!(
            predecessors
                .iter()
                .any(|(block, _)| *block == false_block.id)
        );
        for (_, edge) in predecessors {
            assert_eq!(
                edge.arguments
                    .iter()
                    .filter(|argument| lineage.get(argument) == Some(&2))
                    .count(),
                1
            );
            if own {
                assert_eq!(
                    edge.arguments
                        .iter()
                        .filter(|argument| lineage.get(argument) == Some(&1))
                        .count(),
                    1
                );
            }
        }
    } else {
        assert_eq!(
            result_end.block, when_false.target,
            "cleanup only on false exit"
        );
    }
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

fn assert_return_paths(
    run: &Function,
    own: bool,
    lineage: &std::collections::BTreeMap<EntityId, u8>,
    header: &Block,
    acquisition: BlockId,
    start: usize,
) {
    assert_eq!(header.id, acquisition);
    assert!(
        !run.blocks
            .iter()
            .any(|block| edges(block).iter().any(|edge| edge.target == header.id)),
        "return body has no backedge"
    );
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    assert_ne!(when_true.target, when_false.target);
    assert_eq!(
        run.blocks.len(),
        3,
        "one condition and two separately closed exits"
    );
    for (returned, edge) in [(true, when_true), (false, when_false)] {
        assert_eq!(
            edge.arguments
                .iter()
                .filter(|argument| lineage.get(argument) == Some(&2))
                .count(),
            1
        );
        if own {
            assert_eq!(
                edge.arguments
                    .iter()
                    .filter(|argument| lineage.get(argument) == Some(&1))
                    .count(),
                1
            );
        }
        let block = run
            .blocks
            .iter()
            .find(|block| block.id == edge.target)
            .unwrap();
        let term = block.terminator.as_ref().unwrap();
        assert!(matches!(&term.kind, TerminatorKind::Return { values } if values.is_empty()));
        if returned {
            assert_eq!(term.origin.span().start(), start);
            assert_eq!(term.origin.span().end(), start + "return".len());
        }
        let operations = block
            .instructions
            .iter()
            .map(|id| run.instruction(*id).unwrap())
            .collect::<Vec<_>>();
        let count = if own { 3 } else { 1 };
        let (calls, cleanup) = operations.split_at(operations.len().checked_sub(count).unwrap());
        assert_eq!(calls.is_empty(), !returned);
        for instruction in calls {
            let Operation::DirectCall { arguments, .. } = &instruction.operation else {
                panic!("only synchronous consumers precede return cleanup");
            };
            let [argument] = arguments.as_slice() else {
                panic!("one result argument");
            };
            assert_eq!(lineage.get(argument), Some(&2));
        }
        let Operation::BorrowEnd { loan } = cleanup[0].operation else {
            panic!("result ends first on each path");
        };
        assert_eq!(lineage.get(&EntityId::Loan(loan)), Some(&2));
        if own {
            let Operation::BorrowEnd { loan } = cleanup[1].operation else {
                panic!("local source ends second");
            };
            assert_eq!(lineage.get(&EntityId::Loan(loan)), Some(&1));
            let Operation::Drop { owner } = cleanup[2].operation else {
                panic!("original owner drops last");
            };
            assert_eq!(lineage.get(&EntityId::Value(owner)), Some(&4));
        }
    }
    assert_eq!(
        run.instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
            .count(),
        if own { 4 } else { 2 }
    );
    assert_eq!(
        run.instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        if own { 2 } else { 0 }
    );
}
