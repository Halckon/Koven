//! Exact source-to-CFG assertions for the complete bounded caller.
use super::{e2_function::EmittedFunction, model::*};
use lang_frontend::ownership_checking::two_source_experiment::FunctionShell;
fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(id) = entity else {
        panic!("value")
    };
    id
}
fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(id) = entity else {
        panic!("loan")
    };
    id
}
pub(super) fn assert_caller(emitted: &EmittedFunction, shell: &FunctionShell) {
    let module = &emitted.fragments.leaves.program.modules[0];
    let f = module.function(emitted.caller).unwrap();
    assert_eq!(f.origin, Origin::Source(shell.body()));
    assert!(f.return_types.is_empty());
    assert_eq!(f.borrow_return, None);
    let [entry, header, split, a, b, join, success, exit, abort] = f.blocks.as_slice() else {
        panic!("complete CFG")
    };
    let roles = shell.roles();
    assert_eq!(entry.parameters.len(), 4);
    for (role, expected) in roles.into_iter().zip([
        SsaTypeKind::StringOwner,
        SsaTypeKind::StringOwner,
        SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        },
        SsaTypeKind::Boolean,
    ]) {
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = f.entity(entry.parameters[role]).unwrap().ty
        else {
            panic!("original Shared ABI")
        };
        assert_eq!(module.types[target.index()], expected);
    }
    for block in [header, split, a, b, join, success, exit] {
        assert_eq!(block.parameters.len(), 5);
        for (actual, expected) in block.parameters[..4].iter().zip(&entry.parameters) {
            assert_ne!(actual, expected, "per-block identities must rebind");
            assert_eq!(
                f.entity(*actual).unwrap().ty,
                f.entity(*expected).unwrap().ty
            );
        }
    }
    let instructions = |block: &Block| {
        block
            .instructions
            .iter()
            .map(|id| f.instruction(*id).unwrap())
            .collect::<Vec<_>>()
    };
    let branch = |block: &Block, target: &Block, arguments: Vec<EntityId>| {
        assert_eq!(
            block.terminator.as_ref().unwrap().kind,
            TerminatorKind::Branch(Edge {
                target: target.id,
                arguments
            })
        );
    };
    let [init_span, condition_span, choose_span, decrement_span] = shell.control_spans();
    let [zero_span, one_span] = shell.literals();
    let i = instructions(entry);
    assert_eq!(i.len(), 1);
    assert_eq!(
        i[0].operation,
        Operation::Read {
            source: PlaceAccess::Loan(loan(entry.parameters[roles[2]]))
        }
    );
    assert_eq!(i[0].origin, Origin::Source(init_span));
    let mut args = entry.parameters.clone();
    args.push(i[0].results[0]);
    branch(entry, header, args);
    let i = instructions(header);
    assert_eq!(i.len(), 2);
    assert_eq!(
        i[0].operation,
        Operation::Constant(ScalarConstant::Integer(0))
    );
    assert_eq!(i[0].origin, Origin::Source(zero_span));
    assert_eq!(
        i[1].operation,
        Operation::Compare {
            operator: ComparisonOperator::GreaterThan,
            left: value(header.parameters[4]),
            right: value(i[0].results[0])
        }
    );
    assert_eq!(i[1].origin, Origin::Source(condition_span));
    assert_eq!(
        header.terminator.as_ref().unwrap().kind,
        TerminatorKind::Conditional {
            condition: value(i[1].results[0]),
            when_true: Edge {
                target: split.id,
                arguments: header.parameters.clone()
            },
            when_false: Edge {
                target: exit.id,
                arguments: header.parameters.clone()
            }
        }
    );
    let i = instructions(split);
    assert_eq!(i.len(), 1);
    assert_eq!(
        i[0].operation,
        Operation::Read {
            source: PlaceAccess::Loan(loan(split.parameters[roles[3]]))
        }
    );
    assert_eq!(i[0].origin, Origin::Source(choose_span));
    assert_eq!(
        split.terminator.as_ref().unwrap().kind,
        TerminatorKind::Conditional {
            condition: value(i[0].results[0]),
            when_true: Edge {
                target: a.id,
                arguments: split.parameters.clone()
            },
            when_false: Edge {
                target: b.id,
                arguments: split.parameters.clone()
            }
        }
    );
    for (index, arm) in [a, b].into_iter().enumerate() {
        let i = instructions(arm);
        assert_eq!(i.len(), 1);
        let mut args = vec![arm.parameters[roles[0]]; 2];
        args[emitted.fragments.columns[0]] = arm.parameters[roles[0]];
        args[emitted.fragments.columns[1]] = arm.parameters[roles[1]];
        assert_eq!(
            i[0].operation,
            Operation::DirectCall {
                callee: emitted.fragments.arms[index],
                receiver: None,
                arguments: args
            }
        );
        assert!(i[0].results.is_empty());
        branch(arm, join, arm.parameters.clone());
    }
    let i = instructions(join);
    assert_eq!(i.len(), 2);
    assert_eq!(
        i[0].operation,
        Operation::Constant(ScalarConstant::Integer(1))
    );
    assert_eq!(i[0].origin, Origin::Source(one_span));
    assert_eq!(
        i[1].operation,
        Operation::CheckedArithmetic {
            operator: CheckedArithmeticOperator::Subtract,
            left: value(join.parameters[4]),
            right: value(i[0].results[0])
        }
    );
    assert_eq!(i[1].origin, Origin::Source(decrement_span));
    let mut success_args = join.parameters.clone();
    success_args[4] = i[1].results[0];
    let mut abort_args = join.parameters.clone();
    abort_args.push(i[0].results[0]);
    assert_eq!(
        join.terminator.as_ref().unwrap().kind,
        TerminatorKind::Conditional {
            condition: value(i[1].results[1]),
            when_true: Edge {
                target: abort.id,
                arguments: abort_args
            },
            when_false: Edge {
                target: success.id,
                arguments: success_args
            }
        }
    );
    assert!(success.instructions.is_empty());
    branch(success, header, success.parameters.clone());
    assert!(exit.instructions.is_empty());
    assert_eq!(
        exit.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values: vec![] }
    );
    assert!(abort.instructions.is_empty());
    assert_eq!(abort.parameters.len(), 6);
    let term = abort.terminator.as_ref().unwrap();
    assert_eq!(term.kind, TerminatorKind::Abort);
    assert_eq!(
        term.abort,
        Some(AbortReason::CheckedArithmetic {
            operator: CheckedArithmeticOperator::Subtract,
            divisor: value(abort.parameters[5])
        })
    );
    assert_eq!(term.origin, Origin::Source(decrement_span));
    assert!(
        f.instructions
            .iter()
            .all(|i| !matches!(i.operation, Operation::BorrowEnd { .. })),
        "caller does not consume Shared inputs"
    );
}
