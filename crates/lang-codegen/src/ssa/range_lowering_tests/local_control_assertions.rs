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

/// Both short-circuit edges and every later statement-if edge retain the exact
/// root/descriptor/source tuple; the RHS read loan ends only on the chosen edge.
pub(super) fn short_circuit_transport(program: &Program, span: Span, and: bool) {
    let function = program.modules[0]
        .functions
        .iter()
        .find(|function| {
            function.blocks.iter().any(|block| {
                block
                    .terminator
                    .as_ref()
                    .is_some_and(|term| term.origin.span() == span)
            })
        })
        .expect("the dynamic source short circuit must emit its own conditional");
    let branch = function
        .blocks
        .iter()
        .find(|block| {
            block
                .terminator
                .as_ref()
                .is_some_and(|term| term.origin.span() == span)
        })
        .unwrap();
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &branch.terminator.as_ref().unwrap().kind
    else {
        panic!("short circuit condition")
    };
    let state = initial_carriers(function, 3);
    let (rhs, skip) = if and {
        (when_true, when_false)
    } else {
        (when_false, when_true)
    };
    let rhs_state = carried(function, rhs, &state);
    let rhs_block = &function.blocks[rhs.target.index()];
    let calls = rhs_block
        .instructions
        .iter()
        .filter_map(|id| {
            let instruction = function.instruction(*id).unwrap();
            let Operation::DirectCall { arguments, .. } = &instruction.operation else {
                return None;
            };
            let [EntityId::Loan(loan), _] = arguments.as_slice() else {
                return None;
            };
            Some((instruction, *loan))
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1, "exactly one RHS reader invocation");
    let (call, loan) = calls[0];
    let Operation::DirectCall { arguments, .. } = &call.operation else {
        unreachable!()
    };
    let EntityId::Loan(boolean) = arguments[1] else {
        panic!("default Boolean parameter remains Borrow");
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = function.entity(EntityId::Loan(boolean)).unwrap().ty
    else {
        panic!("RHS flag is a shared Boolean loan");
    };
    assert_eq!(
        program.modules[0].type_kind(target),
        Some(&SsaTypeKind::Boolean)
    );
    let original = function.blocks[0].parameters[1];
    assert_eq!(
        function.entity(original).unwrap().ty,
        function.entity(EntityId::Loan(boolean)).unwrap().ty
    );
    let expected = if EntityId::Loan(boolean) == original {
        original
    } else {
        carried_entity(function, rhs, original)
    };
    assert_eq!(
        EntityId::Loan(boolean),
        expected,
        "RHS reuses the exact right-parameter Borrow loan"
    );
    assert!(!function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::BorrowEnd {loan} if loan==boolean)),"callee return must not end the inherited Boolean loan");
    assert_eq!(
        borrowed_owner(function, loan),
        rhs_state.descriptors[1].0,
        "RHS reads the rebound child metadata"
    );
    assert!(instruction_position(function, call) < read_end_position(function, rhs.target, loan));
    assert!(
        !function.blocks[skip.target.index()]
            .instructions
            .iter()
            .any(|id| matches!(
                function.instruction(*id).unwrap().operation,
                Operation::DirectCall { .. } | Operation::BorrowBegin { .. }
            )),
        "skip cannot execute the RHS or create its loan"
    );
    short_circuit_cleanup(function, rhs, skip, &state);
}

