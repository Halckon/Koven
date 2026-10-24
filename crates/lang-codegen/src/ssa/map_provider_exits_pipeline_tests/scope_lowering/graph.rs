//! Borrow parameter anchors are followed through every real incoming column.
mod body;
mod callers;
mod provider;
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::{Case, ast};
use crate::ssa::model::*;
use lang_frontend::{
    parser::ParsedFile,
    source::{SourceMap, Span},
};
use std::collections::BTreeSet;
fn at<'a>(
    g: &Graph<'a>,
    span: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> Result<&'a Instruction, Failure> {
    one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.origin == Origin::Source(span) && predicate(&i.operation)),
        "scope exact original SSA operation",
    )
}
fn family(g: &Graph<'_>, anchors: &[EntityId]) -> Result<BTreeSet<EntityId>, Failure> {
    let mut all = anchors.iter().copied().collect::<BTreeSet<_>>();
    loop {
        let n = all.len();
        for (_, edge) in &g.edges {
            for (&a, &p) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if all.contains(&a) {
                    all.insert(p);
                }
            }
        }
        if n == all.len() {
            break;
        }
    }
    for &e in &all {
        g.anchored(e, anchors)?;
    }
    Ok(all)
}
fn protect(uses: &mut Uses, family: &BTreeSet<EntityId>) {
    for &e in family {
        uses.protect(e);
    }
}
/// A cross-block ordering is accepted only when all paths from entry to the later
/// instruction pass through the former instruction's block. Cycles do not imply order.
fn before(g: &Graph<'_>, a: &Instruction, b: &Instruction) -> Result<(), Failure> {
    if a.block == b.block {
        return need(
            g.position(a)? < g.position(b)?,
            "scope actual same-block instruction order",
            (a, b),
        );
    }
    let entry = g.function.entry_block().unwrap();
    let mut pending = vec![entry];
    let mut seen = BTreeSet::new();
    while let Some(block) = pending.pop() {
        if block == a.block || !seen.insert(block) {
            continue;
        }
        need(
            block != b.block,
            "scope all paths to later operation pass earlier block",
            (a, b, block),
        )?;
        for (from, e) in &g.edges {
            if *from == block {
                pending.push(e.target);
            }
        }
    }
    Ok(())
}
pub(super) fn check(
    c: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    i: &ast::Input,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "scope actual original module")?;
    let scan = function(module, i.scan.span)?;
    let g = Graph::with_abort(scan)?;
    callers::check(c, sources, parsed, i, module)?;
    if c.gate == 2 {
        need(
            g.instructions.is_empty()
                && g.blocks.len() == 1
                && g.edges.is_empty()
                && g.block(scan.entry_block().unwrap())?.parameters.is_empty()
                && matches!(&g.term(scan.entry_block().unwrap())?.kind,TerminatorKind::Return{values}if values.is_empty())
                && g.term(scan.entry_block().unwrap())?.origin
                    == Origin::Source(ast::span(parsed, i.early_return.unwrap())),
            "scope legal unacquired dead tail has no SSA construction/provider/cleanup",
            scan,
        )?;
        crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(
            &g,
            &[],
            &BTreeSet::new(),
        )?;
        return Ok(());
    }
    let entry = g.block(scan.entry_block().unwrap())?;
    need(
        entry.parameters.len() == 2 && scan.borrow_return.is_none() && scan.return_types.is_empty(),
        "scope actual original scanner Borrow ABI",
        entry,
    )?;
    let seed = entry.parameters[0];
    let ty = scan.entity(seed).unwrap().ty;
    let map = ty.semantic_type();
    need(
        matches!(
            ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ) && module.map_container(map).is_some_and(|(kind, k, v)| {
            kind == MapContainerKind::Map
                && module.type_kind(k) == Some(&SsaTypeKind::StringOwner)
                && module.type_kind(v) == Some(&SsaTypeKind::StringOwner)
        }),
        "scope original Borrow Map String/String capability",
        (seed, ty),
    )?;
    let originals = family(&g, &[seed])?;
    let mut uses = Uses::default();
    let mut expected_map = originals.clone();
    protect(&mut uses, &originals);
    let leased = if let Some(site) = &i.site {
        provider::check(c, parsed, i, module, &g, (&originals, site), &mut uses)?
    } else {
        need(
            !g.instructions.iter().any(|op| {
                matches!(
                    op.operation,
                    Operation::MapIterationFirst { .. }
                        | Operation::MapIterationHasNext { .. }
                        | Operation::MapIterationKeyLoan { .. }
                        | Operation::MapIterationValueLoan { .. }
                        | Operation::MapIterationNext { .. }
                )
            }),
            "scope original no-For has no provider operations",
            scan.id,
        )?;
        BTreeSet::new()
    };
    expected_map.extend(leased.iter().copied());
    if let Some(call) = &i.observe_call {
        let op = at(&g, ast::span(parsed, call.id), |o| {
            matches!(o, Operation::DirectCall { .. })
        })?;
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments,
        } = &op.operation
        else {
            return Err(failure("scope actual control observe ABI", op));
        };
        need(
            *callee == function(module, i.observe.as_ref().unwrap().span)?.id
                && arguments.len() == 1,
            "scope control selected original observe",
            op,
        )?;
        let argument = arguments[0];
        if originals.contains(&argument) {
            // This ABI directly borrows an already active input loan. Full actual
            // incoming columns are anchored to entry; the callee cannot end it.
            g.anchored(argument, &[seed])?;
            uses.permit(argument, op);
            need(!g.instructions.iter().any(|instruction|matches!(instruction.operation,Operation::BorrowEnd{loan}if originals.contains(&EntityId::Loan(loan)))),"scope direct Borrow call preserves caller's active source lease",op)?;
        } else {
            let begin = g.definition(argument)?;
            let Operation::SharedReborrow { source } = begin.operation else {
                return Err(failure(
                    "scope control actual independent argument reborrow",
                    begin,
                ));
            };
            need(
                originals.contains(&EntityId::Loan(source))
                    && begin.origin == Origin::Source(ast::span(parsed, call.arguments[0])),
                "scope control reborrow rooted at actual current input",
                begin,
            )?;
            uses.permit(EntityId::Loan(source), begin);
            let aliases = family(&g, &[argument])?;
            expected_map.extend(aliases.iter().copied());
            protect(&mut uses, &aliases);
            uses.permit(argument, op);
            let end=one(g.instructions.iter().copied().filter(|instruction|matches!(instruction.operation,Operation::BorrowEnd{loan}if aliases.contains(&EntityId::Loan(loan)))),"scope independent control argument unique End")?;
            need(
                end.origin == Origin::Source(ast::span(parsed, call.id)),
                "scope independent control argument CallReturn",
                end,
            )?;
            before(&g, op, end)?;
            for e in end.operation.entities() {
                uses.permit(e, end);
            }
        }
    }

    body::check(c, sources, parsed, i, module, &g, &mut uses)?;
    for op in &g.instructions {
        need(
            !matches!(op.operation,Operation::Drop{owner}if module.map_container(scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some()),
            "scope borrowed scanner cannot Drop caller Map",
            op,
        )?;
    }
    for (index, data) in scan.loans.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            let e = EntityId::Loan(LoanId {
                function: scan.id,
                index,
            });
            need(
                originals.contains(&e)
                    || leased.contains(&e)
                    || i.observe_call.as_ref().is_some_and(|call| {
                        g.definition(e).is_ok_and(|op| {
                            op.origin == Origin::Source(ast::span(parsed, call.arguments[0]))
                        })
                    }),
                "scope every actual Map loan has original input/provider/control provenance",
                (e, data),
            )?;
        }
    }
    crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(
        &g,
        &[map],
        &expected_map,
    )?;
    uses.check(&g, None)
}
