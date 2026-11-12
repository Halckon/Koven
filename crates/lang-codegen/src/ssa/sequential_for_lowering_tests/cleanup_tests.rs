//! SPEC-0182: exact entity/edge cleanup assertions for one temporary provider.
use super::{
    super::model::{
        BinaryOperator, Block, ComparisonOperator, Edge, EntityId, Function, FunctionId,
        Instruction, LoanId, LoanKind, Operation, PlaceAccess, ScalarConstant, TerminatorKind,
        ValueId,
    },
    analyze, defining_instruction, incoming_edges, parameter_slot,
};
use crate::test_support::diagnostics::diagnostic;

#[derive(Clone, Copy, Debug)]
enum Exit {
    Normal,
    Continue,
    Break,
    Return,
}

pub(super) fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value: {entity:?}")
    };
    value
}

pub(super) fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(loan) = entity else {
        panic!("expected loan: {entity:?}")
    };
    loan
}

fn only_instruction(function: &Function, predicate: impl Fn(&Operation) -> bool) -> &Instruction {
    let matches = function
        .instructions
        .iter()
        .filter(|i| predicate(&i.operation))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "expected one instruction: {matches:?}");
    matches[0]
}

fn call(function: &Function, callee: FunctionId) -> &Instruction {
    only_instruction(
        function,
        |operation| matches!(operation, Operation::DirectCall { callee: actual, .. } if *actual == callee),
    )
}

pub(super) fn transport(function: &Function, edge: &Edge, entity: EntityId) -> EntityId {
    let slots = edge
        .arguments
        .iter()
        .enumerate()
        .filter(|(_, arg)| **arg == entity)
        .map(|(slot, _)| slot)
        .collect::<Vec<_>>();
    assert_eq!(slots.len(), 1, "one transport slot for {entity:?}");
    function.block(edge.target).unwrap().parameters[slots[0]]
}

pub(super) fn cleanup(function: &Function, block: &Block) -> Vec<Operation> {
    block
        .instructions
        .iter()
        .filter_map(|id| {
            let operation = &function.instruction(*id).unwrap().operation;
            matches!(
                operation,
                Operation::Drop { .. } | Operation::BorrowEnd { .. }
            )
            .then(|| operation.clone())
        })
        .collect()
}

fn position(block: &Block, instruction: &Instruction) -> usize {
    assert_eq!(instruction.block, block.id);
    block
        .instructions
        .iter()
        .position(|id| *id == instruction.id)
        .unwrap()
}

