//! Original temporary Bundle field read: exact frontend cleanup and SSA ownership.
use crate::ssa::model::{EntityId, Function, Operation, Program, ScalarConstant, TerminatorKind};
use lang_frontend::{
    ownership_checking::{UnitDropPoint, UnitDropTarget, ValidatedCompilationUnitOwnership},
    parser::ParsedFile,
    source::SourceMap,
    type_checking::ValidatedCompilationUnitTypes,
};

pub(super) fn assert_frontend(
    sources: &SourceMap,
    parsed: &ParsedFile,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
) {
    let constructions = typed
        .types()
        .constructions()
        .iter()
        .filter(|descriptor| {
            sources.slice(
                parsed
                    .ast()
                    .expressions()
                    .get(descriptor.expression().expression())
                    .unwrap()
                    .span(),
            ) == Ok("Bundle(\"owned\", 7)")
        })
        .collect::<Vec<_>>();
    let [construction] = constructions.as_slice() else {
        panic!("one original temporary constructor")
    };
    let id = construction.expression();
    let span = parsed
        .ast()
        .expressions()
        .get(id.expression())
        .unwrap()
        .span();
    let drops = owned
        .ownership()
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Temporary(id))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one exact parent cleanup fact")
    };
    assert_eq!(drop.point(), UnitDropPoint::AfterExpression(id));
    assert_eq!(drop.value_origin(), span);
}

pub(super) fn assert_ssa(program: &Program, function: &Function) {
    crate::ssa::verify::verify_program(program).unwrap();
    let [block] = function.blocks.as_slice() else {
        panic!("original temporary fixture has one block")
    };
    assert_eq!(
        block.instructions,
        function
            .instructions
            .iter()
            .map(|instruction| instruction.id)
            .collect::<Vec<_>>(),
        "instruction positions follow the actual block order"
    );
    assert!(function.name.contains(".temporary."), "{}", function.name);
    let allocations = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            if let Operation::HeapAllocate { payload, .. } = instruction.operation {
                let [EntityId::Value(owner)] = instruction.results.as_slice() else {
                    panic!("heap owner")
                };
                Some((index, *owner, payload))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [(allocation, owner, payload)] = allocations.as_slice() else {
        panic!("one parent")
    };
    let aggregate = function
        .instructions
        .iter()
        .find(|instruction| instruction.results == [EntityId::Value(*payload)])
        .unwrap();
    let Operation::AggregateConstruct { fields, .. } = &aggregate.operation else {
        panic!("actual Bundle payload")
    };
    let [text, count] = fields.as_slice() else {
        panic!("two Bundle fields")
    };
    assert!(function.instructions.iter().any(|instruction| instruction.results == [EntityId::Value(*text)] && matches!(&instruction.operation, Operation::StringLiteral { bytes, .. } if bytes == b"owned")));
    assert!(
        function
            .instructions
            .iter()
            .any(
                |instruction| instruction.results == [EntityId::Value(*count)]
                    && instruction.operation == Operation::Constant(ScalarConstant::Integer(7))
            )
    );
    let reads = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            if let Operation::HeapFieldRead { receiver, field } = instruction.operation {
                let [EntityId::Value(value)] = instruction.results.as_slice() else {
                    panic!("copied field")
                };
                Some((index, receiver, field, *value))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [(read, loan, field, value)] = reads.as_slice() else {
        panic!("one actual field read")
    };
    assert_eq!(*field, 1);
    let begin = function
        .instructions
        .iter()
        .find(|instruction| instruction.results == [EntityId::Loan(*loan)])
        .unwrap();
    let Operation::BorrowBegin {
        place,
        kind: crate::ssa::model::LoanKind::Shared,
    } = begin.operation
    else {
        panic!("shared receiver loan")
    };
    assert!(
        function
            .instructions
            .iter()
            .any(
                |instruction| instruction.results == [EntityId::Place(place)]
                    && instruction.operation == Operation::RootPlace { owner: *owner }
            )
    );
    let ends = function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, instruction)| instruction.operation == Operation::BorrowEnd { loan: *loan })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let [end] = ends.as_slice() else {
        panic!("one receiver loan end")
    };
    let drops = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| match instruction.operation {
            Operation::Drop { owner } => Some((index, owner)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(drop, dropped)] = drops.as_slice() else {
        panic!("only the parent is dropped; its String has been delivered")
    };
    assert_eq!(dropped, owner);
    assert!(allocation < read && read < end && end < drop);
    let returns = function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .filter_map(|term| match &term.kind {
            TerminatorKind::Return { values } => Some(values),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(returns.len(), 1);
    assert_eq!(returns[0].as_slice(), &[*value]);
    crate::llvm::render_verified_program(program).unwrap();
}
