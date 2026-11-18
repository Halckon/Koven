//! Lifetime contract of a String payload loan.
use std::collections::BTreeSet;

use super::*;

#[derive(Clone, Copy)]
enum NullableLayout {
    Niche,
    Tagged,
    Unregistered,
    TaggedBoolean,
    Rc,
}

#[test]
fn nullable_string_loan_branch_tracks_the_parent_across_a_rebound_child() {
    for layout in [NullableLayout::Niche, NullableLayout::Tagged] {
        let valid = nullable_loan_branch_fixture(layout, LoanKind::Shared, false, false, None);
        assert_eq!(verify_program(&valid), Ok(()));

        let early_end = errors(&nullable_loan_branch_fixture(
            layout,
            LoanKind::Shared,
            true,
            false,
            None,
        ));
        assert!(has_kind(&early_end, |kind| matches!(
            kind,
            VerifyErrorKind::LoanDependencyActive { .. }
        )));
    }
}

#[test]
fn nullable_string_loan_branch_requires_a_registered_tagged_wrapper() {
    for layout in [NullableLayout::Unregistered, NullableLayout::TaggedBoolean] {
        let invalid = nullable_loan_branch_fixture(layout, LoanKind::Shared, false, false, None);
        assert!(has_kind(&errors(&invalid), |kind| matches!(
            kind,
            VerifyErrorKind::OperationContract { .. }
        )));
    }
}

#[test]
fn nullable_string_and_rc_inout_require_retiring_the_proof_before_replace() {
    for layout in [
        NullableLayout::Niche,
        NullableLayout::Tagged,
        NullableLayout::Rc,
    ] {
        let merged =
            nullable_loan_branch_fixture(layout, LoanKind::Exclusive, false, false, Some(false));
        assert_eq!(verify_program(&merged), Ok(()));
        let early_replace =
            nullable_loan_branch_fixture(layout, LoanKind::Exclusive, false, false, Some(true));
        assert!(has_kind(&errors(&early_replace), |kind| matches!(
            kind,
            VerifyErrorKind::LoanDependencyActive { .. }
        )));
    }
}

#[test]
fn nullable_rc_inout_presence_rejects_a_parent_ended_across_blocks() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("nullable-rc-presence");
    let module = program.module_mut(module_id).unwrap();
    let payload = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let rc = module.declare_shared_owner("Rc").unwrap();
    module.define_shared_owner(rc, payload).unwrap();
    let nullable = module.add_nullable_handle_type(rc).unwrap();
    let id = module
        .add_function("probe", Vec::new(), origin.clone())
        .unwrap();
    let function = module.function_mut(id).unwrap();
    let entry = function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: nullable,
            }],
            origin.clone(),
        )
        .unwrap();
    let parent = entity_loan(function.block(entry).unwrap().parameters[0]);
    let check = function.add_block(Vec::new(), origin.clone()).unwrap();
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: parent },
            Vec::new(),
            origin.clone(),
        )
        .unwrap();
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: check,
                arguments: Vec::new(),
            }),
            origin.clone(),
        )
        .unwrap();
    function
        .append_instruction(
            check,
            Operation::NullableLoanIsNull { source: parent },
            vec![EntityType::Value(boolean)],
            origin.clone(),
        )
        .unwrap();
    function
        .set_terminator(check, TerminatorKind::Return { values: Vec::new() }, origin)
        .unwrap();
    assert!(has_kind(&errors(&program), |kind| matches!(
        kind,
        VerifyErrorKind::LoanInactive { .. }
    )));
}

