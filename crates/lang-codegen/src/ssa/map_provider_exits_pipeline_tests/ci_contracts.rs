//! CI-only: corrupt one dimension of real lowered Map SSA. Never execute locally.
use super::{Report, pipeline};
use crate::ssa::{
    model::*,
    verify::{VerifyErrorKind, VerifyLocation, verify_program},
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

const SOURCE: &str = r#"
fun scan(first: Map<String, String>, second: Map<Int, String>): Unit {
  for ((key, value) in first) { println(key); println(value) }
  for ((key, value) in second) { println(value) }
}
fun entry(): Unit {
  val first = mapOf<String, String>()
  val second = mapOf<Int, String>()
  scan(first, second)
}
"#;

fn baseline(unit: bool) -> Program {
    let mut sources = SourceMap::new();
    let id = sources.add_source("provider.ko", SOURCE).unwrap();
    let lexed = lex(&sources, id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let mut report = Report::default();
    let (program, _) = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |_, _, owned, r| {
                r.check(
                    "original two unit Map providers",
                    owned.map_iterations().len() == 2,
                    owned.map_iterations(),
                );
            },
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |_, _, owned, r| {
            r.check(
                "original two single Map providers",
                owned.map_iterations().len() == 2,
                owned.map_iterations(),
            );
        })
    }
    .unwrap();
    assert!(report.passed(), "{report:#?}");
    verify_program(&program).expect("unmodified real SSA must pass before corruption");
    crate::llvm::render_verified_program(&program).expect("unmodified real LLVM must pass");
    program
}

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
                    == 2
            })
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            panic!("one actual two-provider function")
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
                    // The second entry parameter is a real dominating Map<Int,String> loan.
                    let other = loan(f.blocks[0].parameters[1]);
                    let Operation::MapIterationKeyLoan { source, .. } =
                        &mut f.instructions[key.id.index()].operation
                    else {
                        unreachable!()
                    };
                    *source = other;
                    (
                        key.id,
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
