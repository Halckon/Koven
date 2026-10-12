//! Identity-based checks over valid, source-produced SSA only.
use super::*;
use lang_frontend::source::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Carriers {
    root: ValueId,
    root_loan: LoanId,
    descriptors: Vec<(ValueId, LoanId)>,
}

fn defining_instruction(function: &Function, entity: EntityId) -> &Instruction {
    let Definition::InstructionResult { instruction, .. } =
        function.entity(entity).unwrap().definition
    else {
        panic!("expected an instruction-defined entity: {entity:?}")
    };
    function.instruction(instruction).unwrap()
}

fn borrowed_owner(function: &Function, loan: LoanId) -> ValueId {
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = defining_instruction(function, EntityId::Loan(loan)).operation
    else {
        panic!("source/read loan must be an explicit shared borrow")
    };
    let Operation::RootPlace { owner } =
        defining_instruction(function, EntityId::Place(place)).operation
    else {
        panic!("source/read must borrow a whole root")
    };
    owner
}

fn initial_carriers(function: &Function, count: usize) -> Carriers {
    let calls = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::RangeCall { .. }))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), count);
    let Operation::RangeCall {
        source: root_loan, ..
    } = calls[0].operation
    else {
        unreachable!()
    };
    let root = borrowed_owner(function, root_loan);
    let descriptors = calls
        .iter()
        .map(|call| {
            let [EntityId::Value(view), EntityId::Loan(source)] = call.results.as_slice() else {
                panic!("a range call must return a descriptor and its root continuation")
            };
            (*view, *source)
        })
        .collect::<Vec<_>>();
    // In the larger fixture child and sibling borrow the same parent metadata;
    // their returned continuations still protect the collection, not that metadata.
    for call in &calls[1..] {
        let Operation::RangeCall { source, .. } = call.operation else {
            unreachable!()
        };
        assert_eq!(borrowed_owner(function, source), descriptors[0].0);
        let end = function.instructions.iter().find(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == source)
        }).expect("temporary producer metadata borrow must end");
        assert_eq!(end.block, call.block);
        assert!(instruction_position(function, call) < instruction_position(function, end));
    }
    let root_type = function
        .entity(EntityId::Value(root))
        .unwrap()
        .ty
        .semantic_type();
    for (_, source) in &descriptors {
        assert_eq!(
            function.entity(EntityId::Loan(*source)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: root_type
            }
        );
    }
    Carriers {
        root,
        root_loan,
        descriptors,
    }
}

fn instruction_position(function: &Function, instruction: &Instruction) -> usize {
    function.blocks[instruction.block.index()]
        .instructions
        .iter()
        .position(|id| *id == instruction.id)
        .unwrap()
}

fn carried_entity(function: &Function, edge: &Edge, entity: EntityId) -> EntityId {
    let positions = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, actual)| (*actual == entity).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(
        positions.len(),
        1,
        "edge must carry {entity:?} exactly once: {edge:?}"
    );
    let rebound = function.blocks[edge.target.index()].parameters[positions[0]];
    assert_eq!(
        function.entity(entity).unwrap().ty,
        function.entity(rebound).unwrap().ty
    );
    assert_ne!(
        entity, rebound,
        "successor must use its own parameter identity"
    );
    rebound
}

fn carried(function: &Function, edge: &Edge, state: &Carriers) -> Carriers {
    let EntityId::Value(root) = carried_entity(function, edge, EntityId::Value(state.root)) else {
        panic!("root remains an owned value")
    };
    let EntityId::Loan(root_loan) = carried_entity(function, edge, EntityId::Loan(state.root_loan))
    else {
        panic!("source remains a loan")
    };
    let descriptors = state
        .descriptors
        .iter()
        .map(|(view, source)| {
            let EntityId::Value(view) = carried_entity(function, edge, EntityId::Value(*view))
            else {
                panic!("descriptor remains a value")
            };
            let EntityId::Loan(source) = carried_entity(function, edge, EntityId::Loan(*source))
            else {
                panic!("continuation remains a loan")
            };
            (view, source)
        })
        .collect();
    Carriers {
        root,
        root_loan,
        descriptors,
    }
}

fn read_end_position(function: &Function, block: BlockId, loan: LoanId) -> usize {
    let ends = function.blocks[block.index()]
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(position, id)| {
            matches!(function.instructions[id.index()].operation,
                Operation::BorrowEnd { loan: actual } if actual == loan)
            .then_some(position)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ends.len(),
        1,
        "each metadata/source loan must end exactly once: {loan:?}"
    );
    ends[0]
}

