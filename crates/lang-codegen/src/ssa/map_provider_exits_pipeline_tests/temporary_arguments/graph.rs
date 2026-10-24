//! Selected argument evaluation, ordinary Borrow ABI and the actual returned owner.
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod factory;
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod provider;
use super::{Case, ast};
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, function, loan, result, value},
        query_inputs::one,
    },
    model::*,
};
use lang_frontend::{
    parser::ParsedFile,
    source::{SourceMap, Span},
};
use std::collections::BTreeSet;

fn at<'ir>(
    g: &Graph<'ir>,
    origin: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> Result<&'ir Instruction, Failure> {
    one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.origin == Origin::Source(origin) && predicate(&i.operation)),
        "argument actual source-origin SSA operation",
    )
}

fn anchored(g: &Graph<'_>, current: EntityId, anchors: &[EntityId]) -> Result<(), Failure> {
    let mut pending = vec![current];
    let mut seen = BTreeSet::new();
    let mut dependencies = Vec::new();
    while let Some(entity) = pending.pop() {
        if anchors.contains(&entity) || !seen.insert(entity) {
            continue;
        }
        let data = g
            .function
            .entity(entity)
            .ok_or_else(|| failure("argument transported actual entity", entity))?;
        let Definition::BlockParameter { block, .. } = data.definition else {
            return Err(failure("argument identity reaches wrong producer", data));
        };
        let incoming = g.incoming(block);
        need(
            !incoming.is_empty(),
            "argument complete incoming exists",
            entity,
        )?;
        for (_, edge) in incoming {
            let argument = g.argument(edge, entity)?;
            dependencies.push((entity, argument));
            pending.push(argument);
        }
    }
    let mut reached = anchors.iter().copied().collect::<BTreeSet<_>>();
    loop {
        let before = reached.len();
        for &(entity, argument) in &dependencies {
            if reached.contains(&argument) {
                reached.insert(entity);
            }
        }
        if before == reached.len() {
            break;
        }
    }
    need(
        seen.is_subset(&reached),
        "argument every incoming node is anchored",
        (current, seen, reached),
    )
}

fn marker<'ir>(g: &Graph<'ir>, span: Span) -> Result<&'ir Instruction, Failure> {
    at(g, span, |operation| match operation {
        Operation::PrintLiteral { bytes } => bytes == b"after\n",
        Operation::PrintString { value } => {
            let Ok(begin) = g.definition(EntityId::Loan(*value)) else {
                return false;
            };
            let Operation::BorrowBegin { place, .. } = begin.operation else {
                return false;
            };
            let Ok(root) = g.definition(EntityId::Place(place)) else {
                return false;
            };
            let Operation::RootPlace { owner } = root.operation else {
                return false;
            };
            g.definition(EntityId::Value(owner)).is_ok_and(|literal|
                matches!(&literal.operation, Operation::StringLiteral {bytes, ..} if bytes == b"after"))
        }
        _ => false,
    })
}

pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
    control: bool,
) -> Result<(), Failure> {
    let input = ast::input(sources, parsed, control)?;
    let module = one(program.modules.iter(), "argument actual original module")?;
    let entry = function(module, input.entry.span)?;
    let factory = function(module, input.factory.span)?;
    let argument = function(module, input.argument.span)?;
    let g = Graph::with_abort(entry)?;
    let block = entry.entry_block().unwrap();
    need(
        g.block(block)?.parameters.is_empty(),
        "argument caller entry has no parameters",
        entry.id,
    )?;
    let value_call = one(
        g.instructions.iter().copied().filter(
            |i| matches!(i.operation, Operation::DirectCall {callee, ..} if callee == argument.id),
        ),
        "argument evaluator is actually called once",
    )?;
    need(
        value_call.origin == Origin::Source(ast::span(parsed, input.value))
            && matches!(&value_call.operation, Operation::DirectCall {receiver:None, arguments, ..} if arguments.is_empty()),
        "argument evaluator selected actual source Call",
        value_call,
    )?;
    let actual_value = value(value_call)?;
    let int = entry
        .entity(EntityId::Value(actual_value))
        .unwrap()
        .ty
        .semantic_type();
    need(
        module.type_kind(int)
            == Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
            && argument.return_types.as_slice() == [int]
            && argument
                .block(argument.entry_block().unwrap())
                .unwrap()
                .parameters
                .is_empty(),
        "argument evaluator actual Int return ABI",
        argument.id,
    )?;
    let call = one(
        g.instructions.iter().copied().filter(
            |i| matches!(i.operation, Operation::DirectCall {callee, ..} if callee == factory.id),
        ),
        "argument selected Int factory called once",
    )?;
    need(
        call.origin == Origin::Source(ast::span(parsed, input.produce)),
        "argument actual selected Int overload Callspan",
        call,
    )?;
    let Operation::DirectCall {
        receiver: None,
        ref arguments,
        ..
    } = call.operation
    else {
        return Err(failure("argument selected direct factory ABI", call));
    };
    let [EntityId::Loan(actual_loan)] = arguments.as_slice() else {
        return Err(failure("argument actual SharedBorrow Int operand", call));
    };
    let begin = g.definition(EntityId::Loan(*actual_loan))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("argument actual shared loan birth", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::BorrowEnd { loan: *actual_loan }),
        "argument actual unique synchronous Borrow End",
    )?;
    let parameters = &factory
        .block(factory.entry_block().unwrap())
        .unwrap()
        .parameters;
    let ty = EntityType::Loan {
        kind: LoanKind::Shared,
        target: int,
    };
    need(
        root.operation
            == Operation::RootPlace {
                owner: actual_value,
            }
            && root.origin == Origin::Source(ast::span(parsed, input.value))
            && begin.origin == root.origin
            && end.origin == Origin::Source(ast::span(parsed, input.produce))
            && [value_call, root, begin, call, end]
                .iter()
                .all(|i| i.block == block)
            && g.position(value_call)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(call)?
            && g.position(call)? < g.position(end)?
            && entry.entity(EntityId::Loan(*actual_loan)).unwrap().ty == ty
            && entry.entity(EntityId::Place(place)).unwrap().ty == EntityType::Place(int)
            && matches!(parameters.as_slice(), [parameter] if factory.entity(*parameter).unwrap().ty == ty),
        "argument actual evaluator Value to Root/SharedBorrow/callee ABI/End order",
        (value_call, root, begin, call, end),
    )?;
    let mut uses = Uses::default();
    uses.protect(EntityId::Value(actual_value));
    uses.permit(EntityId::Value(actual_value), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.protect(EntityId::Loan(*actual_loan));
    uses.permit(EntityId::Loan(*actual_loan), call);
    uses.permit(EntityId::Loan(*actual_loan), end);
    let owner = value(call)?;
    let map_type = entry
        .entity(EntityId::Value(owner))
        .unwrap()
        .ty
        .semantic_type();
    let (kind, key, val) = module
        .map_container(map_type)
        .ok_or_else(|| failure("argument actual owning factory result", call))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && [key, val]
            .iter()
            .all(|ty| module.type_kind(*ty) == Some(&SsaTypeKind::StringOwner))
            && factory.return_types.as_slice() == [map_type]
            && g.instructions
                .iter()
                .filter(|i| {
                    matches!(i.operation, Operation::DirectCall { .. })
                        && i.results.iter().any(|entity| {
                            module
                                .map_container(entry.entity(*entity).unwrap().ty.semantic_type())
                                .is_some()
                        })
                })
                .count()
                == 1,
        "argument exact selected map result and no alternate factory result",
        call,
    )?;
    factory::check(case, sources, parsed, module, factory, map_type)?;
    let after = marker(&g, input.after)?;
    if let Some(site) = &input.loop_site {
        provider::check(
            &g,
            module,
            function(module, input.reader.span)?,
            parsed,
            site,
            (owner, call, end, after),
            &mut uses,
        )?;
    } else {
        uses.protect(EntityId::Value(owner));
        let size = at(&g, input.size.unwrap(), |o| {
            matches!(o, Operation::MapSize { .. })
        })?;
        need(
            size.operation
                == Operation::MapSize {
                    owner: EntityId::Value(owner),
                }
                && size.block == block
                && g.position(end)? < g.position(size)?,
            "argument control queries its actual returned owner after argument End",
            size,
        )?;
        uses.permit(EntityId::Value(owner), size);
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::Drop { owner }),
            "argument control actual unique returned-owner Drop",
        )?;
        need(
            drop.block == block && g.position(drop)? == g.position(size)? + 1,
            "argument control Pure owner drops after actual last size",
            drop,
        )?;
        uses.permit(EntityId::Value(owner), drop);
        crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(
            &g,
            &[map_type],
            &BTreeSet::from([EntityId::Value(owner)]),
        )?;
        need(
            !g.instructions.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::MapIterationFirst { .. }
                        | Operation::MapIterationHasNext { .. }
                        | Operation::MapIterationNext { .. }
                        | Operation::MapIterationKeyLoan { .. }
                        | Operation::MapIterationValueLoan { .. }
                )
            }),
            "argument no-For control contains no provider operations",
            entry.id,
        )?;
    }
    need(g.instructions.iter().filter(|i| matches!(i.operation, Operation::Drop {owner}
        if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count() == 1,
        "argument caller drops only its actual returned Map", entry.id)?;
    uses.check(&g, None)
}
