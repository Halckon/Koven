//! Exact source-backed SSA roles and CFG cleanup for the count-break regression.
use super::*;

fn count_break_edges(function: &Function) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            let edges = match &block.terminator.as_ref().unwrap().kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => {
                    vec![when_true, when_false]
                }
                TerminatorKind::Return { .. } => Vec::new(),
                _ => panic!("this original scan has only ordinary loop/if/return edges"),
            };
            edges.into_iter().map(move |edge| (block.id, edge))
        })
        .collect()
}

// Follow each actual incoming phi column backwards. Cycles are visited once;
// every external leaf must be the exact expected seed, never a same-typed owner.
// This is directed transport, not an undirected alias equivalence relation.
fn count_break_origins(
    function: &Function,
    value: EntityId,
) -> std::collections::BTreeSet<EntityId> {
    let edges = count_break_edges(function);
    let mut pending = vec![value];
    let mut visited = std::collections::BTreeSet::new();
    let mut origins = std::collections::BTreeSet::new();
    let mut dependencies = std::collections::BTreeMap::new();
    while let Some(value) = pending.pop() {
        if !visited.insert(value) {
            continue;
        }
        match function.entity(value).unwrap().definition {
            Definition::BlockParameter { block, index } if block != function.blocks[0].id => {
                let incoming = edges
                    .iter()
                    .filter(|(_, edge)| edge.target == block)
                    .collect::<Vec<_>>();
                assert!(!incoming.is_empty(), "no unanchored phi column");
                let inputs = incoming
                    .iter()
                    .map(|(_, edge)| edge.arguments[index])
                    .collect::<Vec<_>>();
                pending.extend(inputs.iter().copied());
                dependencies.insert(value, inputs);
            }
            Definition::InstructionResult { instruction, .. } => {
                if let Operation::Copy { source } =
                    function.instruction(instruction).unwrap().operation
                {
                    let source = EntityId::Value(source);
                    pending.push(source);
                    dependencies.insert(value, vec![source]);
                } else {
                    origins.insert(value);
                }
            }
            Definition::BlockParameter { .. } => {
                origins.insert(value);
            }
        }
    }
    // Ground every reached transport node, not merely the queried union. A
    // good root plus a separate rootless incoming SCC must still fail closed.
    let mut grounded = origins.clone();
    loop {
        let before = grounded.len();
        for (value, inputs) in &dependencies {
            if inputs.iter().any(|input| grounded.contains(input)) {
                grounded.insert(*value);
            }
        }
        if grounded.len() == before {
            break;
        }
    }
    assert_eq!(
        grounded, visited,
        "every incoming transport must reach a real leaf"
    );
    origins
}

fn count_break_instruction(
    function: &Function,
    select: impl Fn(&Instruction) -> bool,
) -> &Instruction {
    let found = function
        .instructions
        .iter()
        .filter(|instruction| select(instruction))
        .collect::<Vec<_>>();
    let [instruction] = found.as_slice() else {
        panic!("one exact source instruction")
    };
    instruction
}

