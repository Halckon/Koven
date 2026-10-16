//! Read-only identity assertions for five direct ordinary nullable while sources.
use crate::ssa::model::{
    Definition, EntityId, EntityType, LoanKind, Operation, Program, SsaTypeKind, TerminatorKind,
};

pub(super) fn assert_direct_proof(program: &Program, case: usize) {
    assert_proof(program, case, false);
}

pub(super) fn assert_unit_direct_proof(program: &Program, case: usize) {
    assert_proof(program, case, true);
}

fn assert_proof(program: &Program, case: usize, unit: bool) {
    let module = only(program.modules.iter());
    let function = only(module.functions.iter().filter(|function| {
        if unit {
            // Unit names include source-qualified declaration identities.
            // This fixture has exactly one function with a nullable guard.
            function.blocks.iter().any(|block| {
                matches!(
                    block.terminator.as_ref().map(|term| &term.kind),
                    Some(TerminatorKind::NullableBranch { .. })
                )
            })
        } else {
            function.name == "entry"
        }
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
    assert_eq!(body.parameters.len(), when_non_null.arguments.len() + 1);
    assert_eq!(body.parameters.last(), Some(&EntityId::Loan(*view)));
    assert_eq!(absent.parameters.len(), when_null.arguments.len());
    assert!(!when_null.arguments.contains(&EntityId::Loan(*view)));
    let flag = if unit {
        let boolean_loan = |entities: &[EntityId]| {
            only(entities.iter().copied().filter(|entity| {
                matches!(function.entity(*entity).unwrap().ty,
                    EntityType::Loan { kind: LoanKind::Shared, target }
                    if module.type_kind(target) == Some(&SsaTypeKind::Boolean))
            }))
        };
        let entry = function.block(function.entry_block().unwrap()).unwrap();
        let entry_flag = boolean_loan(&entry.parameters);
        let header_flag = boolean_loan(&header.parameters);
        let body_flag = boolean_loan(&body.parameters);
        let absent_flag = boolean_loan(&absent.parameters);
        for block in [header, absent] {
            assert_eq!(
                block
                    .parameters
                    .iter()
                    .filter(|entity| matches!(entity, EntityId::Loan(_)))
                    .count(),
                1
            );
        }
        assert_eq!(
            body.parameters
                .iter()
                .filter(|entity| matches!(entity, EntityId::Loan(_)))
                .count(),
            2
        );
        let header_column = header
            .parameters
            .iter()
            .position(|entity| *entity == header_flag)
            .unwrap();
        let body_column = body
            .parameters
            .iter()
            .position(|entity| *entity == body_flag)
            .unwrap();
        let absent_column = absent
            .parameters
            .iter()
            .position(|entity| *entity == absent_flag)
            .unwrap();
        let TerminatorKind::Branch(initial) = &entry.terminator.as_ref().unwrap().kind else {
            panic!("real initial header edge")
        };
        assert_eq!(initial.target, header.id);
        assert_eq!(initial.arguments[header_column], entry_flag);
        assert_eq!(when_non_null.arguments[body_column], header_flag);
        assert_eq!(when_null.arguments[absent_column], header_flag);
        for instruction in &function.instructions {
            if let Operation::BorrowEnd { loan } = instruction.operation {
                assert!(
                    ![entry_flag, header_flag, body_flag, absent_flag]
                        .contains(&EntityId::Loan(loan))
                );
            }
        }
        Some((header_column, body_flag))
    } else {
        for block in [header, absent] {
            assert!(
                !block
                    .parameters
                    .iter()
                    .any(|entity| matches!(entity, EntityId::Loan(_)))
            );
        }
        None
    };
    let owner_column = only(
        when_non_null
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, entity)| (*entity == EntityId::Value(*owner)).then_some(index)),
    );
    let EntityId::Value(body_owner) = body.parameters[owner_column] else {
        panic!("paired owner")
    };
    let EntityType::Value(nullable) = function.entity(EntityId::Value(*owner)).unwrap().ty else {
        panic!("owned nullable")
    };
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
        panic!("nullable identity")
    };
    assert!(matches!(
        module.type_kind(*inner),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    assert_eq!(
        function.entity(EntityId::Value(body_owner)).unwrap().ty,
        EntityType::Value(nullable)
    );
    assert_eq!(
        function.entity(EntityId::Loan(*view)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: *inner
        }
    );

    let read = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::HeapFieldRead { receiver, field: 0 } if receiver == *view)
    }));
    let end = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == *view)
    }));
    let drop = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::Drop { owner } if owner == body_owner)
    }));
    assert_eq!(read.block, body.id);
    assert_eq!(end.block, body.id);
    assert_eq!(drop.block, body.id);
    let position = |id| {
        body.instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    assert!(position(read.id) < position(end.id));
    assert!(position(end.id) < position(drop.id));
    let result = only(read.results.iter().copied());
    let EntityType::Value(result_type) = function.entity(result).unwrap().ty else {
        panic!("field value")
    };
    assert_eq!(
        module.type_kind(result_type),
        Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true
        })
    );
    let terminator = &body.terminator.as_ref().unwrap().kind;
    assert!(!terminator.entities().contains(&EntityId::Loan(*view)));
    if case < 3 {
        let TerminatorKind::Branch(edge) = terminator else {
            panic!("normal/continue backedge")
        };
        assert_eq!(edge.target, header.id);
        if let Some((column, body_flag)) = flag {
            assert_eq!(edge.arguments[column], body_flag);
            assert_eq!(
                edge.arguments
                    .iter()
                    .filter(|entity| matches!(entity, EntityId::Loan(_)))
                    .count(),
                1
            );
        } else {
            assert!(
                !edge
                    .arguments
                    .iter()
                    .any(|entity| matches!(entity, EntityId::Loan(_)))
            );
        }
        let Definition::BlockParameter { block, index } =
            function.entity(EntityId::Value(*owner)).unwrap().definition
        else {
            panic!("the condition reads the loop header owner parameter")
        };
        assert_eq!(block, header.id);
        let replacement = edge.arguments[index];
        let Definition::InstructionResult {
            instruction,
            index: 0,
        } = function.entity(replacement).unwrap().definition
        else {
            panic!("replacement is the actual nullable-null result")
        };
        assert!(
            matches!(function.instruction(instruction).unwrap().operation,
            Operation::NullableNull { nullable: actual } if actual == nullable)
        );
        assert_ne!(
            replacement,
            EntityId::Value(body_owner),
            "the consumed owner is replaced before recurrence"
        );
    } else {
        assert!(!terminator.targets().contains(&header.id));
        if case == 4 {
            assert!(matches!(terminator, TerminatorKind::Return { .. }));
        }
    }
}

fn only<T>(mut iterator: impl Iterator<Item = T>) -> T {
    let value = iterator.next().expect("exactly one real identity");
    assert!(iterator.next().is_none(), "identity must be unique");
    value
}
