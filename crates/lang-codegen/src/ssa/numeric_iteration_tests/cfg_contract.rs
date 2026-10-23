//! Exact endpoint recurrence in ordinary-source SSA; no execution or fabricated facts.
use lang_frontend::source::Span;

use super::verify_program;
use crate::ssa::{
    integer_range_value_tests::collect_source_functions,
    model::{
        BinaryOperator, Block, BlockId, ComparisonOperator, Definition, Edge, EntityId, EntityType,
        Function, Instruction, Module, Operation, Origin, Program, ScalarConstant, SsaTypeId,
        SsaTypeKind, TerminatorKind, ValueId,
    },
};

#[test]
fn numeric_snapshot_and_terminal_edges_preserve_exact_endpoints() {
    for (integer, range, bits) in [("Int", "IntRange", 32), ("Long", "LongRange", 64)] {
        for finish in ["", "continue"] {
            let source = format!(
                "fun make(a:{integer},b:{integer}):{range}{{println(\"source\");return a..b}}\n\
                 fun entry(a:{integer},b:{integer}):Unit{{for(item in make(a,b)){{val copy:{integer}=item;{finish}}}}}"
            );
            let programs = collect_source_functions(&[source]);
            assert_eq!(
                programs.len(),
                2,
                "both original source layouts use the Unit entry"
            );
            for source in programs {
                let source = source.expect("ordinary numeric iteration reaches SSA");
                verify_program(&source.program).expect("ordinary numeric iteration SSA verifies");
                assert_endpoint_cfg(&source.program, bits, &source.functions);
            }
        }
    }
}

fn assert_endpoint_cfg(program: &Program, bits: u16, functions: &[(String, Span)]) {
    let module = only(program.modules.iter(), "one ordinary source module");
    let span = |name: &str| {
        only(
            functions.iter().filter(|(actual, _)| actual == name),
            "one exact function declaration in the actual lowered source",
        )
        .1
    };
    let make = source_function(module, span("make"));
    let function = source_function(module, span("entry"));
    let call = only(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. })),
        "entry evaluates its source through exactly one direct call",
    );
    let Operation::DirectCall { callee, .. } = call.operation else {
        unreachable!("direct call was selected");
    };
    assert_eq!(
        callee, make.id,
        "the sole source call targets the real make FunctionId"
    );
    let snapshot = result(call);
    let range_type = value_type(function, snapshot);
    let endpoint_type = module
        .integer_range_endpoint(range_type)
        .expect("call result has the registered numeric Range identity");
    assert!(module.valid_integer_range_type(range_type, endpoint_type));
    assert_eq!(
        module.type_kind(endpoint_type),
        Some(&SsaTypeKind::Integer { bits, signed: true })
    );

    let projections = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::AggregateProject { .. }))
        .collect::<Vec<_>>();
    assert_eq!(
        projections.len(),
        3,
        "the source has exactly three snapshot fields"
    );
    let [first, last, empty] = [0, 1, 2].map(|field| {
        let projection = only(
            projections.iter().copied().filter(|instruction| {
                matches!(instruction.operation,
                    Operation::AggregateProject { aggregate, field: actual }
                        if aggregate == snapshot && actual == field)
            }),
            "each exact field is projected once from the same call result",
        );
        assert_eq!(projection.block, call.block);
        assert!(
            call.id.index() < projection.id.index(),
            "source precedes its projections"
        );
        result(projection)
    });
    assert_eq!(value_type(function, first), endpoint_type);
    assert_eq!(value_type(function, last), endpoint_type);
    assert_eq!(
        module.type_kind(value_type(function, empty)),
        Some(&SsaTypeKind::Boolean)
    );

    let preheader = call.block;
    assert_eq!(function.entry_block(), Some(preheader));
    assert!(
        incoming(function, preheader).is_empty(),
        "source cannot be reevaluated by a backedge"
    );
    let (empty_test, empty_edge, body_edge) = conditional(function, preheader);
    assert_eq!(
        empty_test, empty,
        "the stored empty bit selects the zero-round edge"
    );
    let body = body_edge.target;
    let exhausted = empty_edge.target;
    assert_ne!(body, exhausted);
    let current_slot = only(
        body_edge
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, value)| (*value == EntityId::Value(first)).then_some(index)),
        "the first body edge transports exactly the snapshot first endpoint",
    );
    let body_current = value(function.block(body).unwrap().parameters[current_slot]);

    let comparison = only(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Compare { .. })),
        "there is exactly one terminal comparison",
    );
    let Operation::Compare {
        operator,
        left: current,
        right: terminal_last,
    } = comparison.operation
    else {
        unreachable!("comparison was selected");
    };
    assert_eq!(
        operator,
        ComparisonOperator::Equal,
        "termination tests current == last"
    );
    assert_eq!(value_type(function, current), endpoint_type);
    assert_eq!(value_type(function, terminal_last), endpoint_type);
    let terminal = comparison.block;
    // Single-file uses the body directly; unit carries the cleaned state through
    // one unconditional continuation edge. No general graph walk is needed here.
    let continuation = if terminal == body {
        None
    } else {
        let edge = branch(function, body);
        assert_eq!(edge.target, terminal);
        assert_eq!(incoming(function, terminal), vec![(body, edge)]);
        Some(edge)
    };
    let body_source =
        |endpoint| continuation.map_or(endpoint, |edge| before_edge(function, endpoint, edge));
    assert_eq!(
        body_source(current),
        body_current,
        "terminal current is this body's current slot"
    );
    let body_last = body_source(terminal_last);
    assert_eq!(
        before_edge(function, body_last, body_edge),
        last,
        "terminal last originates from field 1 of the same source snapshot"
    );
    let (terminal_test, final_edge, increment_edge) = conditional(function, terminal);
    assert_eq!(terminal_test, result(comparison));
    assert_eq!(
        final_edge.target, exhausted,
        "reaching last exits without incrementing"
    );
    let increment = increment_edge.target;
    assert_ne!(increment, exhausted);
    assert_ne!(increment, body);
    assert_eq!(
        incoming(function, increment),
        vec![(terminal, increment_edge)],
        "the terminal false edge is the increment block's sole entrance"
    );

    let addition = only(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Binary { .. })),
        "there is exactly one low-level cursor operation",
    );
    let Operation::Binary {
        operator,
        left,
        right,
    } = addition.operation
    else {
        unreachable!("binary operation was selected");
    };
    assert_eq!(operator, BinaryOperator::Add);
    assert_eq!(addition.block, increment);
    assert_eq!(
        before_edge(function, left, increment_edge),
        current,
        "increment reads the exact compared current, including unit rebinding"
    );
    let one = defining_instruction(function, right);
    assert!(matches!(
        one.operation,
        Operation::Constant(ScalarConstant::Integer(1))
    ));
    assert_eq!(one.block, increment);
    assert!(one.id.index() < addition.id.index());
    assert_eq!(value_type(function, right), endpoint_type);
    let next = result(addition);
    assert_eq!(value_type(function, next), endpoint_type);
    let backedge = branch(function, increment);
    assert_eq!(backedge.target, body);
    assert_eq!(backedge.arguments[current_slot], EntityId::Value(next));
    assert_eq!(
        before_edge(
            function,
            before_edge(function, body_last, backedge),
            increment_edge
        ),
        terminal_last,
        "the backedge preserves the exact last endpoint instead of substituting current"
    );
    assert_eq!(
        incoming(function, body),
        vec![(preheader, body_edge), (increment, backedge)],
        "only first and next feed the body current slot"
    );
    assert_eq!(
        incoming(function, exhausted),
        vec![(preheader, empty_edge), (terminal, final_edge)],
        "zero-round and reached-last share the exhaustion exit"
    );
    assert!(matches!(
        terminator(function, exhausted),
        TerminatorKind::Return { values } if values.is_empty()
    ));
    assert_eq!(
        function.blocks.len(),
        4 + usize::from(continuation.is_some())
    );
    assert!(
        function.instructions.iter().all(|instruction| !matches!(
            instruction.operation,
            Operation::BorrowBegin { .. } | Operation::ContainerLength { .. }
        )),
        "numeric snapshot and element create no provider loan or Int length"
    );
}