fn assert_temporary_cleanup(exit: Exit) {
    let jump = match exit {
        Exit::Normal => "",
        Exit::Continue => "continue",
        Exit::Break => "break",
        Exit::Return => "return x",
    };
    let text = format!(
        r#"
        class Guard(val name: String) {{ deinit() {{ println(this.name) }} }}
        fun source(): Array<Int> = arrayOf(7, 2)
        fun earlier(): Guard = Guard("earlier")
        fun later(): Guard = Guard("later")
        fun run(): Int {{
            for (x in source()) {{
                val first = earlier()
                val second = later()
                {jump}
            }}
            return 0
        }}
    "#
    );
    diagnostic!("temporary_cleanup_{exit:?}.ko:\n{text}");
    let analysis = analyze(&text, "run");
    let named = |name| analysis.function(name);
    let run = named("run");
    let source = call(run, named("source").id);
    let first = call(run, named("earlier").id);
    let second = call(run, named("later").id);
    let owner = source.results[0];
    let length = only_instruction(run, |op| matches!(op, Operation::ContainerLength { .. }));
    let Operation::ContainerLength { owner: source_loan } = length.operation else {
        unreachable!()
    };
    let borrow = defining_instruction(run, source_loan);
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = borrow.operation
    else {
        panic!("shared source borrow")
    };
    assert_eq!(
        defining_instruction(run, EntityId::Place(place)).operation,
        Operation::RootPlace {
            owner: value(owner)
        }
    );
    let preheader = run.block(run.entry_block().unwrap()).unwrap();
    assert_eq!(source.block, preheader.id);
    assert_eq!(length.block, preheader.id);
    assert!(incoming_edges(run, preheader.id).is_empty());
    let TerminatorKind::Branch(enter) = &preheader.terminator.as_ref().unwrap().kind else {
        panic!("enter header")
    };
    let header = run.block(enter.target).unwrap();
    let header_owner = transport(run, enter, owner);
    let header_loan = transport(run, enter, source_loan);
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("guard body")
    };
    let Operation::Compare {
        operator: ComparisonOperator::LessThan,
        left: cursor,
        right: snapshot,
    } = defining_instruction(run, EntityId::Value(*condition)).operation
    else {
        panic!("cursor < snapshot")
    };
    assert_eq!(
        transport(run, enter, length.results[0]),
        EntityId::Value(snapshot)
    );
    let body = run.block(when_true.target).unwrap();
    let exhausted = run.block(when_false.target).unwrap();
    assert_ne!(body.id, exhausted.id);
    assert_eq!(incoming_edges(run, body.id), [(header.id, when_true)]);
    let body_owner = transport(run, when_true, header_owner);
    let body_loan = transport(run, when_true, header_loan);
    let body_cursor = transport(run, when_true, EntityId::Value(cursor));
    let body_snapshot = transport(run, when_true, EntityId::Value(snapshot));
    let element = only_instruction(run, |op| {
        matches!(op, Operation::ContainerElementPlace { .. })
    });
    assert_eq!(element.block, body.id);
    assert_eq!(
        element.operation,
        Operation::ContainerElementPlace {
            owner: body_loan,
            index: value(body_cursor)
        }
    );
    let element_borrow = only_instruction(
        run,
        |op| matches!(op, Operation::BorrowBegin { place, kind: LoanKind::Shared } if EntityId::Place(*place) == element.results[0]),
    );
    let element_loan = loan(element_borrow.results[0]);
    assert_eq!(element_borrow.block, body.id);
    assert!(position(body, first) < position(body, second));

    // Handwritten contract order, independent of the frontend exit plan/provider builder.
    let local_cleanup = vec![
        Operation::Drop {
            owner: value(second.results[0]),
        },
        Operation::Drop {
            owner: value(first.results[0]),
        },
        Operation::BorrowEnd { loan: element_loan },
    ];
    let mut expected_body = local_cleanup;
    if matches!(exit, Exit::Break | Exit::Return) {
        expected_body.extend([
            Operation::BorrowEnd {
                loan: loan(body_loan),
            },
            Operation::Drop {
                owner: value(body_owner),
            },
        ]);
    }
    assert_eq!(cleanup(run, body), expected_body);
    assert_eq!(
        cleanup(run, exhausted),
        [
            Operation::BorrowEnd {
                loan: loan(transport(run, when_false, header_loan))
            },
            Operation::Drop {
                owner: value(transport(run, when_false, header_owner))
            },
        ]
    );
    // No early source drop, element consume/drop, or second cleanup in any other block.
    for block in &run.blocks {
        if block.id != body.id && block.id != exhausted.id {
            assert!(
                cleanup(run, block).is_empty(),
                "unexpected cleanup in {block:?}"
            );
        }
    }
    assert!(
        !run.instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::Consume { .. }))
    );
    let first_drop = only_instruction(run, |op| {
        *op == Operation::Drop {
            owner: value(second.results[0]),
        }
    });
    let element_end =
        only_instruction(run, |op| *op == Operation::BorrowEnd { loan: element_loan });
    assert!(position(body, second) < position(body, first_drop));

    match exit {
        Exit::Normal | Exit::Continue => {
            let TerminatorKind::Branch(backedge) = &body.terminator.as_ref().unwrap().kind else {
                panic!("continue to header")
            };
            assert_eq!(backedge.target, header.id);
            assert_eq!(
                incoming_edges(run, header.id),
                [(preheader.id, enter), (body.id, backedge)]
            );
            assert_eq!(
                backedge.arguments[parameter_slot(header, header_owner)],
                body_owner
            );
            assert_eq!(
                backedge.arguments[parameter_slot(header, header_loan)],
                body_loan
            );
            assert_eq!(
                backedge.arguments[parameter_slot(header, EntityId::Value(snapshot))],
                body_snapshot
            );
            let advance = defining_instruction(
                run,
                backedge.arguments[parameter_slot(header, EntityId::Value(cursor))],
            );
            let Operation::Binary {
                operator: BinaryOperator::Add,
                left,
                right,
            } = advance.operation
            else {
                panic!("advance cursor")
            };
            assert_eq!(EntityId::Value(left), body_cursor);
            assert_eq!(
                defining_instruction(run, EntityId::Value(right)).operation,
                Operation::Constant(ScalarConstant::Integer(1))
            );
            assert!(position(body, element_end) < position(body, advance));
            assert!(matches!(
                exhausted.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            ));
        }
        Exit::Break => {
            let TerminatorKind::Branch(break_edge) = &body.terminator.as_ref().unwrap().kind else {
                panic!("break to exit")
            };
            let TerminatorKind::Branch(exhaustion_edge) =
                &exhausted.terminator.as_ref().unwrap().kind
            else {
                panic!("exhaustion to exit")
            };
            assert_eq!(break_edge.target, exhaustion_edge.target);
            let merged = run.block(break_edge.target).unwrap();
            assert!(
                merged.parameters.is_empty(),
                "finished provider must not escape"
            );
            assert_eq!(
                incoming_edges(run, merged.id),
                [(body.id, break_edge), (exhausted.id, exhaustion_edge)]
            );
            assert!(matches!(
                merged.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            ));
            assert_eq!(incoming_edges(run, header.id), [(preheader.id, enter)]);
        }
        Exit::Return => {
            let TerminatorKind::Return { values } = &body.terminator.as_ref().unwrap().kind else {
                panic!("return copied element")
            };
            assert_eq!(values.len(), 1);
            let read = defining_instruction(run, EntityId::Value(values[0]));
            assert_eq!(
                read.operation,
                Operation::Read {
                    source: PlaceAccess::Loan(element_loan)
                }
            );
            assert!(
                position(body, read) < position(body, first_drop),
                "return operand precedes resource cleanup"
            );
            assert_eq!(incoming_edges(run, header.id), [(preheader.id, enter)]);
            assert!(matches!(
                exhausted.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            ));
        }
    }
}

