//! Actual receiver allocation, resolved field projection and complete synchronous cleanup.
use super::*;
fn ast_call(
    parsed: &ParsedFile,
    id: lang_frontend::ast::ExpressionId,
) -> Result<(Span, &Expression), Failure> {
    let node = parsed
        .ast()
        .expressions()
        .get(id)
        .map_err(|e| failure("ordinary original Call node", e))?;
    Ok((node.span(), node.payload()))
}

fn allocation<'a>(g: &Graph<'a>, span: Span) -> Result<&'a Instruction, Failure> {
    one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(span)
                && matches!(i.operation, Operation::HeapAllocate { .. })
        }),
        "ordinary actual source receiver allocation",
    )
}

pub(super) fn check(
    g: &Graph<'_>,
    parsed: &ParsedFile,
    call: &Instruction,
    input: &ast::Input,
    proof: &facts::Proof,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let Some(proof) = &proof.receiver else {
        return need(
            matches!(call.operation, Operation::DirectCall { receiver: None, .. }),
            "local argument free factory has no receiver",
            call,
        );
    };
    let original = parsed
        .ast()
        .expressions()
        .get(input.call)
        .unwrap()
        .payload();
    let Expression::Call { callee, .. } = original else {
        return Err(failure("local argument original member Call", original));
    };
    let Expression::Member { receiver, .. } =
        parsed.ast().expressions().get(*callee).unwrap().payload()
    else {
        return Err(failure("ordinary actual member receiver AST", callee));
    };
    let (receiver_span, constructor) = ast_call(parsed, *receiver)?;
    let alloc = allocation(g, receiver_span)?;
    let owner = value(alloc)?;
    let Operation::DirectCall {
        receiver: Some(EntityId::Loan(actual)),
        ..
    } = call.operation
    else {
        return Err(failure("ordinary actual shared receiver operand", call));
    };
    let mut receiver = actual;
    let mut ends = Vec::new();
    uses.protect(EntityId::Loan(actual));
    uses.permit(EntityId::Loan(actual), call);
    if let Some(expected_field) = proof.field {
        let projection = g.definition(EntityId::Loan(actual))?;
        let Operation::SharedHeapFieldLoan { base, field } = projection.operation else {
            return Err(failure(
                "ordinary delegation really projects the heap field",
                projection,
            ));
        };
        need(
            field == expected_field && projection.origin == Origin::Source(proof.begin),
            "ordinary resolved delegation field index",
            projection,
        )?;
        receiver = base;
        uses.protect(EntityId::Loan(base));
        uses.permit(EntityId::Loan(base), projection);
        let Expression::Call { arguments, .. } = constructor else {
            return Err(failure("ordinary Host constructor", constructor));
        };
        let [argument] = arguments.as_slice() else {
            return Err(failure(
                "ordinary one delegate constructor argument",
                arguments,
            ));
        };
        let (delegate_span, _) = ast_call(parsed, argument.value)?;
        let delegate = allocation(g, delegate_span)?;
        let Operation::HeapAllocate { payload, .. } = alloc.operation else {
            unreachable!()
        };
        let payload = g.definition(EntityId::Value(payload))?;
        need(
            matches!(&payload.operation, Operation::AggregateConstruct {fields, ..}
            if fields.as_slice() == [value(delegate)?]),
            "ordinary Host stores exact Factory owner in delegate field",
            payload,
        )?;
        need(
            g.function
                .entity(EntityId::Loan(actual))
                .unwrap()
                .ty
                .semantic_type()
                == g.function
                    .entity(result(delegate)?)
                    .unwrap()
                    .ty
                    .semantic_type(),
            "ordinary projected receiver is actual Factory allocation type",
            projection,
        )?;
        uses.protect(result(delegate)?);
        uses.permit(result(delegate)?, payload);
        ends.push(actual);
    }
    let begin = g.definition(EntityId::Loan(receiver))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure(
            "ordinary outer receiver real shared BorrowBegin",
            begin,
        ));
    };
    let root = g.definition(EntityId::Place(place))?;
    need(
        root.operation == Operation::RootPlace { owner }
            && root.origin == Origin::Source(proof.begin)
            && begin.origin == Origin::Source(proof.begin)
            && root.block == alloc.block
            && begin.block == alloc.block
            && call.block == alloc.block
            && g.position(alloc)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(call)?,
        "ordinary receiver borrows exact source construction",
        (alloc, root, begin, call),
    )?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    ends.push(receiver);
    let mut previous = call;
    for current in ends {
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: current }),
            "ordinary unique actual receiver End",
        )?;
        need(
            end.origin == Origin::Source(proof.end)
                && end.block == call.block
                && g.position(previous)? < g.position(end)?,
            "ordinary call then delegate End then outer End",
            (previous, end),
        )?;
        uses.permit(EntityId::Loan(current), end);
        previous = end;
    }
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner }),
        "ordinary unique temporary receiver Drop",
    )?;
    need(
        drop.block == call.block && g.position(previous)? < g.position(drop)?,
        "ordinary receiver Drop follows all receiver Ends",
        drop,
    )?;
    uses.permit(EntityId::Value(owner), drop);
    Ok(())
}