fn nullable_loan_branch_fixture(
    layout: NullableLayout,
    kind: LoanKind,
    end_parent_first: bool,
    end_before_branch: bool,
    replace_before_child_end: Option<bool>,
) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("nullable-string-loan");
    let module = program.module_mut(module_id).unwrap();
    let (inner, nullable) = if matches!(layout, NullableLayout::Rc) {
        let payload = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        });
        let inner = module.declare_shared_owner("Rc").unwrap();
        module.define_shared_owner(inner, payload).unwrap();
        (inner, module.add_nullable_handle_type(inner).unwrap())
    } else {
        let inner = if matches!(layout, NullableLayout::TaggedBoolean) {
            module.intern_type(SsaTypeKind::Boolean)
        } else {
            module.add_string_owner_type()
        };
        let wrapper = match layout {
            NullableLayout::Niche => module.add_inline_nullable_type(inner),
            NullableLayout::Tagged | NullableLayout::TaggedBoolean => {
                module.add_map_result_type(inner)
            }
            NullableLayout::Unregistered => {
                let boolean = module.intern_type(SsaTypeKind::Boolean);
                module.add_aggregate_type("lookalike", vec![boolean, inner])
            }
            NullableLayout::Rc => unreachable!(),
        };
        (inner, wrapper.unwrap())
    };
    let id = module
        .add_function("inspect", Vec::new(), origin.clone())
        .unwrap();
    let function = module.function_mut(id).unwrap();
    let parent_ty = EntityType::Loan {
        kind,
        target: nullable,
    };
    let view_ty = EntityType::Loan {
        kind: LoanKind::Shared,
        target: inner,
    };
    let entry = function.add_block(vec![parent_ty], origin.clone()).unwrap();
    let parent = entity_loan(function.block(entry).unwrap().parameters[0]);
    let branch = if end_before_branch {
        let next = function.add_block(Vec::new(), origin.clone()).unwrap();
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: parent },
                Vec::new(),
                origin.clone(),
            )
            .unwrap();
        function
            .set_terminator(
                entry,
                TerminatorKind::Branch(Edge {
                    target: next,
                    arguments: Vec::new(),
                }),
                origin.clone(),
            )
            .unwrap();
        next
    } else {
        entry
    };
    let absent = function.add_block(Vec::new(), origin.clone()).unwrap();
    let present = function.add_block(vec![view_ty], origin.clone()).unwrap();
    let view = entity_loan(function.block(present).unwrap().parameters[0]);
    let joined = function.add_block(vec![view_ty], origin.clone()).unwrap();
    let rebound = entity_loan(function.block(joined).unwrap().parameters[0]);
    function
        .set_terminator(
            branch,
            TerminatorKind::NullableLoanBranch {
                source: parent,
                when_null: Edge {
                    target: absent,
                    arguments: Vec::new(),
                },
                when_non_null: Edge {
                    target: present,
                    arguments: Vec::new(),
                },
                view,
            },
            origin.clone(),
        )
        .unwrap();
    function
        .set_terminator(
            present,
            TerminatorKind::Branch(Edge {
                target: joined,
                arguments: vec![EntityId::Loan(view)],
            }),
            origin.clone(),
        )
        .unwrap();
    let order = if end_parent_first {
        [parent, rebound]
    } else {
        [rebound, parent]
    };
    for loan in order {
        if replace_before_child_end == Some(loan == rebound) {
            let (_, values) = function
                .append_instruction(
                    joined,
                    Operation::NullableNull { nullable },
                    vec![EntityType::Value(nullable)],
                    origin.clone(),
                )
                .unwrap();
            let EntityId::Value(value) = values[0] else {
                panic!("null value");
            };
            function
                .append_instruction(
                    joined,
                    Operation::LoanReplace {
                        loan: parent,
                        value,
                    },
                    Vec::new(),
                    origin.clone(),
                )
                .unwrap();
        }
        function
            .append_instruction(
                joined,
                Operation::BorrowEnd { loan },
                Vec::new(),
                origin.clone(),
            )
            .unwrap();
    }
    for block in [joined, absent] {
        function
            .set_terminator(
                block,
                TerminatorKind::Return { values: Vec::new() },
                origin.clone(),
            )
            .unwrap();
    }
    program
}

