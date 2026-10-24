//! Strong source-to-SSA identity oracle shared by readonly field-copy and call tests.

use super::ResourceUnit;
use crate::ssa::model::{
    EntityId, EntityType, Function, LoanKind, Operation, PlaceAccess, SsaTypeKind,
};

pub(super) fn assert_nested_this_field_chain(
    unit: &ResourceUnit,
    body: &Function,
    expression: &str,
    fields: &[usize],
    callee: &Function,
    copied: bool,
) {
    let block = body.block(body.entry_block().unwrap()).unwrap();
    let [EntityId::Loan(receiver)] = block.parameters.as_slice() else {
        panic!("the destructor has only its original shared this loan");
    };
    let Some(EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    }) = body.receiver()
    else {
        panic!("the registered destructor has a shared receiver");
    };
    assert_eq!(
        body.entity(EntityId::Loan(*receiver)).unwrap().ty,
        body.receiver().unwrap()
    );
    assert_eq!(unit.module().deinit(target), Some(body.id()));
    let instructions = block
        .instructions
        .iter()
        .map(|id| body.instruction(*id).unwrap())
        .collect::<Vec<_>>();
    let calls = instructions.iter().enumerate().filter(|(_, instruction)| matches!(
        instruction.operation, Operation::DirectCall { callee: target, .. } if target == callee.id()
    )).collect::<Vec<_>>();
    let [(call_index, call)] = calls.as_slice() else {
        panic!("one actual call to the original source helper");
    };
    let start = call_index
        .checked_sub(if copied {
            fields.len() * 2 + 1
        } else {
            fields.len()
        })
        .expect("projection chain precedes its call");
    let mut parent = *receiver;
    let mut parent_type = target;
    let mut loans = Vec::new();
    for (offset, field) in fields.iter().enumerate() {
        let instruction = instructions[start + offset];
        assert!(matches!(instruction.operation,
            Operation::SharedHeapFieldLoan { base, field: actual } if base == parent && actual == *field));
        assert_eq!(
            unit.source.sources().slice(instruction.origin.span()),
            Ok(expression)
        );
        let [EntityId::Loan(child)] = instruction.results.as_slice() else {
            panic!("each projection creates exactly one actual child loan");
        };
        let Some(SsaTypeKind::HeapOwner {
            payload: Some(payload),
            ..
        }) = unit.module().type_kind(parent_type)
        else {
            panic!("each parent is the previous concrete ordinary class owner");
        };
        let Some(SsaTypeKind::Aggregate { fields, .. }) = unit.module().type_kind(*payload) else {
            panic!("ordinary class has its concrete payload");
        };
        parent_type = fields[*field];
        assert_eq!(
            body.entity(EntityId::Loan(*child)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: parent_type
            }
        );
        loans.push(*child);
        parent = *child;
    }
    let argument = if copied {
        let read = instructions[start + fields.len()];
        assert_eq!(
            read.operation,
            Operation::Read {
                source: PlaceAccess::Loan(parent)
            }
        );
        assert_eq!(
            unit.source.sources().slice(read.origin.span()),
            Ok(expression)
        );
        let [value @ EntityId::Value(_)] = read.results.as_slice() else {
            panic!("the leaf read alone produces the owned copy");
        };
        assert_eq!(
            body.entity(*value).unwrap().ty,
            EntityType::Value(parent_type)
        );
        *value
    } else {
        EntityId::Loan(parent)
    };
    assert!(
        matches!(&call.operation, Operation::DirectCall { receiver: None, arguments, .. }
        if arguments.as_slice() == [argument])
    );
    let end_start = if copied {
        start + fields.len() + 1
    } else {
        call_index + 1
    };
    for (offset, loan) in loans.iter().rev().enumerate() {
        assert_eq!(
            instructions[end_start + offset].operation,
            Operation::BorrowEnd { loan: *loan }
        );
        assert_eq!(
            body.instructions
                .iter()
                .filter(|instruction| instruction.operation == Operation::BorrowEnd { loan: *loan })
                .count(),
            1
        );
    }
    assert!(
        !body
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation,
        Operation::BorrowEnd { loan } if loan == *receiver))
    );
    assert!(!body.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::Drop { .. }
            | Operation::Consume { .. }
            | Operation::HeapAllocate { .. }
            | Operation::HeapPayloadPlace { .. }
    )));
    assert!(
        !body
            .instructions
            .iter()
            .flat_map(|instruction| &instruction.results)
            .any(
                |result| matches!(body.entity(*result).unwrap().ty, EntityType::Value(ty)
            if matches!(unit.module().type_kind(ty), Some(SsaTypeKind::HeapOwner { .. })))
            )
    );
}
