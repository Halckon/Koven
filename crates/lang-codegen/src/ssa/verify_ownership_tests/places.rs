//! Direct-root place transfer and heap/inline field mutation/loan contracts.

use super::*;

#[test]
fn root_place_take_requires_one_unborrowed_move_only_direct_root() {
    let mut valid = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = valid.parameter(0);
    let entry = valid.entry;
    let move_only = valid.move_only;
    let place = entity_place(
        valid.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    let loan = entity_loan(
        valid.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: move_only,
            }],
        )[0],
    );
    valid.append(entry, Operation::BorrowEnd { loan }, Vec::new());
    let rebound = value(
        valid.append(
            entry,
            Operation::RootPlaceTake { owner, place },
            vec![EntityType::Value(move_only)],
        )[0],
    );
    valid.append(entry, Operation::Drop { owner: rebound }, Vec::new());
    valid.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert_eq!(verify_program(&valid.program), Ok(()));

    let mut borrowed = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = borrowed.parameter(0);
    let entry = borrowed.entry;
    let move_only = borrowed.move_only;
    let place = entity_place(
        borrowed.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    let loan = entity_loan(
        borrowed.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: move_only,
            }],
        )[0],
    );
    let rebound = value(
        borrowed.append(
            entry,
            Operation::RootPlaceTake { owner, place },
            vec![EntityType::Value(move_only)],
        )[0],
    );
    borrowed.append(entry, Operation::BorrowEnd { loan }, Vec::new());
    borrowed.append(entry, Operation::Drop { owner: rebound }, Vec::new());
    borrowed.append(entry, Operation::Drop { owner }, Vec::new());
    borrowed.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&borrowed.program), |kind| matches!(
        kind,
        VerifyErrorKind::OwnerLoanConflict { value } if *value == owner
    )));

    let mut mismatched = Harness::new(&[TestType::MoveOnly, TestType::MoveOnly], &[]);
    let owner = mismatched.parameter(0);
    let other = mismatched.parameter(1);
    let entry = mismatched.entry;
    let move_only = mismatched.move_only;
    let other_place = entity_place(
        mismatched.append(
            entry,
            Operation::RootPlace { owner: other },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    mismatched.append(
        entry,
        Operation::RootPlaceTake {
            owner,
            place: other_place,
        },
        vec![EntityType::Value(move_only)],
    );
    mismatched.append(entry, Operation::Drop { owner: other }, Vec::new());
    mismatched.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&mismatched.program), |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let mut copyable = Harness::new(&[TestType::Integer], &[TestType::Integer]);
    let owner = copyable.parameter(0);
    let entry = copyable.entry;
    let integer = copyable.integer;
    let place = entity_place(
        copyable.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(integer)],
        )[0],
    );
    let result = value(
        copyable.append(
            entry,
            Operation::RootPlaceTake { owner, place },
            vec![EntityType::Value(integer)],
        )[0],
    );
    copyable.terminate(
        entry,
        TerminatorKind::Return {
            values: vec![result],
        },
    );
    assert!(has_kind(&errors(&copyable.program), |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let mut wrong_result = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = wrong_result.parameter(0);
    let entry = wrong_result.entry;
    let move_only = wrong_result.move_only;
    let integer = wrong_result.integer;
    let place = entity_place(
        wrong_result.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    wrong_result.append(
        entry,
        Operation::RootPlaceTake { owner, place },
        vec![EntityType::Value(integer)],
    );
    wrong_result.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&wrong_result.program), |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let mut repeated = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = repeated.parameter(0);
    let entry = repeated.entry;
    let move_only = repeated.move_only;
    let place = entity_place(
        repeated.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    let first = value(
        repeated.append(
            entry,
            Operation::RootPlaceTake { owner, place },
            vec![EntityType::Value(move_only)],
        )[0],
    );
    let second = value(
        repeated.append(
            entry,
            Operation::RootPlaceTake { owner, place },
            vec![EntityType::Value(move_only)],
        )[0],
    );
    repeated.append(entry, Operation::Drop { owner: first }, Vec::new());
    repeated.append(entry, Operation::Drop { owner: second }, Vec::new());
    repeated.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    let failures = errors(&repeated.program);
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::ValueUnavailable { value } if *value == owner
    )));
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::PlaceUnavailable { place: actual } if *actual == place
    )));
}

