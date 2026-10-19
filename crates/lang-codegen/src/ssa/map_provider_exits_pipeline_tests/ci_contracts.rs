//! CI-only: corrupt one dimension of real lowered Map SSA. Never execute locally.
use super::ci_baseline::baseline;
use crate::ssa::{
    model::*,
    verify::{VerifyErrorKind, VerifyLocation, verify_program},
};

fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(id) = entity else {
        panic!("actual loan result")
    };
    id
}
fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(id) = entity else {
        panic!("actual value result")
    };
    id
}

// Preserve IDs, block membership, origins and terminators. Only the requested operation is added.
fn insert_before(
    f: &mut Function,
    before: InstructionId,
    op: Operation,
    results: Vec<EntityType>,
) -> Vec<EntityId> {
    let original = f.instruction(before).unwrap().clone();
    let block = original.block;
    let position = f.blocks[block.index()]
        .instructions
        .iter()
        .position(|id| *id == before)
        .unwrap();
    let terminator = f.blocks[block.index()].terminator.take();
    let (id, entities) = f
        .append_instruction(block, op, results, original.origin)
        .unwrap();
    let instructions = &mut f.blocks[block.index()].instructions;
    assert_eq!(instructions.pop(), Some(id));
    instructions.insert(position, id);
    f.blocks[block.index()].terminator = terminator;
    entities
}

#[test]
fn map_real_lowered_ssa_private_damage_has_precise_rejections() {
    const TYPE_ERROR: &str = "operand and result types do not match the operation contract";
    for unit in [false, true] {
        let mut valid = baseline(unit);
        let functions = valid
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .filter(|f| {
                f.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. }))
                    .count()
                    == 1
            })
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            panic!("one actual provider function")
        };
        let function = (*function).clone();
        let fid = function.id();
        let guards = function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::MapIterationHasNext { .. }))
            .collect::<Vec<_>>();
        let guard = guards[0];
        let key = function
            .instructions
            .iter()
            .find(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. }))
            .unwrap();
        let val = function
            .instructions
            .iter()
            .find(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. }))
            .unwrap();
        let next = function
            .instructions
            .iter()
            .find(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
            .unwrap();
        let Operation::MapIterationKeyLoan { source, .. } = key.operation else {
            unreachable!()
        };
        let boolean = match function.entity(guard.results[0]).unwrap().ty {
            EntityType::Value(t) => t,
            _ => unreachable!(),
        };
        for fault in 0..8 {
            let f = valid
                .module_mut(fid.module())
                .unwrap()
                .function_mut(fid)
                .unwrap();
            *f = function.clone();
            let (at, expected) = match fault {
                0 | 1 => {
                    let Operation::MapIterationHasNext { cursor, .. } = guard.operation else {
                        unreachable!()
                    };
                    let ty = if fault == 0 {
                        EntityType::Value(boolean)
                    } else {
                        f.entity(EntityId::Value(cursor)).unwrap().ty
                    };
                    let constant = if fault == 0 {
                        ScalarConstant::Boolean(true)
                    } else {
                        ScalarConstant::Integer(0)
                    };
                    let replacement = value(
                        insert_before(f, guard.id, Operation::Constant(constant), vec![ty])[0],
                    );
                    let Operation::MapIterationHasNext { cursor, .. } =
                        &mut f.instructions[guard.id.index()].operation
                    else {
                        unreachable!()
                    };
                    *cursor = replacement;
                    (
                        guard.id,
                        VerifyErrorKind::OperationContract {
                            reason: if fault == 0 {
                                TYPE_ERROR
                            } else {
                                "Map cursor must originate from First/Next and explicit CFG transport"
                            },
                        },
                    )
                }
                2 => {
                    // A real, dominating active String loan cannot serve as a Map provider source.
                    let wrong = loan(key.results[0]);
                    let Operation::MapIterationValueLoan { source, .. } =
                        &mut f.instructions[val.id.index()].operation
                    else {
                        unreachable!()
                    };
                    *source = wrong;
                    (
                        val.id,
                        VerifyErrorKind::OperationContract { reason: TYPE_ERROR },
                    )
                }
                3 => {
                    insert_before(f, key.id, Operation::BorrowEnd { loan: source }, vec![]);
                    (key.id, VerifyErrorKind::LoanInactive { loan: source })
                }
                4 | 5 => {
                    let component = loan(if fault == 4 {
                        key.results[0]
                    } else {
                        val.results[0]
                    });
                    let end = f
                        .instructions
                        .iter_mut()
                        .find(|i| i.operation == Operation::BorrowEnd { loan: component })
                        .expect("actual component End before Next");
                    end.operation = Operation::PrintString { value: component };
                    let Operation::MapIterationNext { source: parent, .. } = next.operation else {
                        unreachable!()
                    };
                    (
                        next.id,
                        VerifyErrorKind::LoanDependencyActive {
                            parent,
                            dependent: component,
                        },
                    )
                }
                6 => {
                    let component = loan(key.results[0]);
                    let EntityType::Loan { target, .. } = &mut f.loans[component.index()].ty else {
                        unreachable!()
                    };
                    *target = boolean;
                    (
                        key.id,
                        VerifyErrorKind::OperationContract { reason: TYPE_ERROR },
                    )
                }
                7 => {
                    // A complete, current-block same-type reborrow is a distinct lease.
                    let ty = f.entity(EntityId::Loan(source)).unwrap().ty;
                    let caller = loan(
                        insert_before(f, key.id, Operation::SharedReborrow { source }, vec![ty])[0],
                    );
                    insert_before(f, next.id, Operation::BorrowEnd { loan: caller }, vec![]);
                    verify_program(&valid).expect(
                        "extra balanced lease is valid before the single source substitution",
                    );
                    let f = valid
                        .module_mut(fid.module())
                        .unwrap()
                        .function_mut(fid)
                        .unwrap();
                    let Operation::MapIterationKeyLoan { source, .. } =
                        &mut f.instructions[key.id.index()].operation
                    else {
                        unreachable!()
                    };
                    *source = caller;
                    (
                        key.id,
                        VerifyErrorKind::OperationContract {
                            reason: "Map cursor and source must belong to the same live provider lease",
                        },
                    )
                }
                _ => unreachable!(),
            };
            let errors = verify_program(&valid).expect_err("private damage must not reach LLVM");
            let origin = function.instruction(at).unwrap().origin.clone();
            assert!(
                errors
                    .errors
                    .iter()
                    .any(|e| e.location == VerifyLocation::Instruction(at)
                        && e.kind == expected
                        && e.origin == Some(origin.clone())),
                "unit={unit} fault={fault} expected={expected:?} at={at:?} original={origin:?}: {errors:#?}"
            );
            println!(
                "SPEC0331 CI MAP SSA unit={unit} fault={fault} exact={expected:?} at={at:?} origin={origin:?}"
            );
        }
    }
}
