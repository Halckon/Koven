//! Exact P17 checked assignment and P18 clone owner, with all component operand uses.
use super::*;
use lang_frontend::parser::Expression;
fn end<'a>(
    g: &Graph<'a>,
    family: &BTreeSet<EntityId>,
    at: Span,
    after: &Instruction,
    uses: &mut Uses,
) -> Result<&'a Instruction, Failure> {
    let op=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if family.contains(&EntityId::Loan(loan)))),"scope unique synchronous actual loan End")?;
    need(
        op.origin == Origin::Source(at),
        "scope exact original synchronous loan endpoint",
        op,
    )?;
    before(g, after, op)?;
    for e in op.operation.entities() {
        uses.permit(e, op);
    }
    Ok(op)
}
/// Only actual SharedReborrow operations and actual block columns may link an
/// argument to the previously proved component. Every created child has its End.
fn component(
    g: &Graph<'_>,
    argument: EntityId,
    roots: &BTreeSet<EntityId>,
    spans: (Span, Span),
    call: &Instruction,
    uses: &mut Uses,
) -> Result<(), Failure> {
    if roots.contains(&argument) {
        uses.permit(argument, call);
        return Ok(());
    }
    let begin = g.definition(argument)?;
    let Operation::SharedReborrow { source } = begin.operation else {
        return Err(failure(
            "scope component argument actual Shared lineage",
            begin,
        ));
    };
    need(
        begin.origin == Origin::Source(spans.0),
        "scope actual selected component argument birth",
        begin,
    )?;
    component(g, EntityId::Loan(source), roots, spans, begin, uses)?;
    let aliases = family(g, &[argument])?;
    protect(uses, &aliases);
    uses.permit(argument, call);
    before(g, begin, call)?;
    end(g, &aliases, spans.1, call, uses)?;
    Ok(())
}
fn owner_argument<'a>(
    g: &Graph<'a>,
    argument: EntityId,
    owner: ValueId,
    at: Span,
    call: &Instruction,
    uses: &mut Uses,
) -> Result<&'a Instruction, Failure> {
    let begin = g.definition(argument)?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("scope exact owner argument Shared birth", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    need(
        root.operation == Operation::RootPlace { owner }
            && root.origin == Origin::Source(at)
            && begin.origin == root.origin,
        "scope exact original owner/argument RootPlace identity",
        (root, begin),
    )?;
    before(g, root, begin)?;
    before(g, begin, call)?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.protect(argument);
    uses.permit(argument, call);
    Ok(begin)
}
fn literal(
    g: &Graph<'_>,
    argument: EntityId,
    span: Span,
    bytes: &[u8],
    call: &Instruction,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let begin = g.definition(argument)?;
    let Operation::BorrowBegin { place, .. } = begin.operation else {
        return Err(failure("scope literal actual Shared argument", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("scope literal actual root", root));
    };
    let producer = g.definition(EntityId::Value(owner))?;
    need(
        matches!(&producer.operation,Operation::StringLiteral{bytes:actual,..}if actual==bytes)
            && producer.origin == Origin::Source(span),
        "scope actual control literal bytes/origin",
        producer,
    )?;
    owner_argument(g, argument, owner, span, call, uses)?;
    let stop = end(
        g,
        &BTreeSet::from([argument]),
        call.origin.span(),
        call,
        uses,
    )?;
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.operation == Operation::Drop { owner }),
        "scope literal unique original owner Drop",
    )?;
    need(
        drop.origin == Origin::Source(span),
        "scope literal actual cleanup origin",
        drop,
    )?;
    before(g, stop, drop)?;
    uses.permit(EntityId::Value(owner), drop);
    Ok(())
}
pub(super) fn check(
    c: &Case,
    _sources: &SourceMap,
    parsed: &ParsedFile,
    i: &ast::Input,
    module: &Module,
    g: &Graph<'_>,
    uses: &mut Uses,
) -> Result<(), Failure> {
    if !matches!(c.gate, 17 | 18) {
        return Ok(());
    }
    let read = i.read.as_ref().unwrap();
    let invoke = at(g, ast::span(parsed, read.id), |o| {
        matches!(o, Operation::DirectCall { .. })
    })?;
    let Operation::DirectCall {
        callee,
        receiver: None,
        arguments,
    } = &invoke.operation
    else {
        return Err(failure("scope exact actual read ABI", invoke));
    };
    need(
        *callee == function(module, i.reader.as_ref().unwrap().span)?.id
            && arguments.len() == 2
            && invoke.results.is_empty(),
        "scope body exact original read target and K/V slot order",
        invoke,
    )?;
    let mut components = Vec::new();
    let mut roots = Vec::new();
    if let Some(site) = &i.site {
        for predicate in [0, 1] {
            let projection = at(g, site.span, |op| {
                if predicate == 0 {
                    matches!(op, Operation::MapIterationKeyLoan { .. })
                } else {
                    matches!(op, Operation::MapIterationValueLoan { .. })
                }
            })?;
            let aliases = family(g, &[result(projection)?])?;
            protect(uses, &aliases);
            roots.push(projection);
            components.push(aliases);
        }
        for n in 0..2 {
            component(
                g,
                arguments[n],
                &components[n],
                (
                    ast::span(parsed, read.arguments[n]),
                    ast::span(parsed, read.id),
                ),
                invoke,
                uses,
            )?;
        }
    } else {
        for (n, bytes) in [b"key".as_slice(), b"value".as_slice()]
            .into_iter()
            .enumerate()
        {
            literal(
                g,
                arguments[n],
                ast::span(parsed, read.arguments[n]),
                bytes,
                invoke,
                uses,
            )?;
        }
    }
    if let Some(id) = i.assignment {
        checked(parsed, i, module, g, uses, id, invoke)?;
    }
    if let Some(id) = i.clone {
        let cloned = at(g, ast::span(parsed, id), |o| {
            matches!(o, Operation::StringClone { .. })
        })?;
        let Operation::StringClone { source } = cloned.operation else {
            unreachable!()
        };
        let Expression::Call { callee, .. } = parsed.ast().expressions().get(id).unwrap().payload()
        else {
            unreachable!()
        };
        let Expression::Member { receiver, .. } =
            parsed.ast().expressions().get(*callee).unwrap().payload()
        else {
            unreachable!()
        };
        if c.provider {
            component(
                g,
                EntityId::Loan(source),
                &components[0],
                (ast::span(parsed, *receiver), ast::span(parsed, id)),
                cloned,
                uses,
            )?;
            before(g, roots[0], cloned)?;
        } else {
            literal(
                g,
                EntityId::Loan(source),
                ast::span(parsed, *receiver),
                b"key",
                cloned,
                uses,
            )?;
        }
        let owner = value(cloned)?;
        let print = i.clone_print.as_ref().unwrap();
        let actual = at(g, ast::span(parsed, print.id), |o| {
            matches!(o, Operation::PrintString { .. })
        })?;
        let Operation::PrintString { value: borrow } = actual.operation else {
            unreachable!()
        };
        owner_argument(
            g,
            EntityId::Loan(borrow),
            owner,
            ast::span(parsed, print.arguments[0]),
            actual,
            uses,
        )?;
        let stop = end(
            g,
            &BTreeSet::from([EntityId::Loan(borrow)]),
            ast::span(parsed, print.id),
            actual,
            uses,
        )?;
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::Drop { owner }),
            "scope unique fresh cloned owner cleanup",
        )?;
        need(
            drop.origin == Origin::Source(i.cloned.unwrap()),
            "scope actual named clone Drop identity",
            drop,
        )?;
        before(g, cloned, actual)?;
        before(g, stop, drop)?;
        before(g, drop, invoke)?;
        uses.permit(EntityId::Value(owner), drop);
    }
    if let Some(site) = &i.site {
        let vend = end(g, &components[1], site.span, invoke, uses)?;
        let kend = end(g, &components[0], site.span, invoke, uses)?;
        let next = at(g, site.span, |o| {
            matches!(o, Operation::MapIterationNext { .. })
        })?;
        before(g, vend, kend)?;
        before(g, kend, next)?;
        need(
            !g.edges.iter().any(|(_, edge)| {
                edge.target == g.branch(next.block).unwrap().target
                    && edge
                        .arguments
                        .iter()
                        .any(|e| components.iter().any(|family| family.contains(e)))
            }),
            "scope no live component on actual provider backedge",
            next,
        )?;
    }
    Ok(())
}
fn checked(
    parsed: &ParsedFile,
    i: &ast::Input,
    module: &Module,
    g: &Graph<'_>,
    uses: &mut Uses,
    id: lang_frontend::ast::ExpressionId,
    read: &Instruction,
) -> Result<(), Failure> {
    let Expression::Assignment { value: rhs, .. } =
        parsed.ast().expressions().get(id).unwrap().payload()
    else {
        unreachable!()
    };
    let Expression::Binary {
        left: lhs,
        right: rhsarg,
        ..
    } = parsed.ast().expressions().get(*rhs).unwrap().payload()
    else {
        unreachable!()
    };
    let add = at(g, ast::span(parsed, *rhs), |o| {
        matches!(o, Operation::CheckedArithmetic { .. })
    })?;
    let Operation::CheckedArithmetic {
        operator: CheckedArithmeticOperator::Add,
        left,
        right,
    } = add.operation
    else {
        return Err(failure("scope actual assigned checked Add", add));
    };
    let zero = g.definition(EntityId::Value(left))?;
    let one_value = g.definition(EntityId::Value(right))?;
    need(
        zero.operation == Operation::Constant(ScalarConstant::Integer(0))
            && one_value.operation == Operation::Constant(ScalarConstant::Integer(1))
            && one_value.origin == Origin::Source(ast::span(parsed, *rhsarg))
            && ast::span(parsed, *lhs).start() < ast::span(parsed, *rhsarg).start(),
        "scope assigned n+1 uses actual current zero and source one",
        (zero, one_value),
    )?;
    let [EntityId::Value(sum), EntityId::Value(failed)] = add.results.as_slice() else {
        return Err(failure(
            "scope checked assignment result/failure identities",
            add,
        ));
    };
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(add.block)?.kind
    else {
        return Err(failure(
            "scope checked assignment actual success/Abort CFG",
            add,
        ));
    };
    need(
        *condition == *failed
            && matches!(g.term(when_true.target)?.kind, TerminatorKind::Abort)
            && when_false.target != when_true.target,
        "scope failure aborts and successful assignment enters normal continuation",
        g.term(add.block)?,
    )?;
    g.incoming_from(when_false.target, &[add.block])?;
    g.incoming_from(when_true.target, &[add.block])?;
    before(g, add, read)?;
    let count = i.count_call.as_ref().unwrap();
    let call = at(g, ast::span(parsed, count.id), |o| {
        matches!(o, Operation::DirectCall { .. })
    })?;
    let Operation::DirectCall {
        callee,
        receiver: None,
        arguments,
    } = &call.operation
    else {
        return Err(failure(
            "scope checked assigned scalar actual consumer ABI",
            call,
        ));
    };
    need(
        *callee == function(module, i.count.as_ref().unwrap().span)?.id && arguments.len() == 1,
        "scope checked current scalar exact selected count callable",
        call,
    )?;
    let begin = g.definition(arguments[0])?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("scope count actual current scalar Borrow", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("scope count assigned result actual place", root));
    };
    let sums = family(g, &[EntityId::Value(*sum)])?;
    need(
        sums.contains(&EntityId::Value(owner))
            && root.origin == Origin::Source(ast::span(parsed, count.arguments[0]))
            && begin.origin == root.origin,
        "scope actual assigned Add result reaches latest count argument",
        (root, begin),
    )?;
    for e in [EntityId::Value(left), EntityId::Value(right)] {
        uses.protect(e);
        uses.permit(e, add);
    }
    protect(uses, &sums);
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.protect(arguments[0]);
    uses.permit(arguments[0], call);
    let stop = end(
        g,
        &BTreeSet::from([arguments[0]]),
        ast::span(parsed, count.id),
        call,
        uses,
    )?;
    before(g, read, call)?;
    if let Some(site) = &i.site {
        let next = at(g, site.span, |o| {
            matches!(o, Operation::MapIterationNext { .. })
        })?;
        before(g, stop, next)?;
    }
    Ok(())
}