fn source_function(module: &Module, span: Span) -> &Function {
    only(
        module.functions.iter().filter(
            |function| matches!(&function.origin, Origin::Source(actual) if *actual == span),
        ),
        "the full original declaration identifies exactly one SSA function",
    )
}

fn only<T>(mut values: impl Iterator<Item = T>, message: &str) -> T {
    let result = values.next().expect(message);
    assert!(values.next().is_none(), "{message}");
    result
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("numeric endpoints must be owned Values");
    };
    value
}

fn result(instruction: &Instruction) -> ValueId {
    value(only(
        instruction.results.iter().copied(),
        "exactly one instruction result",
    ))
}

fn value_type(function: &Function, value: ValueId) -> SsaTypeId {
    let EntityType::Value(ty) = function.entity(EntityId::Value(value)).unwrap().ty else {
        panic!("numeric value has a Value type");
    };
    ty
}

fn defining_instruction(function: &Function, value: ValueId) -> &Instruction {
    let Definition::InstructionResult {
        instruction,
        index: 0,
    } = function.entity(EntityId::Value(value)).unwrap().definition
    else {
        panic!("constant is a direct instruction result");
    };
    function.instruction(instruction).unwrap()
}

fn terminator(function: &Function, block: BlockId) -> &TerminatorKind {
    &function
        .block(block)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
}

fn conditional(function: &Function, block: BlockId) -> (ValueId, &Edge, &Edge) {
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = terminator(function, block)
    else {
        panic!("the endpoint guard must be a conditional terminator");
    };
    (*condition, when_true, when_false)
}

fn branch(function: &Function, block: BlockId) -> &Edge {
    let TerminatorKind::Branch(edge) = terminator(function, block) else {
        panic!("the cursor continuation must be an unconditional branch");
    };
    edge
}

fn edges(block: &Block) -> Vec<&Edge> {
    match &block.terminator.as_ref().unwrap().kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::Return { .. } => Vec::new(),
        _ => panic!("this successful scalar fixture has no other exit kinds"),
    }
}

fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            edges(block)
                .into_iter()
                .filter(move |edge| edge.target == target)
                .map(move |edge| (block.id, edge))
        })
        .collect()
}

/// Translate one known CFG edge only; a dominating Value retains its identity.
fn before_edge(function: &Function, endpoint: ValueId, edge: &Edge) -> ValueId {
    match function
        .entity(EntityId::Value(endpoint))
        .unwrap()
        .definition
    {
        Definition::BlockParameter { block, index } if block == edge.target => {
            value(edge.arguments[index])
        }
        _ => endpoint,
    }
}
