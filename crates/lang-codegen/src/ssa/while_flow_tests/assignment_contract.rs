//! Read-only identities for the original integer compound source in both lowering modes.
use lang_frontend::{
    lexer::lex,
    parser::{AssignmentOperator, BinaryOperator, Expression, LiteralKind, parse_file},
    source::{SourceMap, Span},
};

use crate::ssa::model::{
    BlockId, CheckedArithmeticOperator, Definition, EntityId, EntityType, Function, Instruction,
    Operation, Origin, Ownership, Program, ScalarConstant, SsaTypeKind, TerminatorKind, ValueId,
};

pub(super) fn assert_compound(program: &Program, source: &str, unit: bool) {
    let [zero_span, assignment_span, sum_span, return_span] = source_spans(source, unit);
    let [module] = program.modules.as_slice() else {
        panic!("one original source module")
    };
    let [function] = module.functions.as_slice() else {
        panic!("only the actual entry is generated")
    };
    assert_eq!(function.receiver, None);
    assert_eq!(function.borrow_return, None);
    assert_eq!(function.carrier_return, None);
    let entry = function.block(function.entry_block().unwrap()).unwrap();
    let [EntityId::Value(number)] = entry.parameters.as_slice() else {
        panic!("the original own number is the sole value parameter")
    };
    let number = *number;
    let parameter = function.entity(EntityId::Value(number)).unwrap();
    let EntityType::Value(integer) = parameter.ty else {
        panic!("number is a value, not a borrowed or place identity")
    };
    assert_eq!(
        module.type_kind(integer),
        Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true
        })
    );
    assert_eq!(module.type_ownership(integer), Some(Ownership::Copyable));
    assert_eq!(function.return_types, [integer]);
    assert_eq!(
        parameter.definition,
        Definition::BlockParameter {
            block: entry.id,
            index: 0
        }
    );
    let [zero, first, second] = function.instructions.as_slice() else {
        panic!("the source has one initializer and exactly two checked additions")
    };
    assert_eq!(
        zero.operation,
        Operation::Constant(ScalarConstant::Integer(0))
    );
    assert_source(&zero.origin, zero_span, function);
    let [EntityId::Value(initial)] = zero.results.as_slice() else {
        panic!("the real initializer produces one value")
    };
    assert_eq!(
        function.entity(EntityId::Value(*initial)).unwrap().ty,
        parameter.ty
    );
    assert_eq!(zero.block, entry.id);
    assert_eq!(first.block, entry.id);
    assert_eq!(entry.instructions, [zero.id, first.id]);
    let (assigned, normal) = checked_success(function, first, *initial, number, assignment_span);
    assert_eq!(
        second.block, normal,
        "the return sum follows compound success"
    );
    assert_eq!(function.block(normal).unwrap().instructions, [second.id]);
    let (returned, done) = checked_success(function, second, assigned, number, sum_span);
    for instruction in [first, second] {
        let [value, failure] = instruction.results.as_slice() else {
            panic!("checked arithmetic has value and failure results")
        };
        assert_eq!(function.entity(*value).unwrap().ty, parameter.ty);
        let EntityType::Value(boolean) = function.entity(*failure).unwrap().ty else {
            panic!("the checked failure is a Boolean value")
        };
        assert_eq!(module.type_kind(boolean), Some(&SsaTypeKind::Boolean));
    }
    let done = function.block(done).unwrap();
    assert!(done.instructions.is_empty());
    let terminator = done.terminator.as_ref().unwrap();
    let TerminatorKind::Return { values } = &terminator.kind else {
        panic!("the second normal edge returns its actual checked result")
    };
    assert_eq!(values.as_slice(), &[returned]);
    assert_source(&terminator.origin, return_span, function);
    assert_eq!(
        function.blocks.len(),
        5,
        "entry, two failure and two success blocks"
    );
}

fn checked_success(
    function: &Function,
    instruction: &Instruction,
    left: ValueId,
    right: ValueId,
    span: Span,
) -> (ValueId, BlockId) {
    assert_eq!(
        instruction.operation,
        Operation::CheckedArithmetic {
            operator: CheckedArithmeticOperator::Add,
            left,
            right
        }
    );
    assert_source(&instruction.origin, span, function);
    let [EntityId::Value(value), EntityId::Value(failure)] = instruction.results.as_slice() else {
        panic!("the original checked addition produces value and failure identities")
    };
    let block = function.block(instruction.block).unwrap();
    let terminator = block.terminator.as_ref().unwrap();
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &terminator.kind
    else {
        panic!("checked overflow is tested before the normal continuation")
    };
    assert_eq!(condition, failure);
    assert_source(&terminator.origin, span, function);
    assert_ne!(when_true.target, when_false.target);
    assert!(when_true.arguments.is_empty());
    assert!(when_false.arguments.is_empty());
    let failure_block = function.block(when_true.target).unwrap();
    assert!(failure_block.parameters.is_empty());
    assert!(failure_block.instructions.is_empty());
    let abort = failure_block.terminator.as_ref().unwrap();
    assert_eq!(abort.kind, TerminatorKind::Abort);
    assert_source(&abort.origin, span, function);
    assert!(
        function
            .block(when_false.target)
            .unwrap()
            .parameters
            .is_empty()
    );
    (*value, when_false.target)
}

fn source_spans(source: &str, unit: bool) -> [Span; 4] {
    let text = if unit {
        format!("package p\n{source}")
    } else {
        source.to_owned()
    };
    let mut sources = SourceMap::new();
    let id = sources
        .add_source(if unit { "p/range.ko" } else { "range.ko" }, &text)
        .unwrap();
    let lexed = lex(&sources, id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let span = |predicate: fn(&Expression) -> bool| {
        let mut nodes = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, node)| predicate(node.payload()).then_some(node.span()));
        let span = nodes.next().expect("one real source node");
        assert!(nodes.next().is_none(), "the source identity is unique");
        span
    };
    [
        span(|node| matches!(node, Expression::Literal(LiteralKind::Integer(_)))),
        span(|node| {
            matches!(
                node,
                Expression::Assignment {
                    operator: AssignmentOperator::AddAssign,
                    ..
                }
            )
        }),
        span(|node| {
            matches!(
                node,
                Expression::Binary {
                    operator: BinaryOperator::Add,
                    ..
                }
            )
        }),
        span(|node| matches!(node, Expression::Return { .. })),
    ]
}

fn assert_source(origin: &Origin, expected: Span, function: &Function) {
    let Origin::Source(actual) = origin else {
        panic!("the original operation retains its real source origin")
    };
    // The independent parse supplies coordinates, not transferable SourceMap identities.
    assert_eq!(
        (actual.start(), actual.end()),
        (expected.start(), expected.end())
    );
    assert_eq!(actual.source_id(), function.origin.span().source_id());
}
