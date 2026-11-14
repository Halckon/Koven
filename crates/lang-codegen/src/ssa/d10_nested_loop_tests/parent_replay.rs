//! Inspect the actual parent-Replace / child-counter graph without rewriting SSA.
//! Fixture replay only; the caller supplies the separately bound nested source capability.
use super::*;
pub(super) fn replay(
    unit: &OwnedCompilationUnitView<'_, '_>,
    program: &Program,
    id: FunctionId,
) -> Option<()> {
    verify::verify_program(program).ok()?;
    let source = source::facts(unit)?;
    let [module] = program.modules.as_slice() else {
        return None;
    };
    let [f] = module.functions.as_slice() else {
        return None;
    };
    if f.id() != id
        || f.blocks.len() != 11
        || f.instructions.len() != 23
        || f.origin != source.callable
        || f.blocks.iter().any(|b| b.origin != source.callable)
        || !f.return_types.is_empty()
        || f.receiver.is_some()
        || f.borrow_return.is_some()
        || f.carrier_return.is_some()
        || f.never_returns
    {
        return None;
    }
    let i = &f.instructions;
    let value = |n: usize, r: usize| match i.get(n)?.results.get(r)? {
        EntityId::Value(v) => Some(*v),
        _ => None,
    };
    let block = |n: usize| f.block(i[n].block);
    let e = f.block(f.entry_block()?)?;
    let ph = block(3)?;
    let pb = block(5)?;
    let pf = block(11)?;
    let ch = block(17)?;
    let cb = block(19)?;
    let cf = block(21)?;
    let [EntityId::Loan(parameter)] = e.parameters.as_slice() else {
        return None;
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: int,
    } = f.entity(EntityId::Loan(*parameter))?.ty
    else {
        return None;
    };
    if module.types.get(int.index())?
        != &(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        })
    {
        return None;
    }
    fn carried(
        block: &Block,
        child: bool,
    ) -> Option<(LoanId, ValueId, ValueId, ValueId, Option<ValueId>)> {
        match (child, block.parameters.as_slice()) {
            (
                false,
                [
                    EntityId::Loan(l),
                    EntityId::Value(a),
                    EntityId::Value(b),
                    EntityId::Value(n),
                ],
            ) => Some((*l, *a, *b, *n, None)),
            (
                true,
                [
                    EntityId::Loan(l),
                    EntityId::Value(a),
                    EntityId::Value(b),
                    EntityId::Value(n),
                    EntityId::Value(k),
                ],
            ) => Some((*l, *a, *b, *n, Some(*k))),
            _ => None,
        }
    }
    let hp = carried(ph, false)?;
    let bp = carried(pb, false)?;
    let hc = carried(ch, true)?;
    let bc = carried(cb, true)?;
    let fc = carried(cf, false)?;
    let [EntityId::Value(final_a), EntityId::Value(final_b)] = pf.parameters.as_slice() else {
        return None;
    };
    let [EntityId::Place(suffix_place)] = i[12].results.as_slice() else {
        return None;
    };
    let [EntityId::Loan(suffix_loan)] = i[13].results.as_slice() else {
        return None;
    };
    let [EntityId::Place(replace_place)] = i[5].results.as_slice() else {
        return None;
    };
    let [EntityId::Loan(replace_loan)] = i[6].results.as_slice() else {
        return None;
    };
    for (n, literal) in [0, 1, 7].into_iter().zip(&source.literals) {
        let Operation::StringLiteral { string, bytes } = &i[n].operation else {
            return None;
        };
        if module.types.get(string.index())? != &SsaTypeKind::StringOwner
            || bytes != &literal.1
            || i[n].origin != literal.0
        {
            return None;
        }
    }
    let expected = [
        (
            2,
            Operation::Read {
                source: PlaceAccess::Loan(*parameter),
            },
        ),
        (3, Operation::Constant(ScalarConstant::Integer(0))),
        (
            4,
            Operation::Compare {
                operator: ComparisonOperator::GreaterThan,
                left: hp.3,
                right: value(3, 0)?,
            },
        ),
        (
            10,
            Operation::Read {
                source: PlaceAccess::Loan(bp.0),
            },
        ),
        (11, Operation::Drop { owner: *final_a }),
        (12, Operation::RootPlace { owner: *final_b }),
        (
            13,
            Operation::BorrowBegin {
                place: *suffix_place,
                kind: LoanKind::Shared,
            },
        ),
        (
            14,
            Operation::PrintString {
                value: *suffix_loan,
            },
        ),
        (15, Operation::BorrowEnd { loan: *suffix_loan }),
        (16, Operation::Drop { owner: *final_b }),
        (17, Operation::Constant(ScalarConstant::Integer(0))),
        (
            18,
            Operation::Compare {
                operator: ComparisonOperator::GreaterThan,
                left: hc.4?,
                right: value(17, 0)?,
            },
        ),
        (5, Operation::RootPlace { owner: bp.1 }),
        (
            6,
            Operation::BorrowBegin {
                place: *replace_place,
                kind: LoanKind::Exclusive,
            },
        ),
        (
            8,
            Operation::RootReplace {
                owner: bp.1,
                loan: *replace_loan,
                replacement: value(7, 0)?,
            },
        ),
        (
            9,
            Operation::Drop {
                owner: value(8, 1)?,
            },
        ),
        (19, Operation::Constant(ScalarConstant::Integer(1))),
        (
            20,
            Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Subtract,
                left: bc.4?,
                right: value(19, 0)?,
            },
        ),
        (21, Operation::Constant(ScalarConstant::Integer(1))),
        (
            22,
            Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Subtract,
                left: fc.3,
                right: value(21, 0)?,
            },
        ),
    ];
    if expected.iter().any(|(n, op)| i[*n].operation != *op) {
        return None;
    }
    for (n, origin) in [
        (2, &source.reads[0]),
        (10, &source.reads[1]),
        (4, &source.conditions[0]),
        (18, &source.conditions[1]),
        (11, &source.drops[1]),
        (16, &source.drops[2]),
        (9, &source.drops[0]),
        (12, &source.borrow[1]),
        (13, &source.borrow[1]),
        (5, &source.borrow[0]),
        (6, &source.borrow[0]),
        (14, &source.calls[1]),
        (15, &source.calls[1]),
        (8, &source.calls[0]),
        (20, &source.steps[1]),
        (22, &source.steps[0]),
    ] {
        if i[n].origin != *origin {
            return None;
        }
    }
    // Check the scalar literal origins too, rather than only their numeric values.
    let [input] = unit.inputs() else {
        return None;
    };
    for (n, enclosing, assignment) in [
        (3, &source.conditions[0], false),
        (17, &source.conditions[1], false),
        (19, &source.steps[1], true),
        (21, &source.steps[0], true),
    ] {
        let Origin::Source(span) = enclosing else {
            return None;
        };
        let mut nodes = input
            .parsed()
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| node.span() == *span);
        let (_, node) = nodes.next()?;
        if nodes.next().is_some() {
            return None;
        }
        let rhs = match (assignment, node.payload()) {
            (false, lang_frontend::parser::Expression::Binary { right, .. }) => *right,
            (true, lang_frontend::parser::Expression::Assignment { value, .. }) => *value,
            _ => return None,
        };
        if i[n].origin != Origin::Source(input.parsed().ast().expressions().get(rhs).ok()?.span()) {
            return None;
        }
    }
    fn term(b: &Block) -> Option<&TerminatorKind> {
        Some(&b.terminator.as_ref()?.kind)
    }
    let edge = |target, arguments| Edge { target, arguments };
    let args = |l, a, b, n, k: Option<ValueId>| {
        std::iter::once(EntityId::Loan(l))
            .chain([EntityId::Value(a), EntityId::Value(b), EntityId::Value(n)])
            .chain(k.map(EntityId::Value))
            .collect::<Vec<_>>()
    };
    if term(e)?
        != &TerminatorKind::Branch(edge(
            ph.id,
            args(*parameter, value(0, 0)?, value(1, 0)?, value(2, 0)?, None),
        ))
        || term(ph)?
            != &(TerminatorKind::Conditional {
                condition: value(4, 0)?,
                when_true: edge(pb.id, ph.parameters.clone()),
                when_false: edge(pf.id, vec![EntityId::Value(hp.1), EntityId::Value(hp.2)]),
            })
        || term(pb)?
            != &TerminatorKind::Branch(edge(
                ch.id,
                args(bp.0, value(8, 0)?, bp.2, bp.3, Some(value(10, 0)?)),
            ))
        || term(ch)?
            != &(TerminatorKind::Conditional {
                condition: value(18, 0)?,
                when_true: edge(cb.id, ch.parameters.clone()),
                when_false: edge(cf.id, args(hc.0, hc.1, hc.2, hc.3, None)),
            })
        || term(pf)? != &(TerminatorKind::Return { values: vec![] })
    {
        return None;
    }
    let mut all = vec![e, ph, pb, pf, ch, cb, cf];
    for (from, checked, rhs, arguments, header, control, step, child) in [
        (
            cb,
            20,
            19,
            args(bc.0, bc.1, bc.2, bc.3, Some(value(20, 0)?)),
            ch,
            &source.controls[1],
            &source.steps[1],
            true,
        ),
        (
            cf,
            22,
            21,
            args(fc.0, fc.1, fc.2, value(22, 0)?, None),
            ph,
            &source.controls[0],
            &source.steps[0],
            false,
        ),
    ] {
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = term(from)?
        else {
            return None;
        };
        if *condition != value(checked, 1)?
            || when_true.arguments != arguments
            || when_false.arguments != arguments
            || from.terminator.as_ref()?.origin != *step
        {
            return None;
        }
        let abort = f.block(when_true.target)?;
        let resume = f.block(when_false.target)?;
        carried(abort, child)?;
        carried(resume, child)?;
        if term(abort)? != &TerminatorKind::Abort
            || abort.terminator.as_ref()?.abort
                != Some(AbortReason::CheckedArithmetic {
                    operator: CheckedArithmeticOperator::Subtract,
                    divisor: value(rhs, 0)?,
                })
            || abort.terminator.as_ref()?.origin != *step
            || !abort.instructions.is_empty()
            || !resume.instructions.is_empty()
            || term(resume)? != &TerminatorKind::Branch(edge(header.id, resume.parameters.clone()))
            || resume.terminator.as_ref()?.origin != *control
        {
            return None;
        }
        all.extend([abort, resume]);
    }
    if all
        .iter()
        .map(|b| b.id)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != 11
    {
        return None;
    }
    for b in &all {
        if !matches!(term(b)?, TerminatorKind::Abort) && b.terminator.as_ref()?.abort.is_some() {
            return None;
        }
        for p in &b.parameters {
            if let EntityId::Loan(_) = p
                && f.entity(*p)?.ty
                    != (EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: int,
                    })
            {
                return None;
            }
        }
    }
    for (b, indices) in [
        (e, (0..3)),
        (ph, (3..5)),
        (pb, (5..11)),
        (pf, (11..17)),
        (ch, (17..19)),
        (cb, (19..21)),
        (cf, (21..23)),
    ] {
        let actual = indices.map(|n| &i[n]).collect::<Vec<_>>();
        if b.instructions != actual.iter().map(|op| op.id).collect::<Vec<_>>()
            || actual.iter().any(|op| op.block != b.id)
        {
            return None;
        }
    }
    for (b, origin) in [
        (e, &source.controls[0]),
        (ph, &source.conditions[0]),
        (pb, &source.controls[1]),
        (ch, &source.conditions[1]),
        (pf, &source.callable),
    ] {
        if b.terminator.as_ref()?.origin != *origin {
            return None;
        }
    }
    Some(())
}