#[test]
fn nullable_string_branch_keeps_the_ancestor_of_a_rebound_field_loan_active() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("nullable-string-field");
    let module = program.module_mut(module_id).unwrap();
    let string = module.add_string_owner_type();
    let nullable = module.add_inline_nullable_type(string).unwrap();
    let aggregate = module.add_aggregate_type("Holder", vec![nullable]).unwrap();
    let id = module
        .add_function("inspect", Vec::new(), origin.clone())
        .unwrap();
    let function = module.function_mut(id).unwrap();
    let shared = |target| EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    };
    let entry = function
        .add_block(vec![shared(aggregate)], origin.clone())
        .unwrap();
    let parent = entity_loan(function.block(entry).unwrap().parameters[0]);
    let (_, field) = function
        .append_instruction(
            entry,
            Operation::SharedFieldLoan {
                base: parent,
                field: 0,
            },
            vec![shared(nullable)],
            origin.clone(),
        )
        .unwrap();
    let test = function
        .add_block(vec![shared(nullable)], origin.clone())
        .unwrap();
    let source = entity_loan(function.block(test).unwrap().parameters[0]);
    let present = function
        .add_block(vec![shared(string)], origin.clone())
        .unwrap();
    let absent = function.add_block(Vec::new(), origin.clone()).unwrap();
    let view = entity_loan(function.block(present).unwrap().parameters[0]);
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: test,
                arguments: field,
            }),
            origin.clone(),
        )
        .unwrap();
    function
        .set_terminator(
            test,
            TerminatorKind::NullableLoanBranch {
                source,
                view,
                when_null: Edge {
                    target: absent,
                    arguments: Vec::new(),
                },
                when_non_null: Edge {
                    target: present,
                    arguments: Vec::new(),
                },
            },
            origin.clone(),
        )
        .unwrap();
    for loan in [view, parent] {
        function
            .append_instruction(
                present,
                Operation::BorrowEnd { loan },
                Vec::new(),
                origin.clone(),
            )
            .unwrap();
    }
    for block in [present, absent] {
        function
            .set_terminator(
                block,
                TerminatorKind::Return { values: Vec::new() },
                origin.clone(),
            )
            .unwrap();
    }
    assert_eq!(verify_program(&program), Ok(()));
    let function = program
        .module_mut(module_id)
        .unwrap()
        .function_mut(id)
        .unwrap();
    function.blocks[present.index()].instructions.swap(0, 1);
    assert!(has_kind(&errors(&program), |kind| matches!(
        kind,
        VerifyErrorKind::LoanDependencyActive { .. }
    )));
}

#[test]
fn nullable_string_branch_checks_the_parent_of_a_borrowed_call_result() {
    assert_nullable_string_branch_checks_source(false);
}

#[test]
fn nullable_string_branch_checks_the_parent_of_a_required_map_value() {
    assert_nullable_string_branch_checks_source(true);
}

fn assert_nullable_string_branch_checks_source(map_value: bool) {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("nullable-string-borrow-call");
    let module = program.module_mut(module_id).unwrap();
    let string = module.add_string_owner_type();
    let nullable = module.add_inline_nullable_type(string).unwrap();
    let shared = |target| EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    };
    let callee = module
        .add_function("identity", vec![nullable], origin.clone())
        .unwrap();
    let identity = module.function_mut(callee).unwrap();
    identity.borrow_return = Some(0);
    let entry = identity
        .add_block(vec![shared(nullable)], origin.clone())
        .unwrap();
    let loan = entity_loan(identity.block(entry).unwrap().parameters[0]);
    identity
        .set_terminator(entry, TerminatorKind::BorrowReturn { loan }, origin.clone())
        .unwrap();

    let key_type = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let map = module
        .add_map_container_type(crate::ssa::model::MapContainerKind::Map, key_type, nullable)
        .unwrap();
    let parent_type = if map_value { map } else { nullable };
    let id = module
        .add_function("inspect", Vec::new(), origin.clone())
        .unwrap();
    let function = module.function_mut(id).unwrap();
    let entry = function
        .add_block(
            vec![shared(parent_type), EntityType::Value(key_type)],
            origin.clone(),
        )
        .unwrap();
    let entry_parameters = &function.block(entry).unwrap().parameters;
    let parent = entity_loan(entry_parameters[0]);
    let EntityId::Value(key) = entry_parameters[1] else {
        panic!("map lookup key")
    };
    let test = function.add_block(Vec::new(), origin.clone()).unwrap();
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: test,
                arguments: Vec::new(),
            }),
            origin.clone(),
        )
        .unwrap();
    let (call, result) = function
        .append_instruction(
            test,
            if map_value {
                Operation::MapRequireValue {
                    source: parent,
                    key: EntityId::Value(key),
                }
            } else {
                Operation::BorrowCall {
                    callee,
                    arguments: vec![EntityId::Loan(parent)],
                    source: parent,
                }
            },
            vec![shared(nullable)],
            origin.clone(),
        )
        .unwrap();
    let source = entity_loan(result[0]);
    let absent = function
        .add_block(vec![shared(nullable)], origin.clone())
        .unwrap();
    let present = function
        .add_block(vec![shared(nullable), shared(string)], origin.clone())
        .unwrap();
    let view = entity_loan(function.block(present).unwrap().parameters[1]);
    function
        .set_terminator(
            test,
            TerminatorKind::NullableLoanBranch {
                source,
                view,
                when_null: Edge {
                    target: absent,
                    arguments: result.clone(),
                },
                when_non_null: Edge {
                    target: present,
                    arguments: result,
                },
            },
            origin.clone(),
        )
        .unwrap();
    for block in [present, absent] {
        let loans = function.block(block).unwrap().parameters.clone();
        for loan in loans.into_iter().rev().map(entity_loan) {
            function
                .append_instruction(
                    block,
                    Operation::BorrowEnd { loan },
                    Vec::new(),
                    origin.clone(),
                )
                .unwrap();
        }
        function
            .set_terminator(
                block,
                TerminatorKind::Return { values: Vec::new() },
                origin.clone(),
            )
            .unwrap();
    }

    assert_eq!(verify_program(&program), Ok(()));
    let function = program
        .module_mut(module_id)
        .unwrap()
        .function_mut(id)
        .unwrap();
    let transfer = function.blocks[entry.index()].terminator.take().unwrap();
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: parent },
            Vec::new(),
            origin,
        )
        .unwrap();
    function.blocks[entry.index()].terminator = Some(transfer);
    assert!(errors(&program).iter().any(|error| error.location
        == crate::ssa::verify::VerifyLocation::Instruction(call)
        && matches!(error.kind, VerifyErrorKind::LoanInactive { loan } if loan == parent)));
}

