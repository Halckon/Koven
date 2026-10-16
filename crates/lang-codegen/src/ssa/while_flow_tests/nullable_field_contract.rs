//! Query-only identities for the three direct nullable-field call fixtures.
use std::collections::BTreeSet;

use crate::ssa::model::{
    Definition, EntityId, EntityType, LoanKind, Operation, Program, SsaTypeKind, TerminatorKind,
};

pub(super) fn assert_field_child(program: &Program, returned: bool) {
    let module = only(program.modules.iter());
    let function = only(module.functions.iter().filter(|function| {
        function.blocks.iter().any(|block| {
            matches!(
                block.terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::NullableBranch { .. })
            )
        })
    }));
    let header = only(function.blocks.iter().filter(|block| {
        matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::NullableBranch { .. })
        )
    }));
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = &header.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    let body = function.block(when_non_null.target).unwrap();
    let absent = function.block(when_null.target).unwrap();
    let owner_column = only(
        when_non_null
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, entity)| (*entity == EntityId::Value(*owner)).then_some(index)),
    );
    let EntityId::Value(body_owner) = body.parameters[owner_column] else {
        panic!("the actual true-edge owner column")
    };
    assert_eq!(body.parameters.len(), when_non_null.arguments.len() + 1);
    assert_eq!(body.parameters.last(), Some(&EntityId::Loan(*view)));
    assert_eq!(absent.parameters.len(), when_null.arguments.len());
    assert!(!when_non_null.arguments.contains(&EntityId::Loan(*view)));
    assert!(!when_null.arguments.contains(&EntityId::Loan(*view)));
    assert_eq!(
        function.entity(EntityId::Loan(*view)).unwrap().definition,
        Definition::BlockParameter {
            block: body.id,
            index: body.parameters.len() - 1,
        }
    );
    let Definition::BlockParameter { block, index } =
        function.entity(EntityId::Value(*owner)).unwrap().definition
    else {
        panic!("the guard reads its real loop-header owner parameter")
    };
    assert_eq!(block, header.id);
    assert_eq!(header.parameters[index], EntityId::Value(*owner));
    let EntityType::Value(nullable) = function.entity(EntityId::Value(*owner)).unwrap().ty else {
        panic!("owned nullable storage")
    };
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
        panic!("registered nullable handle")
    };
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(*inner)
    else {
        panic!("registered ordinary class payload")
    };
    let Some(SsaTypeKind::Aggregate { fields, .. }) = module.type_kind(*payload) else {
        panic!("actual heap payload fields")
    };
    let (field_index, string) = only(
        fields
            .iter()
            .enumerate()
            .filter(|(_, ty)| module.type_kind(**ty) == Some(&SsaTypeKind::StringOwner)),
    );
    assert_eq!(
        function.entity(EntityId::Value(body_owner)).unwrap().ty,
        EntityType::Value(nullable)
    );
    assert_eq!(
        function.entity(EntityId::Loan(*view)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: *inner,
        }
    );

    let projection = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::SharedHeapFieldLoan { .. })
    }));
    assert!(matches!(
        projection.operation,
        Operation::SharedHeapFieldLoan { base, field }
            if base == *view && field == field_index
    ));
    let EntityId::Loan(child) = only(projection.results.iter().copied()) else {
        panic!("the field projection creates exactly one child loan")
    };
    assert_ne!(child, *view);
    let child_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: *string,
    };
    assert_eq!(function.entity(EntityId::Loan(child)).unwrap().ty, child_type);
    let call = only(function.instructions.iter().filter(|instruction| {
        matches!(&instruction.operation, Operation::DirectCall { arguments, .. }
            if arguments.contains(&EntityId::Loan(child)))
            || matches!(instruction.operation, Operation::PrintString { value } if value == child)
    }));
    match &call.operation {
        Operation::DirectCall {
            callee,
            receiver,
            arguments,
        } => {
            assert!(
                returned,
                "the returned fixture calls its synchronous Borrow callee"
            );
            assert_eq!(*receiver, None);
            assert_eq!(arguments.as_slice(), [EntityId::Loan(child)]);
            let callee = module.function(*callee).unwrap();
            assert_eq!(callee.borrow_return, None);
            assert_eq!(callee.carrier_return, None);
            let entry = callee.block(callee.entry_block().unwrap()).unwrap();
            let parameter = only(entry.parameters.iter().copied());
            assert!(matches!(parameter, EntityId::Loan(_)));
            assert_eq!(callee.entity(parameter).unwrap().ty, child_type);
        }
        Operation::PrintString { value } => {
            assert!(!returned, "the direct and Group fixtures print the same child");
            assert_eq!(*value, child);
        }
        _ => unreachable!(),
    }
    let child_end = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == child)
    }));
    let parent_end = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == *view)
    }));
    let owner_drop = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::Drop { owner } if owner == body_owner)
    }));

    // These three bodies contain no argument/control-flow split: the actual birth
    // edge supplies the current owner and parent until this synchronous call ends.
    let mut previous = None;
    for instruction in [projection, call, child_end, parent_end, owner_drop] {
        assert_eq!(instruction.block, body.id);
        let position = body
            .instructions
            .iter()
            .position(|id| *id == instruction.id)
            .unwrap();
        if let Some(previous) = previous {
            assert!(
                previous < position,
                "child, call, child end, parent end, owner drop"
            );
        }
        previous = Some(position);
    }
    let child_users = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction
                .operation
                .entities()
                .contains(&EntityId::Loan(child))
        })
        .map(|instruction| instruction.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(child_users, BTreeSet::from([call.id, child_end.id]));
    let terminator = &body.terminator.as_ref().unwrap().kind;
    assert!(!terminator.entities().contains(&EntityId::Loan(child)));
    assert!(!terminator.entities().contains(&EntityId::Loan(*view)));
    if returned {
        let EntityId::Value(result) = only(call.results.iter().copied()) else {
            panic!("the synchronous call returns an ordinary value")
        };
        assert!(matches!(terminator, TerminatorKind::Return { values }
            if values.as_slice() == [result]));
    } else {
        assert!(matches!(terminator, TerminatorKind::Branch(edge)
            if edge.target != header.id));
    }

    for instruction in &function.instructions {
        assert!(!matches!(
            instruction.operation,
            Operation::StringClone { .. }
                | Operation::Read { .. }
                | Operation::HeapFieldRead { .. }
                | Operation::NullableTake { .. }
                | Operation::NullableWrap { .. }
        ));
        assert!(
            instruction.results.iter().all(|entity| {
                function.entity(*entity).unwrap().ty != EntityType::Value(*string)
            }),
            "the borrowed String field never becomes a copied-out owner"
        );
    }
    // Inspect every reachable null-side block, including the shared loop exit.
    // A unique projection alone would not show that it is excluded from this edge.
    let mut pending = vec![absent.id];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        assert_ne!(id, body.id, "the null edge cannot reach the child birth");
        let block = function.block(id).unwrap();
        for loan in [*view, child] {
            assert!(!block.parameters.contains(&EntityId::Loan(loan)));
            for id in &block.instructions {
                let instruction = function.instruction(*id).unwrap();
                assert!(
                    !instruction
                        .operation
                        .entities()
                        .contains(&EntityId::Loan(loan))
                );
                assert!(!instruction.results.contains(&EntityId::Loan(loan)));
            }
            assert!(
                !block
                    .terminator
                    .as_ref()
                    .unwrap()
                    .kind
                    .entities()
                    .contains(&EntityId::Loan(loan))
            );
        }
        pending.extend(block.terminator.as_ref().unwrap().kind.targets());
    }
}

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one actual identity");
    assert!(values.next().is_none(), "identity is unique");
    value
}
