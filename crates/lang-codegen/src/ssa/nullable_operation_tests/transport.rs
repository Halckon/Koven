//! CFG transport must preserve each owner/proof pairing, not merely origin sets.
use super::*;
use crate::ssa::model::{AbortReason, ScalarConstant};

fn paired_transport_program(swapped_proofs: bool, permute_owners: bool, backedge: bool) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let (module_id, inner, nullable) = add_pointer_types(&mut program);
    let module = program.module_mut(module_id).unwrap();
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let function_id = module
        .add_function("paired", Vec::new(), origin.clone())
        .unwrap();
    let function = module.function_mut(function_id).unwrap();
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: inner,
    };
    let entry_types = vec![
        EntityType::Value(nullable),
        EntityType::Value(nullable),
        EntityType::Value(boolean),
    ];
    let entry = function
        .add_block(entry_types.clone(), origin.clone())
        .unwrap();
    let first_null = function
        .add_block(entry_types.clone(), origin.clone())
        .unwrap();
    let mut first_types = entry_types;
    first_types.push(shared);
    let first_present = function
        .add_block(first_types.clone(), origin.clone())
        .unwrap();
    let second_null = function
        .add_block(first_types.clone(), origin.clone())
        .unwrap();
    let mut both_types = first_types;
    both_types.push(shared);
    let both_present = function
        .add_block(both_types.clone(), origin.clone())
        .unwrap();
    let left = function
        .add_block(both_types.clone(), origin.clone())
        .unwrap();
    let right = function.add_block(both_types, origin.clone()).unwrap();
    let join_types = vec![
        EntityType::Value(nullable),
        EntityType::Value(nullable),
        shared,
        shared,
    ];
    let join = function
        .add_block(join_types.clone(), origin.clone())
        .unwrap();
    let entry_parameters = function.block(entry).unwrap().parameters.clone();
    let first_parameters = function.block(first_present).unwrap().parameters.clone();
    let both_parameters = function.block(both_present).unwrap().parameters.clone();
    function
        .set_terminator(
            entry,
            TerminatorKind::NullableBranch {
                owner: value(entry_parameters[0]),
                when_null: Edge {
                    target: first_null,
                    arguments: entry_parameters.clone(),
                },
                when_non_null: Edge {
                    target: first_present,
                    arguments: entry_parameters,
                },
                view: loan(first_parameters[3]),
            },
            origin.clone(),
        )
        .unwrap();
    function
        .set_terminator(
            first_present,
            TerminatorKind::NullableBranch {
                owner: value(first_parameters[1]),
                when_null: Edge {
                    target: second_null,
                    arguments: first_parameters.clone(),
                },
                when_non_null: Edge {
                    target: both_present,
                    arguments: first_parameters,
                },
                view: loan(both_parameters[4]),
            },
            origin.clone(),
        )
        .unwrap();
    for block in [first_null, second_null] {
        function
            .set_abort_terminator(block, AbortReason::Text("null".into()), origin.clone())
            .unwrap();
    }
    function
        .set_terminator(
            both_present,
            TerminatorKind::Conditional {
                condition: value(both_parameters[2]),
                when_true: Edge {
                    target: left,
                    arguments: both_parameters.clone(),
                },
                when_false: Edge {
                    target: right,
                    arguments: both_parameters,
                },
            },
            origin.clone(),
        )
        .unwrap();
    let left_parameters = function.block(left).unwrap().parameters.clone();
    let right_parameters = function.block(right).unwrap().parameters.clone();
    let (left_first_proof, left_second_proof) = if swapped_proofs { (4, 3) } else { (3, 4) };
    function
        .set_terminator(
            left,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: vec![
                    left_parameters[0],
                    left_parameters[1],
                    left_parameters[left_first_proof],
                    left_parameters[left_second_proof],
                ],
            }),
            origin.clone(),
        )
        .unwrap();
    // With permutation, both owner/proof columns have {first, second} origins.
    // In the negative case each edge crosses the pairs, despite identical origin sets.
    let (first_owner, second_owner) = if permute_owners { (1, 0) } else { (0, 1) };
    let (first_proof, second_proof) = if swapped_proofs || !permute_owners {
        (3, 4)
    } else {
        (4, 3)
    };
    function
        .set_terminator(
            right,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: vec![
                    right_parameters[first_owner],
                    right_parameters[second_owner],
                    right_parameters[first_proof],
                    right_parameters[second_proof],
                ],
            }),
            origin.clone(),
        )
        .unwrap();
    let output = if backedge {
        let output = function.add_block(join_types, origin.clone()).unwrap();
        let condition = append_value(
            function,
            join,
            Operation::Constant(ScalarConstant::Boolean(false)),
            boolean,
            &origin,
        );
        let parameters = function.block(join).unwrap().parameters.clone();
        function
            .set_terminator(
                join,
                TerminatorKind::Conditional {
                    condition,
                    when_true: Edge {
                        target: join,
                        arguments: parameters.clone(),
                    },
                    when_false: Edge {
                        target: output,
                        arguments: parameters,
                    },
                },
                origin.clone(),
            )
            .unwrap();
        output
    } else {
        join
    };
    let parameters = function.block(output).unwrap().parameters.clone();
    for index in 0..2 {
        let taken = append_value(
            function,
            output,
            Operation::NullableTake {
                owner: value(parameters[index]),
                proof: loan(parameters[index + 2]),
            },
            inner,
            &origin,
        );
        function
            .append_instruction(
                output,
                Operation::Drop { owner: taken },
                Vec::new(),
                origin.clone(),
            )
            .unwrap();
    }
    function
        .set_terminator(
            output,
            TerminatorKind::Return { values: Vec::new() },
            origin,
        )
        .unwrap();
    program
}

