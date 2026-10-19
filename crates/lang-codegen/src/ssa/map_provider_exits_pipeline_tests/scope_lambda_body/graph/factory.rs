//! The internal make() result returns the same fully populated factory owner.
use super::super::super::temporary_arguments;
use super::*;
pub(super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    i: &ast::Input,
    module: &Module,
    map: SsaTypeId,
) -> Result<(), Failure> {
    let make = function(module, i.make.span)?;
    let factory = function(module, i.produce.span)?;
    let argument_ast = temporary_arguments::ast::function(sources, parsed, "argument", None)?;
    let evaluator = function(module, argument_ast.span)?;
    let g = Graph::new(make)?;
    let call_expr = |wanted| {
        one(
            parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Expression::Call { callee, .. }
                        if ast::inside(i.make.span, n.span())
                            && sources.slice(ast::span(parsed, *callee)) == Ok(wanted) =>
                    {
                        Some(id)
                    }
                    _ => None,
                }),
            "A2 actual make body source Call",
        )
    };
    let produce_id = call_expr("produce")?;
    let argument_id = call_expr("argument")?;
    let argument = at(&g, ast::span(parsed, argument_id), |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    let call = at(&g, ast::span(parsed, produce_id), |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    let Operation::DirectCall {
        callee,
        receiver: None,
        arguments,
    } = &call.operation
    else {
        return Err(failure("A2 actual selected factory ABI", call));
    };
    let [EntityId::Loan(actual)] = arguments.as_slice() else {
        return Err(failure("A2 actual original Borrow Int slot", call));
    };
    let begin = g.definition(EntityId::Loan(*actual))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("A2 actual argument Shared birth", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.operation == Operation::BorrowEnd { loan: *actual }),
        "A2 actual synchronous factory argument End",
    )?;
    need(
        argument.operation
            == Operation::DirectCall {
                callee: evaluator.id,
                receiver: None,
                arguments: vec![],
            }
            && *callee == factory.id
            && root.operation
                == Operation::RootPlace {
                    owner: value(argument)?,
                }
            && root.origin == argument.origin
            && begin.origin == argument.origin
            && end.origin == call.origin
            && g.blocks.len() == 1
            && g.edges.is_empty()
            && g.position(argument)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(call)?
            && g.position(call)? < g.position(end)?
            && make.return_types.as_slice() == [map]
            && factory.return_types.as_slice() == [map]
            && matches!(&g.term(call.block)?.kind,TerminatorKind::Return{values}if values.as_slice()==[value(call)?]),
        "A2 exact evaluator/Root/Borrow/factory/End/owned result Return",
        (argument, root, begin, call, end),
    )?;
    let mut uses = Uses::default();
    for e in [
        result(argument)?,
        EntityId::Place(place),
        EntityId::Loan(*actual),
        result(call)?,
    ] {
        uses.protect(e);
    }
    uses.permit(result(argument)?, root);
    uses.permit(EntityId::Place(place), begin);
    uses.permit(EntityId::Loan(*actual), call);
    uses.permit(EntityId::Loan(*actual), end);
    uses.check(&g, Some((call.block, value(call)?)))?;
    let case = temporary_arguments::sources().remove(0);
    temporary_arguments::graph::factory::check(&case, sources, parsed, module, factory, map)
}