pub(super) fn assert_count_break_structure(
    sources: &SourceMap,
    program: &Program,
    receiver_begin_span: lang_frontend::source::Span,
) {
    let scans = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| {
            sources
                .slice(function.origin.span())
                .unwrap()
                .starts_with("fun scan(")
        })
        .collect::<Vec<_>>();
    let [function] = scans.as_slice() else {
        panic!("one original scan")
    };
    let function = *function;
    let origin_is = |origin: &Origin, text| sources.slice(origin.span()) == Ok(text);
    let seed = count_break_instruction(function, |instruction| {
        matches!(instruction.operation, Operation::ContainerConstruct { .. })
            && origin_is(&instruction.origin, "listOf(\"first\")")
    })
    .results[0];
    let is_from = |value, expected| count_break_origins(function, value) == [expected].into();
    let length = count_break_instruction(function, |instruction| {
        matches!(instruction.operation, Operation::ContainerLength { .. })
    });
    let Operation::ContainerLength {
        owner: provider_loan,
    } = length.operation
    else {
        unreachable!()
    };
    let loan_from_owner = |instruction: &Instruction, owner| {
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = instruction.operation
        else {
            return false;
        };
        let Definition::InstructionResult { instruction, .. } =
            function.entity(EntityId::Place(place)).unwrap().definition
        else {
            return false;
        };
        matches!(
            function.instruction(instruction).unwrap().operation,
            Operation::RootPlace { owner: actual } if is_from(EntityId::Value(actual), owner)
        )
    };
    let provider_begin = count_break_instruction(function, |instruction| {
        instruction.results.as_slice() == [provider_loan]
    });
    assert!(loan_from_owner(provider_begin, seed));
    let receiver_begin = count_break_instruction(function, |instruction| {
        instruction.id != provider_begin.id && loan_from_owner(instruction, seed)
    });
    // Named receiver BorrowBegin uses its checked loan's receiver-expression span,
    // not the full call span used by RootPlace and the eventual RangeCall.
    assert_eq!(receiver_begin.origin, Origin::Source(receiver_begin_span));
    let receiver = receiver_begin.results[0];
    let element_place = count_break_instruction(function, |instruction| {
        matches!(
            instruction.operation,
            Operation::ContainerElementPlace { .. }
        )
    });
    let Operation::ContainerElementPlace {
        owner,
        index: element_index,
    } = element_place.operation
    else {
        unreachable!()
    };
    assert!(is_from(owner, provider_loan));
    let element = count_break_instruction(function, |instruction| {
        matches!(
            instruction.operation,
            Operation::BorrowBegin { place, kind: LoanKind::Shared }
                if EntityId::Place(place) == element_place.results[0]
        )
    })
    .results[0];
    let branches = function
        .blocks
        .iter()
        .filter(|block| {
            origin_is(
                &block.terminator.as_ref().unwrap().origin,
                "if(stop){break}else{1}",
            )
        })
        .collect::<Vec<_>>();
    let [branch] = branches.as_slice() else {
        panic!("one original stop branch")
    };
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &branch.terminator.as_ref().unwrap().kind
    else {
        panic!("stop controls the real branch")
    };
    assert_eq!(function.blocks[0].parameters.len(), 1);
    let module = &program.modules[function.id.module().index()];
    let condition_read = count_break_instruction(function, |instruction| {
        instruction.results.as_slice() == [EntityId::Value(*condition)]
    });
    let Operation::Read {
        source: PlaceAccess::Loan(stop),
    } = condition_read.operation
    else {
        panic!("the condition reads the original borrowed stop parameter")
    };
    assert_eq!(condition_read.block, branch.id);
    assert!(origin_is(&condition_read.origin, "stop"));
    let EntityType::Value(boolean) = function.entity(EntityId::Value(*condition)).unwrap().ty
    else {
        panic!("the condition is a Boolean value, not its borrowed storage")
    };
    assert_eq!(module.type_kind(boolean), Some(&SsaTypeKind::Boolean));
    let stop_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: boolean,
    };
    assert_eq!(function.entity(EntityId::Loan(stop)).unwrap().ty, stop_type);
    assert_eq!(
        function
            .entity(function.blocks[0].parameters[0])
            .unwrap()
            .ty,
        stop_type
    );
    assert!(is_from(
        EntityId::Loan(stop),
        function.blocks[0].parameters[0]
    ));
    assert_eq!(
        receiver_begin.block, branch.id,
        "receiver is evaluated before count"
    );
    let break_block = function.block(when_true.target).unwrap();
    let call_block = function.block(when_false.target).unwrap();
    let range_call = count_break_instruction(function, |instruction| {
        matches!(instruction.operation, Operation::RangeCall { .. })
    });
    assert_eq!(
        range_call.block, call_block.id,
        "only stop=false invokes prefix"
    );
    assert!(origin_is(
        &range_call.origin,
        "source.prefix(if(stop){break}else{1})"
    ));
    let Operation::RangeCall {
        callee,
        arguments,
        source,
    } = &range_call.operation
    else {
        unreachable!()
    };
    assert_eq!(arguments.len(), 2);
    assert_eq!(arguments[0], EntityId::Loan(*source));
    assert!(is_from(EntityId::Loan(*source), receiver));
    let count_loan = arguments[1];
    let count = count_break_instruction(function, |instruction| {
        matches!(
            instruction.operation,
            Operation::Constant(ScalarConstant::Integer(1))
        ) && origin_is(&instruction.origin, "1")
    });
    let count_begin = count_break_instruction(function, |instruction| {
        instruction.results.as_slice() == [count_loan]
    });
    assert_eq!(count.block, call_block.id);
    assert_eq!(count_begin.block, call_block.id);
    assert!(loan_from_owner(count_begin, count.results[0]));
    let EntityType::Value(integer) = function.entity(count.results[0]).unwrap().ty else {
        panic!("the original literal is an Int value")
    };
    assert_eq!(
        module.type_kind(integer),
        Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true
        })
    );
    let count_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: integer,
    };
    assert_eq!(function.entity(count_loan).unwrap().ty, count_type);
    let provider = program.modules[callee.module().index()]
        .function(*callee)
        .unwrap();
    assert!(origin_is(
        &provider.origin,
        "borrow fun <T> List<T>.prefix(count:Int):View<T> from this=rangeView(this,0,count)"
    ));
    assert_eq!(provider.carrier_return, Some(0));
    assert!(provider.receiver.is_none());
    assert_eq!(provider.blocks[0].parameters.len(), 2);
    assert_eq!(
        provider
            .entity(provider.blocks[0].parameters[1])
            .unwrap()
            .ty,
        count_type
    );
    let read = count_break_instruction(function, |instruction| {
        matches!(instruction.operation, Operation::DirectCall { .. })
    });
    assert_eq!(read.block, call_block.id, "only stop=false invokes read");
    let Operation::DirectCall {
        callee,
        receiver: None,
        arguments,
    } = &read.operation
    else {
        panic!("ordinary read consumer")
    };
    let consumer = program.modules[callee.module().index()]
        .function(*callee)
        .unwrap();
    assert!(origin_is(
        &consumer.origin,
        "fun read(view:View<String>):Unit{for(item in view){println(item)}}"
    ));
    let [EntityId::Loan(metadata)] = arguments.as_slice() else {
        panic!("one view metadata loan")
    };
    let metadata_begin = count_break_instruction(function, |instruction| {
        instruction.results.as_slice() == [EntityId::Loan(*metadata)]
    });
    assert!(loan_from_owner(metadata_begin, range_call.results[0]));
    let position = |block: &Block, id| {
        block
            .instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    let end = |block: &Block, seed| {
        count_break_instruction(function, |instruction| {
            instruction.block == block.id
                && matches!(instruction.operation,
                    Operation::BorrowEnd { loan } if is_from(EntityId::Loan(loan), seed))
        })
    };
    let break_receiver = end(break_block, receiver);
    let break_element = end(break_block, element);
    let break_source = end(break_block, provider_loan);
    assert!(position(break_block, break_receiver.id) < position(break_block, break_element.id));
    assert!(position(break_block, break_element.id) < position(break_block, break_source.id));
    assert_eq!(
        break_block
            .instructions
            .iter()
            .filter(|id| {
                matches!(
                    function.instruction(**id).unwrap().operation,
                    Operation::BorrowEnd { .. }
                )
            })
            .count(),
        3
    );
    let range_end = count_break_instruction(function, |instruction| {
        matches!(instruction.operation, Operation::RangeEnd { view, source }
            if EntityId::Value(view) == range_call.results[0]
                && EntityId::Loan(source) == range_call.results[1])
    });
    assert_eq!(range_end.block, call_block.id);
    let count_end = end(call_block, count_loan);
    let metadata_end = end(call_block, EntityId::Loan(*metadata));
    let receiver_end = end(call_block, receiver);
    let element_end = end(call_block, element);
    let order = [
        count.id,
        count_begin.id,
        range_call.id,
        count_end.id,
        read.id,
        metadata_end.id,
        range_end.id,
        receiver_end.id,
        element_end.id,
    ];
    assert!(
        order
            .windows(2)
            .all(|pair| { position(call_block, pair[0]) < position(call_block, pair[1]) })
    );
    let TerminatorKind::Branch(backedge) = &call_block.terminator.as_ref().unwrap().kind else {
        panic!("normal call resumes the same loop")
    };
    let header = function.block(backedge.target).unwrap();
    let TerminatorKind::Conditional {
        condition: has_next,
        when_true: body,
        when_false: exhausted,
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("provider guard")
    };
    assert_eq!(body.target, branch.id);
    let entry = &function.blocks[0];
    let TerminatorKind::Branch(first) = &entry.terminator.as_ref().unwrap().kind else {
        panic!("the original entry reaches the provider guard")
    };
    assert_eq!(first.target, header.id);
    assert_eq!(provider_begin.block, entry.id);
    assert_eq!(length.block, entry.id);
    let guard = count_break_instruction(function, |instruction| {
        instruction.results.as_slice() == [EntityId::Value(*has_next)]
    });
    let Operation::Compare {
        operator: ComparisonOperator::LessThan,
        left: cursor,
        right,
    } = guard.operation
    else {
        panic!("the real provider guard compares cursor against this source length")
    };
    assert_eq!(guard.block, header.id);
    assert!(is_from(EntityId::Value(right), length.results[0]));
    let incoming = |edge: &Edge, parameter| {
        let Definition::BlockParameter { block, index } =
            function.entity(parameter).unwrap().definition
        else {
            panic!("the exact edge carries this provider parameter")
        };
        assert_eq!(block, edge.target);
        edge.arguments[index]
    };
    let initial = incoming(first, EntityId::Value(cursor));
    let initial = count_break_instruction(function, |instruction| {
        instruction.results.as_slice() == [initial]
    });
    assert_eq!(
        initial.operation,
        Operation::Constant(ScalarConstant::Integer(0))
    );
    assert_eq!(
        incoming(body, EntityId::Value(element_index)),
        EntityId::Value(cursor)
    );
    let next = incoming(backedge, EntityId::Value(cursor));
    let next = count_break_instruction(function, |instruction| {
        instruction.results.as_slice() == [next]
    });
    let Operation::Binary {
        operator: BinaryOperator::Add,
        left,
        right,
    } = next.operation
    else {
        panic!("the successful iteration advances exactly this cursor")
    };
    assert_eq!(next.block, call_block.id);
    let body_cursor = incoming(when_false, EntityId::Value(left));
    assert_eq!(incoming(body, body_cursor), EntityId::Value(cursor));
    let step = count_break_instruction(function, |instruction| {
        instruction.results.as_slice() == [EntityId::Value(right)]
    });
    assert_eq!(
        step.operation,
        Operation::Constant(ScalarConstant::Integer(1))
    );
    let exhausted = function.block(exhausted.target).unwrap();
    let exhaustion_end = end(exhausted, provider_loan);
    let drops = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        2,
        "one source-owner drop per mutually exclusive exit"
    );
    for (block, source_end) in [(break_block, break_source), (exhausted, exhaustion_end)] {
        let drop = count_break_instruction(function, |instruction| {
            instruction.block == block.id
                && matches!(instruction.operation,
                    Operation::Drop { owner } if is_from(EntityId::Value(owner), seed))
        });
        assert!(position(block, source_end.id) < position(block, drop.id));
        let TerminatorKind::Branch(edge) = &block.terminator.as_ref().unwrap().kind else {
            panic!("cleanup reaches the common exit")
        };
        assert!(
            edge.arguments
                .iter()
                .all(|argument| !is_from(*argument, seed)),
            "dead owner is not transported after cleanup"
        );
    }
    let TerminatorKind::Branch(break_exit) = &break_block.terminator.as_ref().unwrap().kind else {
        unreachable!()
    };
    let TerminatorKind::Branch(exhaustion_exit) = &exhausted.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    assert_eq!(break_exit.target, exhaustion_exit.target);
    assert_ne!(
        break_exit.target, header.id,
        "stop=true exits rather than continuing"
    );
    let exit = function.block(break_exit.target).unwrap();
    assert!(exit.instructions.is_empty());
    assert!(matches!(
        &exit.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.is_empty()
    ));
}