#[test]
fn consuming_an_owner_invalidates_its_untransferred_places() {
    let mut harness = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = harness.parameter(0);
    let entry = harness.entry;
    let move_only = harness.move_only;
    let place = entity_place(
        harness.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    harness.append(entry, Operation::Drop { owner }, Vec::new());
    let loan = entity_loan(
        harness.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: move_only,
            }],
        )[0],
    );
    harness.append(entry, Operation::BorrowEnd { loan }, Vec::new());
    harness.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&harness.program), |kind| matches!(
        kind,
        VerifyErrorKind::PlaceUnavailable { .. }
    )));
}

#[test]
fn heap_field_replace_requires_an_active_unshadowed_exclusive_receiver() {
    let exclusive = heap_field_program(
        LoanKind::Exclusive,
        false,
        false,
        0,
        TestType::Integer,
        HeapFieldAction::Replace,
    );
    assert_eq!(verify_program(&exclusive), Ok(()));

    let shared = errors(&heap_field_program(
        LoanKind::Shared,
        false,
        false,
        0,
        TestType::Integer,
        HeapFieldAction::Replace,
    ));
    assert!(has_kind(&shared, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let inactive = errors(&heap_field_program(
        LoanKind::Exclusive,
        true,
        false,
        0,
        TestType::Integer,
        HeapFieldAction::Replace,
    ));
    assert!(has_kind(&inactive, |kind| matches!(
        kind,
        VerifyErrorKind::LoanInactive { .. }
    )));

    let dependent = errors(&heap_field_program(
        LoanKind::Exclusive,
        false,
        true,
        0,
        TestType::Integer,
        HeapFieldAction::Replace,
    ));
    assert!(has_kind(&dependent, |kind| matches!(
        kind,
        VerifyErrorKind::LoanDependencyActive { .. }
    )));

    let dependent_read = errors(&heap_field_program(
        LoanKind::Exclusive,
        false,
        true,
        0,
        TestType::Integer,
        HeapFieldAction::Read,
    ));
    assert!(has_kind(&dependent_read, |kind| matches!(
        kind,
        VerifyErrorKind::LoanDependencyActive { .. }
    )));

    let out_of_bounds = errors(&heap_field_program(
        LoanKind::Exclusive,
        false,
        false,
        1,
        TestType::Integer,
        HeapFieldAction::Replace,
    ));
    assert!(has_kind(&out_of_bounds, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let move_only = heap_field_program(
        LoanKind::Exclusive,
        false,
        false,
        0,
        TestType::MoveOnly,
        HeapFieldAction::Replace,
    );
    assert_eq!(verify_program(&move_only), Ok(()));

    let move_only_read = errors(&heap_field_program(
        LoanKind::Shared,
        false,
        false,
        0,
        TestType::MoveOnly,
        HeapFieldAction::Read,
    ));
    assert!(has_kind(&move_only_read, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));
}

#[test]
fn inline_field_replace_requires_an_owned_field_and_an_unshadowed_exclusive_receiver() {
    let valid =
        inline_field_replace_program(LoanKind::Exclusive, false, false, 0, false, false, false);
    assert_eq!(verify_program(&valid), Ok(()));

    let move_only_owner =
        inline_field_replace_program(LoanKind::Exclusive, false, false, 0, true, false, false);
    assert_eq!(verify_program(&move_only_owner), Ok(()));

    let move_only_field =
        inline_field_replace_program(LoanKind::Exclusive, false, false, 0, false, true, false);
    assert_eq!(verify_program(&move_only_field), Ok(()));

    let shared = errors(&inline_field_replace_program(
        LoanKind::Shared,
        false,
        false,
        0,
        false,
        false,
        false,
    ));
    assert!(has_kind(&shared, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let inactive = errors(&inline_field_replace_program(
        LoanKind::Exclusive,
        true,
        false,
        0,
        false,
        false,
        false,
    ));
    assert!(has_kind(&inactive, |kind| matches!(
        kind,
        VerifyErrorKind::LoanInactive { .. }
    )));

    let dependent = errors(&inline_field_replace_program(
        LoanKind::Exclusive,
        false,
        true,
        0,
        false,
        false,
        false,
    ));
    assert!(has_kind(&dependent, |kind| matches!(
        kind,
        VerifyErrorKind::LoanDependencyActive { .. }
    )));

    for invalid in [
        inline_field_replace_program(LoanKind::Exclusive, false, false, 1, false, false, false),
        inline_field_replace_program(LoanKind::Exclusive, false, false, 0, false, false, true),
    ] {
        assert!(has_kind(&errors(&invalid), |kind| matches!(
            kind,
            VerifyErrorKind::OperationContract { .. }
        )));
    }
}

#[allow(clippy::too_many_arguments)]
fn inline_field_replace_program(
    receiver_kind: LoanKind,
    end_receiver: bool,
    shared_child: bool,
    field: usize,
    move_only_receiver: bool,
    move_only_field: bool,
    wrong_value_type: bool,
) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("inline-field-replace");
    let module = program.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let field_type = if move_only_field { resource } else { integer };
    let mut fields = vec![field_type];
    if move_only_receiver && !move_only_field {
        fields.push(resource);
    }
    let receiver_type = module
        .add_aggregate_type("Inline", fields)
        .expect("inline aggregate");
    let value_type = if wrong_value_type {
        boolean
    } else {
        field_type
    };
    let receiver_entity_type = EntityType::Loan {
        kind: receiver_kind,
        target: receiver_type,
    };
    let function_id = module
        .add_instance_function("replace", receiver_entity_type, Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(
            vec![receiver_entity_type, EntityType::Value(value_type)],
            origin.clone(),
        )
        .expect("entry");
    let parameters = function.block(entry).expect("entry").parameters.clone();
    let [EntityId::Loan(receiver), EntityId::Value(value)] = parameters.as_slice() else {
        panic!("receiver and replacement parameters");
    };
    if end_receiver {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: *receiver },
                Vec::new(),
                origin.clone(),
            )
            .expect("end receiver");
    }
    let child = if shared_child {
        let (_, results) = function
            .append_instruction(
                entry,
                Operation::SharedReborrow { source: *receiver },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: receiver_type,
                }],
                origin.clone(),
            )
            .expect("shared child");
        let EntityId::Loan(child) = results[0] else {
            panic!("shared child loan");
        };
        Some(child)
    } else {
        None
    };
    function
        .append_instruction(
            entry,
            Operation::InlineFieldReplace {
                receiver: *receiver,
                field,
                value: *value,
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("inline field replace");
    if let Some(child) = child {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: child },
                Vec::new(),
                origin.clone(),
            )
            .expect("end child");
    }
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");
    program
}

#[test]
fn shared_heap_field_loan_tracks_the_receiver_dependency() {
    for exclusive in [false, true] {
        let valid = shared_heap_field_loan_program(false, exclusive, 0);
        assert_eq!(verify_program(&valid), Ok(()));

        let parent_ended_first = errors(&shared_heap_field_loan_program(true, exclusive, 0));
        assert!(has_kind(&parent_ended_first, |kind| matches!(
            kind,
            VerifyErrorKind::LoanDependencyActive { .. }
        )));

        let out_of_bounds = errors(&shared_heap_field_loan_program(false, exclusive, 1));
        assert!(has_kind(&out_of_bounds, |kind| matches!(
            kind,
            VerifyErrorKind::OperationContract { .. }
        )));
    }
}

#[test]
fn shared_inline_field_loan_blocks_ending_its_parent_reborrow() {
    let valid = shared_inline_field_loan_program(false);
    assert_eq!(verify_program(&valid), Ok(()));

    let parent_ended_first = errors(&shared_inline_field_loan_program(true));
    assert!(has_kind(&parent_ended_first, |kind| matches!(
        kind,
        VerifyErrorKind::LoanDependencyActive { .. }
    )));
}

fn shared_inline_field_loan_program(end_parent_first: bool) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("inline-field-loan");
    let module = program.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let receiver_type = module
        .add_aggregate_type("Counter", vec![integer])
        .expect("inline aggregate");
    let function_id = module
        .add_instance_function(
            "read",
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: receiver_type,
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: receiver_type,
            }],
            origin.clone(),
        )
        .expect("entry");
    let EntityId::Loan(receiver) = function.block(entry).expect("entry").parameters[0] else {
        panic!("receiver loan");
    };
    let (_, reborrow_results) = function
        .append_instruction(
            entry,
            Operation::SharedReborrow { source: receiver },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: receiver_type,
            }],
            origin.clone(),
        )
        .expect("shared reborrow");
    let EntityId::Loan(reborrow) = reborrow_results[0] else {
        panic!("shared reborrow result");
    };
    let (_, field_results) = function
        .append_instruction(
            entry,
            Operation::SharedFieldLoan {
                base: reborrow,
                field: 0,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("field loan");
    let EntityId::Loan(field_loan) = field_results[0] else {
        panic!("field loan result");
    };
    if end_parent_first {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: reborrow },
                Vec::new(),
                origin.clone(),
            )
            .expect("end parent reborrow");
    }
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: field_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("end field loan");
    if !end_parent_first {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: reborrow },
                Vec::new(),
                origin.clone(),
            )
            .expect("end parent reborrow");
    }
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");
    program
}