#[test]
fn nullable_string_ending_a_swapped_parameter_uses_the_incoming_origin() {
    use crate::ssa::verify_ownership::aliases::provider_lifetime::{Reference, State};

    let mut program = Program::default();
    let module = program.add_module("swapped-origins");
    let function = program
        .module_mut(module)
        .unwrap()
        .add_function("inspect", Vec::new(), origin())
        .unwrap();
    let first = LoanId { function, index: 0 };
    let second = LoanId { function, index: 1 };
    let child = LoanId { function, index: 2 };
    let reference = |origin| Reference {
        lineage: BTreeSet::from([origin]),
        origin,
        live: true,
    };

    // The backedge passed the same borrowed parameters in the opposite order.
    let mut state = State::default();
    state.loans.insert(first, reference(second));
    state.loans.insert(second, reference(first));
    state.loans.insert(
        child,
        Reference {
            lineage: BTreeSet::from([second, child]),
            ..reference(child)
        },
    );
    state.end(first);

    assert!(!state.loans[&first].live);
    assert!(state.loans[&second].live);
    assert!(!state.loans[&child].live);
}

#[test]
fn nullable_string_reborrow_accepts_only_widening_and_keeps_the_source_live() {
    assert_eq!(
        verify_program(&nullable_string_reborrow_program(false, false)),
        Ok(())
    );
    assert!(has_kind(
        &errors(&nullable_string_reborrow_program(true, false)),
        |kind| matches!(kind, VerifyErrorKind::OperationContract { .. }),
    ));
    assert!(has_kind(
        &errors(&nullable_string_reborrow_program(false, true)),
        |kind| matches!(kind, VerifyErrorKind::LoanDependencyActive { .. }),
    ));
}

fn nullable_string_reborrow_program(reverse: bool, end_source_first: bool) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("nullable-string-widening");
    let module = program.module_mut(module_id).unwrap();
    let string = module.add_string_owner_type();
    let nullable = module.add_inline_nullable_type(string).unwrap();
    let (actual, expected) = if reverse {
        (nullable, string)
    } else {
        (string, nullable)
    };
    let shared = |target| EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    };
    let function_id = module
        .add_function("reborrow", Vec::new(), origin.clone())
        .unwrap();
    let function = module.function_mut(function_id).unwrap();
    let entry = function
        .add_block(vec![shared(actual)], origin.clone())
        .unwrap();
    let source = entity_loan(function.block(entry).unwrap().parameters[0]);
    let (_, results) = function
        .append_instruction(
            entry,
            Operation::SharedReborrow { source },
            vec![shared(expected)],
            origin.clone(),
        )
        .unwrap();
    let joined = function
        .add_block(vec![shared(actual), shared(expected)], origin.clone())
        .unwrap();
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: joined,
                arguments: vec![EntityId::Loan(source), results[0]],
            }),
            origin.clone(),
        )
        .unwrap();
    let parameters = &function.block(joined).unwrap().parameters;
    let source = entity_loan(parameters[0]);
    let view = entity_loan(parameters[1]);
    let order = if end_source_first {
        [source, view]
    } else {
        [view, source]
    };
    for loan in order {
        function
            .append_instruction(
                joined,
                Operation::BorrowEnd { loan },
                Vec::new(),
                origin.clone(),
            )
            .unwrap();
    }
    function
        .set_terminator(
            joined,
            TerminatorKind::Return { values: Vec::new() },
            origin,
        )
        .unwrap();
    program
}
