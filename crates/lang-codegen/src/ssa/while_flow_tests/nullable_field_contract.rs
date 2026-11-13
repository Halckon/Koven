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
    assert_eq!(
        function.entity(EntityId::Loan(child)).unwrap().ty,
        child_type
    );
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
            assert!(
                !returned,
                "the direct and Group fixtures print the same child"
            );
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

pub(in crate::ssa) mod value_argument_contract {
    //! Query-only contract for the original empty/populated nullable-field call frames.
    use std::collections::BTreeSet;

    use lang_frontend::source::SourceId;

    use crate::ssa::model::{
        Definition, EntityId, EntityType, LoanKind, Operation, Origin, Program, ScalarConstant,
        SsaTypeKind, TerminatorKind,
    };

    pub(in crate::ssa) fn assert_value_argument(
        program: &Program,
        source: &str,
        unit: bool,
        prefix: bool,
    ) {
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
                .filter_map(|(index, entity)| {
                    (*entity == EntityId::Value(*owner)).then_some(index)
                }),
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
        let EntityType::Value(nullable) = function.entity(EntityId::Value(*owner)).unwrap().ty
        else {
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

        let read = only(function.instructions.iter().filter(|instruction| {
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
        let call =
            only(function.instructions.iter().filter(|instruction| {
                matches!(instruction.operation, Operation::DirectCall { .. })
            }));
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
}

pub(in crate::ssa) mod direct_contract {
    //! Read-only identity assertions for five direct ordinary nullable while sources.
    use crate::ssa::model::{
        Definition, EntityId, EntityType, Function, FunctionId, LoanKind, Module, Operation,
        Program, SsaTypeKind, TerminatorKind,
    };

    pub(in crate::ssa) fn assert_direct_proof(program: &Program, case: usize) {
        assert_proof(program, case, false);
    }

    pub(in crate::ssa) fn assert_unit_direct_proof(program: &Program, case: usize) {
        assert_proof(program, case, true);
    }

    pub(in crate::ssa) fn assert_memory_unit_direct_proof(
        program: &Program,
        entry: FunctionId,
        case: usize,
    ) {
        let module = only(program.modules.iter());
        let function = module
            .function(entry)
            .expect("same-analysis returned entry");
        assert_function_proof(module, function, case, true);
    }

    fn assert_proof(program: &Program, case: usize, unit: bool) {
        let module = only(program.modules.iter());
        let function = only(module.functions.iter().filter(|function| {
            if unit {
                // Unit names include source-qualified declaration identities.
                // This fixture has exactly one function with a nullable guard.
                function
                    .name
                    .strip_prefix("koven.p.entry.d")
                    .is_some_and(|index| {
                        !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())
                    })
                    && function.blocks.iter().any(|block| {
                        matches!(
                            block.terminator.as_ref().map(|term| &term.kind),
                            Some(TerminatorKind::NullableBranch { .. })
                        )
                    })
            } else {
                function.name == "entry"
            }
        }));
        assert_function_proof(module, function, case, unit);
    }

    fn assert_function_proof(module: &Module, function: &Function, case: usize, unit: bool) {
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
        if unit {
            // All five unit cases now lower through `ssa::refine`: the unused `flag` loan stays
            // an entry parameter, and loop headers without a backedge fold into `entry`.
            let entry = function.block(function.entry_block().unwrap()).unwrap();
            if case >= 3 {
                assert_eq!(header.id, entry.id);
            } else {
                let TerminatorKind::Branch(initial) = &entry.terminator.as_ref().unwrap().kind
                else {
                    panic!("real initial header edge")
                };
                assert_eq!(initial.target, header.id);
                assert!(
                    !header
                        .parameters
                        .iter()
                        .any(|entity| matches!(entity, EntityId::Loan(_)))
                );
            }
            for block in [body, absent] {
                assert!(
                    block
                        .parameters
                        .iter()
                        .all(|entity| !matches!(entity, EntityId::Loan(_))
                            || *entity == EntityId::Loan(*view))
                );
            }
        } else {
            for block in [header, absent] {
                assert!(
                    !block
                        .parameters
                        .iter()
                        .any(|entity| matches!(entity, EntityId::Loan(_)))
                );
            }
        }
        let owner_column = only(
            when_non_null
                .arguments
                .iter()
                .enumerate()
                .filter_map(|(index, entity)| {
                    (*entity == EntityId::Value(*owner)).then_some(index)
                }),
        );
        let EntityId::Value(body_owner) = body.parameters[owner_column] else {
            panic!("paired owner")
        };
        let EntityType::Value(nullable) = function.entity(EntityId::Value(*owner)).unwrap().ty
        else {
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

    pub(in crate::ssa) fn assert_owner_return(program: &Program) {
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
        let column = only(
            when_non_null
                .arguments
                .iter()
                .enumerate()
                .filter_map(|(index, entity)| {
                    (*entity == EntityId::Value(*owner)).then_some(index)
                }),
        );
        let EntityId::Value(body_owner) = body.parameters[column] else {
            panic!("real true-edge owner")
        };
        assert_eq!(body.parameters.last(), Some(&EntityId::Loan(*view)));
        assert!(!when_null.arguments.contains(&EntityId::Loan(*view)));
        let EntityType::Value(nullable) = function.entity(EntityId::Value(*owner)).unwrap().ty
        else {
            panic!("nullable owner")
        };
        let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
            panic!("nullable storage")
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
        let end = only(function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == *view)));
        assert_eq!(end.block, body.id);
        assert!(body.instructions.contains(&end.id));
        assert!(
            matches!(&body.terminator.as_ref().unwrap().kind, TerminatorKind::Return { values } if values.as_slice() == [body_owner])
        );
        for instruction in &function.instructions {
            assert!(
                !matches!(instruction.operation, Operation::Drop { owner } if owner == body_owner)
            );
            assert!(!matches!(
                instruction.operation,
                Operation::NullableTake { .. } | Operation::NullableWrap { .. }
            ));
        }
    }
}