fn short_circuit_cleanup(function: &Function, rhs: &Edge, skip: &Edge, state: &Carriers) {
    let mut seen = std::collections::BTreeMap::<BlockId, Carriers>::new();
    let mut pending = vec![
        (rhs.target, carried(function, rhs, state)),
        (skip.target, carried(function, skip, state)),
    ];
    let mut cleanups = 0;
    while let Some((block, state)) = pending.pop() {
        if let Some(prior) = seen.insert(block, state.clone()) {
            assert_eq!(
                prior, state,
                "each incoming edge rebinds the same exact carrier tuple"
            );
            continue;
        }
        let block_node = &function.blocks[block.index()];
        if block_node.instructions.iter().any(|id| {
            matches!(
                function.instruction(*id).unwrap().operation,
                Operation::RangeEnd { .. }
            )
        }) {
            assert_reads(function, block, &state);
            assert_cleanup(function, block, &state);
            let ends = block_node
                .instructions
                .iter()
                .filter_map(|id| match function.instruction(*id).unwrap().operation {
                    Operation::RangeEnd { view, source } => Some((view, source)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                ends,
                state.descriptors.iter().rev().copied().collect::<Vec<_>>(),
                "sibling then child then parent cleanup"
            );
            cleanups += 1;
            continue;
        }
        assert!(!block_node.instructions.iter().any(|id| matches!(function.instruction(*id).unwrap().operation,Operation::Drop {owner} if owner==state.root)),"root cannot drop on an intermediate edge");
        let edges = match &block_node.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            other => panic!("normal expression must reach its cleanup: {other:?}"),
        };
        for edge in edges {
            pending.push((edge.target, carried(function, edge, &state)));
        }
    }
    assert_eq!(
        cleanups, 1,
        "both short-circuit paths reach the one lexical cleanup"
    );
}

pub(super) fn short_circuit_prefix_transport(program: &Program, span: Span) {
    let function = program.modules[0]
        .functions
        .iter()
        .find(|function| {
            function.blocks.iter().any(|block| {
                block
                    .terminator
                    .as_ref()
                    .is_some_and(|term| term.origin.span() == span)
            })
        })
        .unwrap();
    let branch = function
        .blocks
        .iter()
        .find(|block| {
            block
                .terminator
                .as_ref()
                .is_some_and(|term| term.origin.span() == span)
        })
        .unwrap();
    let state = initial_carriers(function, 3);
    let prefix=branch.instructions.iter().filter_map(|id| {
        let instruction=function.instruction(*id).unwrap();
        let Operation::BorrowBegin {place,kind:LoanKind::Shared}=instruction.operation else {return None;};
        if !matches!(defining_instruction(function,EntityId::Place(place)).operation,Operation::RootPlace {owner} if owner==state.descriptors[0].0) {return None;}
        let [EntityId::Loan(loan)]=instruction.results.as_slice() else {panic!("metadata prefix loan");};
        (!branch.instructions.iter().any(|id| matches!(function.instruction(*id).unwrap().operation,Operation::BorrowEnd {loan:ended} if ended==*loan))).then_some(*loan)
    }).collect::<Vec<_>>();
    assert_eq!(
        prefix.len(),
        1,
        "one already-active parent metadata prefix loan"
    );
    let mut pending = vec![(branch.id, prefix[0])];
    let mut seen = std::collections::BTreeMap::new();
    let mut ended = 0;
    while let Some((block, loan)) = pending.pop() {
        if let Some(prior) = seen.insert(block, loan) {
            assert_eq!(prior, loan);
            continue;
        }
        let block_node = &function.blocks[block.index()];
        if block_node.instructions.iter().any(|id| matches!(function.instruction(*id).unwrap().operation,Operation::BorrowEnd {loan:actual} if actual==loan)) {
            let calls=block_node.instructions.iter().filter_map(|id| {
                let instruction=function.instruction(*id).unwrap();
                matches!(&instruction.operation,Operation::DirectCall {arguments,..} if arguments.first()==Some(&EntityId::Loan(loan))).then_some(instruction)
            }).collect::<Vec<_>>();
            assert_eq!(calls.len(),1,"prefix delivers once to the outer synchronous call");
            let end=read_end_position(function,block,loan);
            assert!(instruction_position(function,calls[0])<end);
            let range_end=block_node.instructions.iter().position(|id| matches!(function.instruction(*id).unwrap().operation,Operation::RangeEnd {..})).unwrap();
            assert!(end<range_end,"outer call loan ends before descriptor cleanup");
            ended+=1;
            continue;
        }
        let edges = match &block_node.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            other => panic!("prefix must reach normal call-return: {other:?}"),
        };
        for edge in edges {
            let EntityId::Loan(rebound) = carried_entity(function, edge, EntityId::Loan(loan))
            else {
                panic!("prefix stays a loan")
            };
            pending.push((edge.target, rebound));
        }
    }
    assert_eq!(ended, 1);
}