fn assert_reads(function: &Function, block: BlockId, state: &Carriers) {
    let instructions = &function.blocks[block.index()].instructions;
    for (view, _) in &state.descriptors {
        let calls = instructions
            .iter()
            .filter_map(|id| {
                let instruction = function.instruction(*id).unwrap();
                let Operation::DirectCall { arguments, .. } = &instruction.operation else {
                    return None;
                };
                let [EntityId::Loan(loan)] = arguments.as_slice() else {
                    return None;
                };
                (borrowed_owner(function, *loan) == *view).then_some((instruction, *loan))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            calls.len(),
            1,
            "each descriptor must be read once in {block:?}"
        );
        let (call, loan) = calls[0];
        assert!(instruction_position(function, call) < read_end_position(function, block, loan));
    }
}

fn assert_cleanup(function: &Function, block: BlockId, state: &Carriers) {
    let instructions = function.blocks[block.index()]
        .instructions
        .iter()
        .map(|id| function.instruction(*id).unwrap())
        .collect::<Vec<_>>();
    let source_end = read_end_position(function, block, state.root_loan);
    let consume = instructions
        .iter()
        .position(|instruction| {
            matches!(&instruction.operation, Operation::DirectCall { arguments, .. }
            if arguments.as_slice() == [EntityId::Value(state.root)])
        })
        .expect("the same root owner must be transferred to consume after scope exit");
    for (descriptor_index, (view, source)) in state.descriptors.iter().enumerate() {
        let ends = instructions.iter().enumerate().filter_map(|(index, instruction)| {
            matches!(instruction.operation, Operation::RangeEnd { view: actual_view, source: actual_source }
                if actual_view == *view && actual_source == *source).then_some(index)
        }).collect::<Vec<_>>();
        assert_eq!(
            ends.len(),
            1,
            "the exact descriptor/root-continuation pair must end once"
        );
        let reads = instructions
            .iter()
            .filter_map(|instruction| {
                let Operation::DirectCall { arguments, .. } = &instruction.operation else {
                    return None;
                };
                let [EntityId::Loan(loan)] = arguments.as_slice() else {
                    return None;
                };
                (borrowed_owner(function, *loan) == *view).then_some((*instruction, *loan))
            })
            .collect::<Vec<_>>();
        assert!(
            !reads.is_empty(),
            "each descriptor is used before its scope ends"
        );
        for (read, loan) in reads {
            let read_end = read_end_position(function, block, loan);
            assert!(instruction_position(function, read) < read_end);
            assert!(
                read_end < ends[0],
                "metadata read loan must end before its descriptor RangeEnd"
            );
        }
        assert!(
            ends[0] < consume,
            "every continuation protects the root until its own RangeEnd"
        );
        if descriptor_index == 0 {
            assert!(
                ends[0] < source_end,
                "the parent descriptor must end before its source loan"
            );
        }
    }
    assert!(
        source_end < consume,
        "RangeEnd -> source BorrowEnd -> root consumption"
    );
}

pub(super) fn comparison_lifetime(program: &Program, span: Span, position: Position) {
    let function = program.modules[0]
        .functions
        .iter()
        .find(|function| {
            function.instructions.iter().any(|instruction| {
                instruction.origin.span() == span
                    && matches!(instruction.operation, Operation::Compare { .. })
            })
        })
        .unwrap();
    let comparison = function
        .instructions
        .iter()
        .find(|instruction| {
            instruction.origin.span() == span
                && matches!(instruction.operation, Operation::Compare { .. })
        })
        .unwrap();
    let state = initial_carriers(function, 1);
    let call = defining_instruction(function, EntityId::Value(state.descriptors[0].0));
    let end = function.instructions.iter().find(|instruction| {
        matches!(instruction.operation, Operation::RangeEnd { view, .. } if view == state.descriptors[0].0)
    }).unwrap();
    assert_eq!(comparison.block, call.block);
    assert_eq!(comparison.block, end.block);
    let compared = instruction_position(function, comparison);
    match position {
        Position::BeforeBinding => assert!(compared < instruction_position(function, call)),
        Position::WhileLive => {
            assert!(instruction_position(function, call) < compared);
            assert!(compared < instruction_position(function, end));
        }
        Position::AfterEnds => {
            let source_end = function.instructions.iter().find(|instruction| {
                matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == state.root_loan)
            }).unwrap();
            assert!(
                instruction_position(function, end) < instruction_position(function, source_end)
            );
            assert!(instruction_position(function, source_end) < compared);
        }
    }
    assert_cleanup(function, comparison.block, &state);
}

pub(super) fn if_transport(program: &Program, span: Span, count: usize) {
    let function = program.modules[0]
        .functions
        .iter()
        .find(|function| {
            function.blocks.iter().any(|block| {
                block.terminator.as_ref().is_some_and(|terminator| {
                    terminator.origin.span() == span
                        && matches!(terminator.kind, TerminatorKind::Conditional { .. })
                })
            })
        })
        .expect("source statement-if must retain its exact conditional terminator");
    let branch = function
        .blocks
        .iter()
        .find(|block| {
            block.terminator.as_ref().is_some_and(|terminator| {
                terminator.origin.span() == span
                    && matches!(terminator.kind, TerminatorKind::Conditional { .. })
            })
        })
        .unwrap();
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &branch.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    assert_ne!(when_true.target, when_false.target);
    let state = initial_carriers(function, count);
    let mut exits = Vec::new();
    for edge in [when_true, when_false] {
        let arm_state = carried(function, edge, &state);
        assert_reads(function, edge.target, &arm_state);
        let arm = &function.blocks[edge.target.index()];
        assert!(
            !arm.instructions.iter().any(|id| matches!(
                function.instructions[id.index()].operation,
                Operation::RangeEnd { .. } | Operation::Drop { .. } | Operation::Consume { .. }
            )),
            "normal arms must preserve the root and all descriptors for the merge"
        );
        let TerminatorKind::Branch(merge) = &arm.terminator.as_ref().unwrap().kind else {
            panic!("both statement-if arms must finish normally")
        };
        exits.push((merge.target, carried(function, merge, &arm_state)));
    }
    assert_eq!(
        exits[0], exits[1],
        "both arms must merge the same descriptor/root relationships"
    );
    let (merge, state) = &exits[0];
    assert_reads(function, *merge, state);
    assert_cleanup(function, *merge, state);
}
