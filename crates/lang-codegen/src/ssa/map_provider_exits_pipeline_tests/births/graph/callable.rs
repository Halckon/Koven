//! Stateless lambda address and its real thunk return supply this CallableInvoke owner.
use super::*;
use crate::ssa::map_provider_exits_pipeline_tests::temporary_arguments;
pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    selected: (&ast::Input, &super::super::facts::Proof),
    module: &Module,
    g: &Graph<'_>,
    uses: &mut Uses,
) -> Result<EntityId, Failure> {
    let (input, proof) = selected;
    let lambda = ast::span(parsed, input.lambda.unwrap());
    let address = at(g, lambda, |o| {
        matches!(o, Operation::FunctionAddress { .. })
    })?;
    let Operation::FunctionAddress { target } = address.operation else {
        unreachable!()
    };
    let thunk = module
        .functions
        .iter()
        .find(|f| f.id == target)
        .ok_or_else(|| failure("birth actual FunctionAddress thunk", address))?;
    need(
        thunk.origin == Origin::Source(lambda)
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::FunctionAddress { .. }))
                .count()
                == 1,
        "birth address selects the actual original noncapturing lambda thunk",
        address,
    )?;
    let invoke = at(g, ast::span(parsed, input.birth), |o| {
        matches!(o, Operation::CallableInvoke { .. })
    })?;
    let Operation::CallableInvoke {
        callable,
        arguments,
    } = &invoke.operation
    else {
        unreachable!()
    };
    need(
        arguments.is_empty()
            && address.block == invoke.block
            && g.position(address)? < g.position(invoke)?,
        "birth actual zeroarg FunctionValue invocation after lambda address",
        invoke,
    )?;
    let mut lineage = BTreeSet::from([result(address)?]);
    uses.protect(result(address)?);
    for op in &g.instructions {
        if let Operation::Copy { source } = op.operation
            && lineage.contains(&EntityId::Value(source))
        {
            need(
                op.origin == Origin::Source(ast::span(parsed, input.callee.unwrap()))
                    && op.block == invoke.block
                    && g.position(op)? < g.position(invoke)?
                    && g.function.entity(result(op)?).unwrap().ty
                        == g.function.entity(result(address)?).unwrap().ty,
                "birth actual callable copy descends from original lambda",
                op,
            )?;
            uses.permit(EntityId::Value(source), op);
            uses.protect(result(op)?);
            lineage.insert(result(op)?);
        }
    }
    need(
        lineage.contains(&EntityId::Value(*callable)),
        "birth CallableInvoke uses the exact original callable value",
        invoke,
    )?;
    uses.permit(EntityId::Value(*callable), invoke);
    let fact = proof
        .callable_drop
        .ok_or_else(|| failure("birth same-analysis callable Drop fact", proof))?;
    let original = value(address)?;
    let drop=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::Drop{owner}if lineage.contains(&EntityId::Value(owner)))),"birth unique actual callable-family Drop")?;
    need(
        drop.operation == Operation::Drop { owner: original }
            && drop.origin == Origin::Source(fact.value_origin())
            && fact.value_origin() == input.factory.unwrap()
            && drop.block == invoke.block
            && g.position(drop)? == g.position(invoke)? + 1,
        "birth exact FE callable Drop consumes original address after copied invocation",
        (fact, address, invoke, drop),
    )?;
    uses.permit(result(address)?, drop);

    // Noncapturing function pointers are Copyable; no hidden owned closure may
    // substitute a capture allocation or consume a different callable family.
    need(
        !g.instructions
            .iter()
            .any(|op| matches!(op.operation, Operation::ClosureConstruct { .. })),
        "birth original empty-capture lambda uses no hidden environment",
        lambda,
    )?;
    thunk_check(
        case,
        sources,
        parsed,
        input,
        module,
        thunk,
        g.function
            .entity(result(invoke)?)
            .unwrap()
            .ty
            .semantic_type(),
    )?;
    result(invoke)
}
fn thunk_check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    input: &ast::Input,
    module: &Module,
    thunk: &Function,
    map: SsaTypeId,
) -> Result<(), Failure> {
    let actual_factory =
        temporary_arguments::ast::function(sources, parsed, "produce", Some("Int"))?;
    let actual_argument = temporary_arguments::ast::function(sources, parsed, "argument", None)?;
    let factory = function(module, actual_factory.span)?;
    let argument = function(module, actual_argument.span)?;
    let g = Graph::new(thunk)?;
    let evaluate = at(
        &g,
        ast::span(parsed, input.argument.unwrap()),
        |o| matches!(o,Operation::DirectCall{callee,receiver:None,arguments}if *callee==argument.id&&arguments.is_empty()),
    )?;
    let call = at(
        &g,
        ast::span(parsed, input.produce.unwrap()),
        |o| matches!(o,Operation::DirectCall{callee,receiver:None,..}if *callee==factory.id),
    )?;
    let Operation::DirectCall { arguments, .. } = &call.operation else {
        unreachable!()
    };
    let [EntityId::Loan(actual)] = arguments.as_slice() else {
        return Err(failure(
            "birth thunk actual sole Borrow Int ABI argument",
            call,
        ));
    };
    let begin = g.definition(EntityId::Loan(*actual))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("birth thunk actual Shared argument", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::BorrowEnd { loan: *actual }),
        "birth thunk unique synchronous argument End",
    )?;
    let int = thunk.entity(result(evaluate)?).unwrap().ty.semantic_type();
    need(
        root.operation
            == Operation::RootPlace {
                owner: value(evaluate)?,
            }
            && [evaluate, root, begin, call, end]
                .iter()
                .all(|i| i.block == call.block)
            && evaluate.origin == root.origin
            && root.origin == begin.origin
            && end.origin == call.origin
            && g.position(evaluate)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(call)?
            && g.position(call)? < g.position(end)?
            && module.type_kind(int)
                == Some(&SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                })
            && argument.return_types.as_slice() == [int]
            && factory.return_types.as_slice() == [map]
            && thunk.return_types.as_slice() == [map]
            && factory
                .block(factory.entry_block().unwrap())
                .unwrap()
                .parameters
                .len()
                == 1
            && factory
                .entity(
                    factory
                        .block(factory.entry_block().unwrap())
                        .unwrap()
                        .parameters[0],
                )
                .unwrap()
                .ty
                == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: int,
                },
        "birth thunk actual evaluator/Root/Shared/selected factory/CallReturn End",
        (evaluate, root, begin, call, end),
    )?;
    need(
        g.blocks.len() == 1
            && g.edges.is_empty()
            && g.block(thunk.entry_block().unwrap())?.parameters.is_empty()
            && matches!(&g.term(call.block)?.kind,TerminatorKind::Return{values}if values.as_slice()==[value(call)?])
            && g.term(call.block)?.origin
                == Origin::Source(ast::span(parsed, input.lambda.unwrap())),
        "birth actual lambda normal Return delivers same selected factory owner",
        thunk.id,
    )?;
    let mut uses = Uses::default();
    for e in [
        result(evaluate)?,
        EntityId::Place(place),
        EntityId::Loan(*actual),
        result(call)?,
    ] {
        uses.protect(e);
    }
    uses.permit(result(evaluate)?, root);
    uses.permit(EntityId::Place(place), begin);
    uses.permit(EntityId::Loan(*actual), call);
    uses.permit(EntityId::Loan(*actual), end);
    uses.check(&g, Some((call.block, value(call)?)))?;
    let original = temporary_arguments::sources()
        .into_iter()
        .find(|c| c.mutable == case.mutable)
        .unwrap();
    temporary_arguments::graph::factory::check(&original, sources, parsed, module, factory, map)
}