/// Named ordinary receivers create separate, call-scoped loans on selected paths.
pub(super) fn short_circuit_receiver_transport(program: &Program, span: Span, and: bool) {
    let module = &program.modules[0];
    let function = module
        .functions
        .iter()
        .find(|function| {
            function.blocks.iter().any(|block| {
                block
                    .terminator
                    .as_ref()
                    .is_some_and(|term| term.origin.span() == span)
            })
        })
        .unwrap();
    let branch = function
        .blocks
        .iter()
        .find(|block| {
            block
                .terminator
                .as_ref()
                .is_some_and(|term| term.origin.span() == span)
        })
        .unwrap();
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &branch.terminator.as_ref().unwrap().kind
    else {
        panic!("source short circuit");
    };
    let (rhs, skip) = if and {
        (when_true, when_false)
    } else {
        (when_false, when_true)
    };
    let calls = function
        .instructions
        .iter()
        .filter(|instruction| {
            let origin = instruction.origin.span();
            origin.source_id() == span.source_id()
                && origin.start() >= span.start()
                && origin.end() <= span.end()
                && matches!(instruction.operation, Operation::DirectCall { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        calls.len(),
        2,
        "LHS and RHS each have one source method call"
    );
    let mut owners = Vec::new();
    for (call, block) in calls.iter().zip([branch.id, rhs.target]) {
        assert_eq!(call.block, block);
        let Operation::DirectCall {
            callee,
            receiver: Some(EntityId::Loan(loan)),
            arguments,
        } = &call.operation
        else {
            panic!("readonly receiver uses its own Loan operand");
        };
        assert!(arguments.is_empty());
        let target = module.function(*callee).unwrap();
        assert_eq!(
            target.receiver(),
            Some(function.entity(EntityId::Loan(*loan)).unwrap().ty)
        );
        assert_eq!(target.blocks[0].parameters.len(), 1);
        let owner = borrowed_owner(function, *loan);
        assert_eq!(
            function.entity(EntityId::Loan(*loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: function
                    .entity(EntityId::Value(owner))
                    .unwrap()
                    .ty
                    .semantic_type()
            }
        );
        let begin = defining_instruction(function, EntityId::Loan(*loan));
        assert_eq!(
            begin.block, block,
            "receiver loan starts only on its call path"
        );
        assert!(instruction_position(function, begin) < instruction_position(function, call));
        assert!(instruction_position(function, call) < read_end_position(function, block, *loan));
        assert_eq!(function.instructions.iter().filter(|instruction| matches!(instruction.operation,Operation::BorrowEnd {loan:ended} if ended==*loan)).count(),1);
        let [EntityId::Value(result)] = call.results.as_slice() else {
            panic!("owned Boolean result");
        };
        assert_eq!(
            module.type_kind(
                function
                    .entity(EntityId::Value(*result))
                    .unwrap()
                    .ty
                    .semantic_type()
            ),
            Some(&SsaTypeKind::Boolean)
        );
        owners.push(owner);
    }
    assert!(
        !function.blocks[skip.target.index()]
            .instructions
            .iter()
            .any(|id| matches!(
                function.instruction(*id).unwrap().operation,
                Operation::DirectCall { .. } | Operation::BorrowBegin { .. }
            )),
        "skip creates no RHS receiver loan or call"
    );
    // Trace the RHS owner back across the conditional edge rather than matching types.
    let right = if let Some(parameter) = function.blocks[rhs.target.index()]
        .parameters
        .iter()
        .position(|entity| *entity == EntityId::Value(owners[1]))
    {
        let EntityId::Value(value) = rhs.arguments[parameter] else {
            panic!("owned receiver carrier");
        };
        value
    } else {
        assert_eq!(
            module.type_ownership(
                function
                    .entity(EntityId::Value(owners[1]))
                    .unwrap()
                    .ty
                    .semantic_type()
            ),
            Some(Ownership::Copyable)
        );
        assert_eq!(
            defining_instruction(function, EntityId::Value(owners[1])).block,
            branch.id,
            "only a dominating Copyable owner can bypass rebinding"
        );
        owners[1]
    };
    assert_ne!(
        owners[0], right,
        "the two named receiver owners remain distinct"
    );
    let mut pending = vec![(branch.id, [owners[0], right])];
    let mut seen = std::collections::BTreeMap::new();
    let mut finishes = 0;
    while let Some((block, owners)) = pending.pop() {
        if let Some(prior) = seen.insert(block, owners) {
            assert_eq!(prior, owners);
            continue;
        }
        let node = &function.blocks[block.index()];
        assert!(!node.instructions.iter().any(|id| matches!(function.instruction(*id).unwrap().operation,Operation::Drop {owner} if owners.contains(&owner))),"receiver owners remain live until explicit finish");
        if let Some(last_end) = node.instructions.iter().rposition(|id| {
            matches!(
                function.instruction(*id).unwrap().operation,
                Operation::RangeEnd { .. }
            )
        }) {
            for owner in owners {
                let transfers=node.instructions.iter().enumerate().filter_map(|(index,id)| {
                    let Operation::DirectCall {receiver:None,arguments,..}=&function.instruction(*id).unwrap().operation else {return None;};
                    let [EntityId::Value(actual)]=arguments.as_slice() else {return None;};
                    let copied=match function.entity(EntityId::Value(*actual)).unwrap().definition {
                        Definition::InstructionResult {instruction,..}=>matches!(function.instruction(instruction).unwrap().operation,Operation::Copy {source} if source==owner),
                        _=>false,
                    };
                    let delivered=*actual==owner || copied;
                    delivered.then_some(index)
                }).collect::<Vec<_>>();
                assert_eq!(
                    transfers.len(),
                    1,
                    "each receiver owner transfers exactly once after the join"
                );
                assert!(last_end < transfers[0]);
            }
            finishes += 1;
            continue;
        }
        let edges = match &node.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            other => panic!("normal receiver continuation: {other:?}"),
        };
        for edge in edges {
            let rebound = owners.map(|owner| {
                if edge.arguments.contains(&EntityId::Value(owner)) {
                    let EntityId::Value(value) =
                        carried_entity(function, edge, EntityId::Value(owner))
                    else {
                        panic!("owner carrier");
                    };
                    value
                } else {
                    assert_eq!(
                        module.type_ownership(
                            function
                                .entity(EntityId::Value(owner))
                                .unwrap()
                                .ty
                                .semantic_type()
                        ),
                        Some(Ownership::Copyable)
                    );
                    owner
                }
            });
            pending.push((edge.target, rebound));
        }
    }
    assert_eq!(finishes, 1);
    short_circuit_cleanup(function, rhs, skip, &initial_carriers(function, 3));
}