#[test]
fn nullable_take_accepts_paired_diamond_and_grounded_loop_transport() {
    for backedge in [false, true] {
        verify_program(&paired_transport_program(false, false, backedge)).unwrap();
    }
}

#[test]
fn nullable_take_rejects_swapped_pairs_even_with_identical_origin_sets() {
    for backedge in [false, true] {
        let program = paired_transport_program(true, true, backedge);
        let errors = verify_program(&program)
            .expect_err("same origins do not prove pairing")
            .errors;
        let function = &program.modules[0].functions[0];
        let takes = function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::NullableTake { .. }));
        for take in takes {
            assert!(
                errors.iter().any(|error| error.location
                    == super::super::verify::VerifyLocation::Instruction(take.id)
                    && matches!(error.kind, VerifyErrorKind::OperationContract { .. })),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn nullable_take_local_contract_preserves_pairs_under_simultaneous_permutation() {
    // The global alias analysis may conservatively overlap both joined owners.
    // This test checks only the local pairing contract, not full ownership acceptance.
    for backedge in [false, true] {
        let program = paired_transport_program(false, true, backedge);
        let module = &program.modules[0];
        let function = &module.functions[0];
        let mut errors = Vec::new();
        for instruction in &function.instructions {
            if matches!(instruction.operation, Operation::NullableTake { .. }) {
                super::super::verify_operation::verify_operation(
                    module,
                    function,
                    instruction,
                    &mut errors,
                );
            }
        }
        assert!(errors.is_empty(), "{errors:?}");
    }
}

#[test]
fn nullable_take_rejects_an_ungrounded_cycle_even_beside_real_births() {
    let mut program = paired_transport_program(false, false, false);
    let module = &mut program.modules[0];
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let function = &mut module.functions[0];
    let origin = function.origin.clone();
    let take = function
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::NullableTake { .. }))
        .unwrap();
    let join = take.block;
    let take_id = take.id;
    let types = function
        .block(join)
        .unwrap()
        .parameters
        .iter()
        .map(|entity| function.entity(*entity).unwrap().ty)
        .collect();
    let cycle = function.add_block(types, origin.clone()).unwrap();
    let parameters = function.block(cycle).unwrap().parameters.clone();
    let condition = append_value(
        function,
        cycle,
        Operation::Constant(ScalarConstant::Boolean(false)),
        boolean,
        &origin,
    );
    function
        .set_terminator(
            cycle,
            TerminatorKind::Conditional {
                condition,
                when_true: Edge {
                    target: cycle,
                    arguments: parameters.clone(),
                },
                when_false: Edge {
                    target: join,
                    arguments: parameters,
                },
            },
            origin,
        )
        .unwrap();
    // The join also has valid births, but they must not ground this separate SCC.
    let mut errors = Vec::new();
    let module = &program.modules[0];
    let function = &module.functions[0];
    super::super::verify_operation::verify_operation(
        module,
        function,
        function.instruction(take_id).unwrap(),
        &mut errors,
    );
    assert!(
        errors.iter().any(|error| error.location
            == super::super::verify::VerifyLocation::Instruction(take_id)
            && matches!(error.kind, VerifyErrorKind::OperationContract { .. })),
        "{errors:?}"
    );
}
