//! Query-only contract for the original empty/populated nullable-field call frames.
use std::collections::BTreeSet;

use lang_frontend::source::SourceId;

use crate::ssa::model::{
    Definition, EntityId, EntityType, LoanKind, Operation, Origin, Program, ScalarConstant,
    SsaTypeKind, TerminatorKind,
};

pub(super) fn assert_value_argument(program: &Program, source: &str, unit: bool, prefix: bool) {
    let module = only(program.modules.iter());
    let function = only(module.functions.iter().filter(|function| {
        function.blocks.iter().any(|block| {
            matches!(
                block.terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::NullableBranch { .. })
            )
        })
    }));
    let Origin::Source(function_span) = &function.origin else {
        panic!("the source entry has an original declaration span")
    };
    let source_id = function_span.source_id();
    assert_eq!(format!("{source_id:?}"), "SourceId(0)");
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
    // SPEC-0350: in both layouts `entry` lies in the refinement subset, which attributes
    // the proof branch to the null comparison rather than the whole `while`.
    assert_source(
        &header.terminator.as_ref().unwrap().origin,
        source_id,
        source.find("current != null").unwrap(),
        "current != null".len(),
        unit,
    );
    let body = function.block(when_non_null.target).unwrap();
    let absent = function.block(when_null.target).unwrap();
    let column = only(
        when_non_null
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, entity)| (*entity == EntityId::Value(*owner)).then_some(index)),
    );
    assert_eq!(when_null.arguments[column], EntityId::Value(*owner));
    let EntityId::Value(body_owner) = body.parameters[column] else {
        panic!("the non-null edge carries the original nullable owner")
    };
    assert_eq!(body.parameters.len(), when_non_null.arguments.len() + 1);
    assert_eq!(body.parameters.last(), Some(&EntityId::Loan(*view)));
    assert_eq!(absent.parameters.len(), when_null.arguments.len());
    assert_eq!(
        function.entity(EntityId::Loan(*view)).unwrap().definition,
        Definition::BlockParameter {
            block: body.id,
            index: body.parameters.len() - 1
        }
    );
    let Definition::BlockParameter { block, index } =
        function.entity(EntityId::Value(*owner)).unwrap().definition
    else {
        panic!("the condition reads its actual loop-header owner")
    };
    assert_eq!(block, header.id);
    assert_eq!(header.parameters[index], EntityId::Value(*owner));
    let EntityType::Value(nullable) = function.entity(EntityId::Value(*owner)).unwrap().ty else {
        panic!("nullable owned storage")
    };
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
        panic!("registered nullable handle")
    };
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(*inner)
    else {
        panic!("ordinary class payload")
    };
    let Some(SsaTypeKind::Aggregate { fields, .. }) = module.type_kind(*payload) else {
        panic!("registered payload layout")
    };
    let integer = *only(fields.iter());
    assert_eq!(
        module.type_kind(integer),
        Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true
        })
    );
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

    let read =
        only(function.instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::HeapFieldRead { .. })
        }));
    assert!(
        matches!(read.operation, Operation::HeapFieldRead { receiver, field: 0 }
        if receiver == *view)
    );
    assert_source(
        &read.origin,
        source_id,
        source.find("current.value").unwrap(),
        "current.value".len(),
        unit,
    );
    let read_value = only(read.results.iter().copied());
    assert!(matches!(read_value, EntityId::Value(_)));
    assert_eq!(
        function.entity(read_value).unwrap().ty,
        EntityType::Value(integer)
    );
    let call = only(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. })),
    );
    let Operation::DirectCall {
        callee,
        receiver,
        arguments,
    } = &call.operation
    else {
        unreachable!()
    };
    assert_eq!(*receiver, None);
    let call_start = source.rfind("pass(").unwrap();
    let call_length = source[call_start..].find(')').unwrap() + 1;
    assert_source(&call.origin, source_id, call_start, call_length, unit);
    let callee = module.function(*callee).unwrap();
    let callee_start = source.find("fun pass").unwrap();
    let callee_length = source[callee_start..].find('}').unwrap() + 1;
    assert_source(&callee.origin, source_id, callee_start, callee_length, unit);
    assert!(
        callee.receiver.is_none()
            && callee.borrow_return.is_none()
            && callee.carrier_return.is_none()
    );
    assert_eq!(callee.return_types, [integer]);
    let parameters = &callee
        .block(callee.entry_block().unwrap())
        .unwrap()
        .parameters;
    assert_eq!(parameters.len(), if prefix { 2 } else { 1 });
    for parameter in parameters {
        assert_eq!(
            callee.entity(*parameter).unwrap().ty,
            EntityType::Value(integer)
        );
    }
    let mut order = Vec::new();
    if prefix {
        let marker = only(function.instructions.iter().filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::Constant(ScalarConstant::Integer(1))
            )
        }));
        assert_source(
            &marker.origin,
            source_id,
            call_start + "pass(".len(),
            1,
            unit,
        );
        let marker_value = only(marker.results.iter().copied());
        assert_ne!(marker_value, read_value);
        assert_eq!(
            function.entity(marker_value).unwrap().ty,
            EntityType::Value(integer)
        );
        assert_eq!(arguments.as_slice(), [marker_value, read_value]);
        order.push(marker);
    } else {
        assert_eq!(arguments.as_slice(), [read_value]);
    }
    let ended = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == *view)
    }));
    let dropped = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::Drop { owner } if owner == body_owner)
    }));
    order.extend([read, ended, dropped, call]);
    let mut previous = None;
    for instruction in order {
        assert_eq!(instruction.block, body.id);
        let position = body
            .instructions
            .iter()
            .position(|id| *id == instruction.id)
            .unwrap();
        if let Some(previous) = previous {
            assert!(
                previous < position,
                "prefix, read, proof end, owner drop, direct call"
            );
        }
        previous = Some(position);
    }
    let view_users = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction
                .operation
                .entities()
                .contains(&EntityId::Loan(*view))
        })
        .map(|instruction| instruction.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(view_users, BTreeSet::from([read.id, ended.id]));
    let EntityId::Value(result) = only(call.results.iter().copied()) else {
        panic!("the call returns an ordinary Int")
    };
    assert!(matches!(&body.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.as_slice() == [result]));
    assert!(function.instructions.iter().all(|instruction| !matches!(
        instruction.operation,
        Operation::NullableTake { .. } | Operation::NullableWrap { .. }
    )));

    // Inspect all reachable null-side blocks, not just the direct false successor.
    let mut pending = vec![absent.id];
    let mut visited = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        assert_ne!(id, body.id, "the null path cannot reach the field or call");
        let block = function.block(id).unwrap();
        assert!(!block.parameters.contains(&EntityId::Loan(*view)));
        for id in &block.instructions {
            let instruction = function.instruction(*id).unwrap();
            assert!(!matches!(
                instruction.operation,
                Operation::HeapFieldRead { .. }
            ));
            assert_ne!(instruction.id, call.id);
            assert!(
                !instruction
                    .operation
                    .entities()
                    .contains(&EntityId::Loan(*view))
            );
        }
        let terminator = &block.terminator.as_ref().unwrap().kind;
        assert!(!terminator.entities().contains(&EntityId::Loan(*view)));
        pending.extend(terminator.targets());
    }
}

fn assert_source(origin: &Origin, source: SourceId, start: usize, length: usize, unit: bool) {
    let Origin::Source(span) = origin else {
        panic!("real source origin")
    };
    assert_eq!(span.source_id(), source);
    let offset = if unit { "package p\n".len() } else { 0 };
    assert_eq!(span.start(), start + offset);
    assert_eq!(span.end(), start + offset + length);
}

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one actual identity");
    assert!(values.next().is_none(), "identity is unique");
    value
}