#[test]
fn normal_exit_keeps_source_until_exhaustion() {
    assert_temporary_cleanup(Exit::Normal);
}
#[test]
fn continue_exit_keeps_source_until_exhaustion() {
    assert_temporary_cleanup(Exit::Continue);
}
#[test]
fn break_exit_cleans_source_before_join() {
    assert_temporary_cleanup(Exit::Break);
}
#[test]
fn return_exit_copies_operand_before_cleanup() {
    assert_temporary_cleanup(Exit::Return);
}

pub(in crate::ssa) mod transport {
//! CFG transport must preserve each owner/proof pairing, not merely origin sets.
use crate::ssa::nullable_operation_tests::*;
use crate::ssa::model::{AbortReason, ScalarConstant};
use crate::test_support::diagnostics::assert_diagnostic;

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
            assert_diagnostic!(
                errors.iter().any(|error| error.location
                    == crate::ssa::verify::VerifyLocation::Instruction(take.id)
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
                crate::ssa::verify_operation::verify_operation(
                    module,
                    function,
                    instruction,
                    &mut errors,
                );
            }
        }
        assert_diagnostic!(errors.is_empty(), "{errors:?}");
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
    crate::ssa::verify_operation::verify_operation(
        module,
        function,
        function.instruction(take_id).unwrap(),
        &mut errors,
    );
    assert_diagnostic!(
        errors.iter().any(|error| error.location
            == crate::ssa::verify::VerifyLocation::Instruction(take_id)
            && matches!(error.kind, VerifyErrorKind::OperationContract { .. })),
        "{errors:?}"
    );
}
}
