//! Query-only synchronous owned-local field loan identities.
use crate::ssa::model::{
    EntityId, EntityType, LoanKind, Operation, PlaceAccess, Program, SsaTypeKind,
};

pub(super) fn assert_field_loan(program: &Program) {
    let module = program.modules.first().expect("one module");
    assert_eq!(program.modules.len(), 1);
    let functions = module
        .functions
        .iter()
        .filter(|function| {
            function.instructions.iter().any(|instruction| {
                matches!(instruction.operation, Operation::HeapPayloadPlace { .. })
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(functions.len(), 1, "the entry's ordinary owned field loan");
    let function = functions[0];
    let root =
        only(function.instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::HeapPayloadPlace { .. })
        }));
    let Operation::HeapPayloadPlace { owner } = root.operation else {
        unreachable!()
    };
    let EntityType::Value(owner_type) = function.entity(EntityId::Value(owner)).unwrap().ty else {
        panic!("owned root")
    };
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(owner_type)
    else {
        panic!("registered heap owner")
    };
    let base = only(root.results.iter().copied());
    assert_eq!(
        function.entity(base).unwrap().ty,
        EntityType::Place(*payload)
    );
    let field = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::FieldPlace { base: operand, field: 0 } if EntityId::Place(operand) == base)));
    let field_place = only(field.results.iter().copied());
    let EntityType::Place(field_type) = function.entity(field_place).unwrap().ty else {
        panic!("field place")
    };
    assert!(matches!(
        module.type_kind(field_type),
        Some(SsaTypeKind::StringOwner)
    ));
    let begin = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { place, kind: LoanKind::Shared } if EntityId::Place(place) == field_place)));
    let EntityId::Loan(loan) = only(begin.results.iter().copied()) else {
        panic!("shared field loan")
    };
    assert_eq!(
        function.entity(EntityId::Loan(loan)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: field_type
        }
    );
    let read = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::PrintString { value } if value == loan)));
    let end = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan)));
    let drop = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::Drop { owner: dropped } if dropped == owner)));
    let block = function.block(root.block).unwrap();
    let mut last = None;
    for instruction in [root, field, begin, read, end, drop] {
        assert_eq!(instruction.block, block.id);
        let index = block
            .instructions
            .iter()
            .position(|id| *id == instruction.id)
            .unwrap();
        if let Some(last) = last {
            assert!(last < index);
        }
        last = Some(index);
    }
    assert!(!function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::Read { source: PlaceAccess::Loan(source) } if source == loan)), "String is borrowed rather than copied out");
}
fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one actual identity");
    assert!(values.next().is_none(), "identity is unique");
    value
}
