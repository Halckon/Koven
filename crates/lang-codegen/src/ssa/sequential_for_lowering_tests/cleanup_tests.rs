//! SPEC-0182: exact entity/edge cleanup assertions for one temporary provider.
use super::{
    super::model::{
        BinaryOperator, Block, ComparisonOperator, Edge, EntityId, Function, FunctionId,
        Instruction, LoanId, LoanKind, Operation, PlaceAccess, ScalarConstant, TerminatorKind,
        ValueId,
    },
    analyze, defining_instruction, incoming_edges, parameter_slot,
};

#[derive(Clone, Copy, Debug)]
enum Exit {
    Normal,
    Continue,
    Break,
    Return,
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value: {entity:?}")
    };
    value
}

fn loan(entity: EntityId) -> LoanId {
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

fn transport(function: &Function, edge: &Edge, entity: EntityId) -> EntityId {
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

fn cleanup(function: &Function, block: &Block) -> Vec<Operation> {
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
    eprintln!("temporary_cleanup_{exit:?}.ko:\n{text}");
    let program = analyze(&text);
    let named = |name| {
        program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.name == name)
            .unwrap()
    };
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
                EntityId::Value(snapshot)
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
