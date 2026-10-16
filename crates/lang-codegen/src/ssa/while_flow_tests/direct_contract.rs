//! Read-only identity assertions for five direct ordinary nullable while sources.
use crate::ssa::model::{
    EntityId, EntityType, LoanKind, Operation, Program, SsaTypeKind, TerminatorKind,
};

pub(super) fn assert_direct_proof(program: &Program, case: usize) {
    let module = only(program.modules.iter());
    let function = only(
        module
            .functions
            .iter()
            .filter(|function| function.name == "entry"),
    );
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
    assert!(
        !absent
            .parameters
            .iter()
            .any(|entity| matches!(entity, EntityId::Loan(_)))
    );
    assert!(
        !header
            .parameters
            .iter()
            .any(|entity| matches!(entity, EntityId::Loan(_)))
    );
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
        assert!(
            !edge
                .arguments
                .iter()
                .any(|entity| matches!(entity, EntityId::Loan(_)))
        );
        let replacement = edge.arguments[owner_column];
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