fn shared_heap_field_loan_program(
    end_parent_first: bool,
    exclusive_receiver: bool,
    field: usize,
) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("heap-field-loan");
    let module = program.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let payload = module
        .add_aggregate_type("Host.payload", vec![integer])
        .expect("payload");
    let owner = module.declare_heap_owner("Host").expect("owner");
    module
        .define_heap_owner(owner, payload)
        .expect("owner payload");
    let receiver_kind = if exclusive_receiver {
        LoanKind::Exclusive
    } else {
        LoanKind::Shared
    };
    let function_id = module
        .add_instance_function(
            "project",
            EntityType::Loan {
                kind: receiver_kind,
                target: owner,
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(
            vec![EntityType::Loan {
                kind: receiver_kind,
                target: owner,
            }],
            origin.clone(),
        )
        .expect("entry");
    let EntityId::Loan(receiver) = function.block(entry).expect("entry").parameters[0] else {
        panic!("receiver loan");
    };
    let (_, results) = function
        .append_instruction(
            entry,
            Operation::SharedHeapFieldLoan {
                base: receiver,
                field,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("field loan");
    let EntityId::Loan(field_loan) = results[0] else {
        panic!("field loan result");
    };
    if end_parent_first {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: receiver },
                Vec::new(),
                origin.clone(),
            )
            .expect("end receiver");
    }
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: field_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("end field loan");
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");
    program
}

#[derive(Clone, Copy)]
enum HeapFieldAction {
    Read,
    Replace,
}

fn heap_field_program(
    receiver_kind: LoanKind,
    end_receiver: bool,
    shared_child: bool,
    field: usize,
    field_type: TestType,
    action: HeapFieldAction,
) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("heap-field");
    let module = program.module_mut(module_id).expect("module");
    let field_type = match field_type {
        TestType::Boolean => module.intern_type(SsaTypeKind::Boolean),
        TestType::Integer => module.intern_type(SsaTypeKind::Integer {
            bits: 64,
            signed: true,
        }),
        TestType::MoveOnly => module.intern_type(SsaTypeKind::Opaque {
            name: "Payload".to_owned(),
            ownership: Ownership::MoveOnly,
        }),
    };
    let payload = module
        .add_aggregate_type("Cell.payload", vec![field_type])
        .expect("payload");
    let owner = module.declare_heap_owner("Cell").expect("owner");
    module
        .define_heap_owner(owner, payload)
        .expect("owner payload");
    let receiver_type = EntityType::Loan {
        kind: receiver_kind,
        target: owner,
    };
    let function_id = module
        .add_instance_function("replace", receiver_type, Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let mut parameters = vec![receiver_type];
    if matches!(action, HeapFieldAction::Replace) {
        parameters.push(EntityType::Value(field_type));
    }
    let entry = function
        .add_block(parameters, origin.clone())
        .expect("entry");
    let EntityId::Loan(receiver) = function.block(entry).expect("entry").parameters[0] else {
        panic!("receiver loan");
    };
    if end_receiver {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: receiver },
                Vec::new(),
                origin.clone(),
            )
            .expect("end receiver");
    }
    let child = if shared_child {
        let (_, results) = function
            .append_instruction(
                entry,
                Operation::SharedReborrow { source: receiver },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: owner,
                }],
                origin.clone(),
            )
            .expect("shared child");
        let EntityId::Loan(child) = results[0] else {
            panic!("shared child loan");
        };
        Some(child)
    } else {
        None
    };
    match action {
        HeapFieldAction::Read => {
            function
                .append_instruction(
                    entry,
                    Operation::HeapFieldRead { receiver, field },
                    vec![EntityType::Value(field_type)],
                    origin.clone(),
                )
                .expect("read");
        }
        HeapFieldAction::Replace => {
            let EntityId::Value(value) = function.block(entry).expect("entry").parameters[1] else {
                panic!("replacement value");
            };
            function
                .append_instruction(
                    entry,
                    Operation::HeapFieldReplace {
                        receiver,
                        field,
                        value,
                    },
                    Vec::new(),
                    origin.clone(),
                )
                .expect("replace");
        }
    }
    if let Some(child) = child {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: child },
                Vec::new(),
                origin.clone(),
            )
            .expect("end child");
    }
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");
    program
}
