//! Actual factory birth, current hidden owner/lease/cursor and complete edge/use closure.
use super::{
    super::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, function, loan, result, value},
        query_inputs::one,
    },
    ast::{Exit, Input},
    query_body::Body,
    query_callers,
    query_inputs::{Inputs, SiteInput},
};
use crate::ssa::model::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Roles {
    pub owner: ValueId,
    pub source: LoanId,
    pub cursor: ValueId,
}
impl Roles {
    pub fn entities(self) -> [EntityId; 3] {
        [
            EntityId::Value(self.owner),
            EntityId::Loan(self.source),
            EntityId::Value(self.cursor),
        ]
    }
    fn protect(self, uses: &mut Uses, protected: &mut BTreeSet<EntityId>) {
        for e in self.entities() {
            uses.protect(e);
            protected.insert(e);
        }
    }
}
pub(super) fn ty(g: &Graph<'_>, entity: EntityId) -> Result<EntityType, Failure> {
    Ok(g.function
        .entity(entity)
        .ok_or_else(|| failure("temporary exits SSA actual entity", entity))?
        .ty)
}
fn set<T: Copy + Ord>(v: &[T]) -> BTreeSet<T> {
    v.iter().copied().collect()
}
fn transport(
    g: &Graph<'_>,
    edge: &Edge,
    roles: Roles,
    uses: &mut Uses,
    protected: &mut BTreeSet<EntityId>,
) -> Result<Roles, Failure> {
    let all = roles.entities();
    need(
        set(&all).len() == 3 && edge.arguments.len() == 3 && set(&edge.arguments) == set(&all),
        "temporary exits SSA full exact current-role edge",
        (edge, roles),
    )?;
    let EntityId::Value(owner) = g.transported(edge, EntityId::Value(roles.owner))? else {
        return Err(failure("temporary exits SSA owner column", edge));
    };
    let EntityId::Loan(source) = g.transported(edge, EntityId::Loan(roles.source))? else {
        return Err(failure("temporary exits SSA lease column", edge));
    };
    let EntityId::Value(cursor) = g.transported(edge, EntityId::Value(roles.cursor))? else {
        return Err(failure("temporary exits SSA cursor column", edge));
    };
    let next = Roles {
        owner,
        source,
        cursor,
    };
    next.protect(uses, protected);
    Ok(next)
}
fn provider(op: &Operation) -> bool {
    matches!(
        op,
        Operation::MapIterationFirst { .. }
            | Operation::MapIterationHasNext { .. }
            | Operation::MapIterationKeyLoan { .. }
            | Operation::MapIterationValueLoan { .. }
            | Operation::MapIterationNext { .. }
    )
}

#[derive(Debug)]
struct Shape {
    blocks: Vec<BlockId>,
    edges: Vec<(BlockId, BlockId)>,
    pre: BlockId,
    after: BlockId,
    call: InstructionId,
    factory: FunctionId,
    consumer: InstructionId,
    guard: ValueId,
    protected: BTreeSet<EntityId>,
    providers: Vec<InstructionId>,
    condition: ValueId,
    returned: Option<(BlockId, ValueId)>,
}
struct Context<'a, 'ir> {
    graph: &'a Graph<'ir>,
    module: &'ir Module,
    f: &'a Input,
    unit: bool,
}
impl Context<'_, '_> {
    fn site(&self, site: &SiteInput, uses: &mut Uses) -> Result<Shape, Failure> {
        let g = self.graph;
        need(
            g.function.origin == Origin::Source(site.function),
            "temporary exits SSA site actual enclosing function",
            g.function,
        )?;
        need(
            super::super::fixture::contains(site.span, site.body)
                && [site.guard.construction, site.read_call, site.transfer]
                    .iter()
                    .all(|&span| super::super::fixture::contains(site.body, span)),
            "temporary exits SSA bridged body contains actual construction/consumer/transfer",
            site,
        )?;
        let factory = function(self.module, site.factory)?;
        let call = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation,Operation::DirectCall{callee,..} if callee==factory.id),
            ),
            "temporary exits SSA actual unique factory call",
        )?;
        need(
            call.origin == Origin::Source(site.source)
                && matches!(&call.operation,Operation::DirectCall{receiver:None,arguments,..} if arguments.is_empty()),
            "temporary exits SSA actual source Call birth zero arguments",
            call,
        )?;
        let owner = value(call)?;
        let EntityType::Value(container) = ty(g, EntityId::Value(owner))? else {
            return Err(failure("temporary exits SSA owning source result", call));
        };
        let kind = if self.f.map {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        };
        need(
            matches!(self.module.type_kind(container),Some(SsaTypeKind::MapContainer{kind:actual,key,value}) if *actual==kind&&self.module.type_kind(*key)==Some(&SsaTypeKind::StringOwner)&&self.module.type_kind(*value)==Some(&SsaTypeKind::StringOwner))
                && factory.receiver.is_none()
                && factory.borrow_return.is_none()
                && factory.carrier_return.is_none()
                && factory.return_types.as_slice() == [container]
                && factory
                    .entry_block()
                    .and_then(|b| factory.block(b))
                    .is_some_and(|b| b.parameters.is_empty()),
            "temporary exits SSA real factory signature Owned String map",
            factory,
        )?;
        let root = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::RootPlace { owner }),
            "temporary exits SSA real factory owner RootPlace",
        )?;
        let EntityId::Place(place) = result(root)? else {
            return Err(failure("temporary exits SSA source place", root));
        };
        let borrow = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    }
            }),
            "temporary exits SSA source shared lease",
        )?;
        let source = loan(borrow)?;
        let first = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::MapIterationFirst { source }),
            "temporary exits SSA actual First",
        )?;
        let cursor = value(first)?;
        let pre = call.block;
        let ops = g.ops(pre)?;
        need(
            [root, borrow, first]
                .iter()
                .all(|i| i.block == pre && i.origin == Origin::Source(site.span))
                && ty(g, EntityId::Place(place))? == EntityType::Place(container)
                && ty(g, EntityId::Loan(source))?
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: container,
                    }
                && matches!(ty(g,EntityId::Value(cursor))?,EntityType::Value(t) if self.module.type_kind(t)==Some(&SsaTypeKind::Integer{bits:64,signed:false}))
                && ops[g.position(call)?..]
                    .iter()
                    .map(|i| i.id)
                    .collect::<Vec<_>>()
                    == [call.id, root.id, borrow.id, first.id],
            "temporary exits SSA actual Call/Root/Borrow/First preheader suffix",
            &ops,
        )?;
        let mut protected = BTreeSet::new();
        let initial = Roles {
            owner,
            source,
            cursor,
        };
        initial.protect(uses, &mut protected);
        protected.insert(EntityId::Place(place));
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Value(owner), root);
        uses.permit(EntityId::Place(place), borrow);
        uses.permit(EntityId::Loan(source), first);
        let entry = g.branch(pre)?;
        let header = entry.target;
        need(
            g.term(pre)?.origin == Origin::Source(site.span),
            "temporary exits SSA actual source preheader branch",
            g.term(pre)?,
        )?;
        let current = transport(g, entry, initial, uses, &mut protected)?;
        let has = one(
            g.ops(header)?.into_iter(),
            "temporary exits SSA actual one-op HasNext header",
        )?;
        need(
            has.operation
                == Operation::MapIterationHasNext {
                    source: current.source,
                    cursor: current.cursor,
                }
                && has.origin == Origin::Source(site.span)
                && matches!(ty(g,result(has)?)?,EntityType::Value(t) if self.module.type_kind(t)==Some(&SsaTypeKind::Boolean)),
            "temporary exits SSA current source/cursor condition",
            has,
        )?;
        uses.permit(EntityId::Loan(current.source), has);
        uses.permit(EntityId::Value(current.cursor), has);
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &g.term(header)?.kind
        else {
            return Err(failure(
                "temporary exits SSA actual HasNext branch",
                g.term(header)?,
            ));
        };
        need(
            *condition == value(has)? && g.term(header)?.origin == Origin::Source(site.span),
            "temporary exits SSA HasNext result is actual condition",
            g.term(header)?,
        )?;
        condition_uses(g, header, *condition)?;
        protected.insert(EntityId::Value(*condition));
        let (body, exhausted) = (when_true.target, when_false.target);
        need(
            set(&[pre, header, body, exhausted]).len() == 4,
            "temporary exits SSA four distinct core blocks",
            (pre, header, body, exhausted),
        )?;
        let body_roles = transport(g, when_true, current, uses, &mut protected)?;
        let false_roles = transport(g, when_false, current, uses, &mut protected)?;
        for block in [header, body, exhausted] {
            need(
                g.block(block)?.origin == Origin::Source(site.span)
                    && g.block(block)?.parameters.iter().all(|&p| {
                        g.function
                            .entity(p)
                            .is_some_and(|e| e.origin == Origin::Source(site.span))
                    }),
                "temporary exits SSA role block/parameter origins",
                g.block(block)?,
            )?;
        }
        g.incoming_from(body, &[header])?;
        g.incoming_from(exhausted, &[header])?;
        let end = Body {
            graph: g,
            module: self.module,
            site,
            block: body,
            roles: body_roles,
            exit: self.f.exit,
            unit: self.unit,
        }
        .check(uses, &mut protected)?;
        let mut blocks = vec![pre, header, body, exhausted];
        let mut edges = vec![(pre, header), (header, body), (header, exhausted)];
        let after = match self.f.exit {
            Exit::Continue => {
                g.incoming_from(header, &[pre, body])?;
                let back = g.branch(body)?;
                let next = end
                    .next
                    .ok_or_else(|| failure("temporary exits SSA actual Continue Next", &end))?;
                need(
                    back.target == header && g.term(body)?.origin == Origin::Source(site.transfer),
                    "temporary exits SSA Continue actual header backedge",
                    g.term(body)?,
                )?;
                let returned = transport(
                    g,
                    back,
                    Roles {
                        cursor: next,
                        ..body_roles
                    },
                    uses,
                    &mut protected,
                )?;
                need(
                    returned == current
                        && g.argument(back, EntityId::Value(current.owner))?
                            == EntityId::Value(body_roles.owner)
                        && g.argument(back, EntityId::Loan(current.source))?
                            == EntityId::Loan(body_roles.source)
                        && g.argument(back, EntityId::Value(current.cursor))?
                            == EntityId::Value(next),
                    "temporary exits SSA every header incoming preserves current owner/lease and actual Next",
                    back,
                )?;
                edges.push((body, header));
                exhausted
            }
            Exit::Break => {
                g.incoming_from(header, &[pre])?;
                let a = g.branch(body)?;
                let b = g.branch(exhausted)?;
                need(
                    a.target == b.target
                        && a.arguments.is_empty()
                        && b.arguments.is_empty()
                        && !blocks.contains(&a.target)
                        && g.term(body)?.origin == Origin::Source(site.span)
                        && g.term(exhausted)?.origin == Origin::Source(site.span)
                        && g.block(a.target)?.parameters.is_empty()
                        && g.block(a.target)?.origin == Origin::Source(site.span),
                    "temporary exits SSA independent terminal paths meet after complete cleanup without live roles",
                    (a, b),
                )?;
                g.incoming_from(a.target, &[body, exhausted])?;
                blocks.push(a.target);
                edges.extend([(body, a.target), (exhausted, b.target)]);
                a.target
            }
            Exit::Return => {
                g.incoming_from(header, &[pre])?;
                exhausted
            }
        };
        let exit_ops = g.ops(exhausted)?;
        let end_source = *exit_ops
            .first()
            .ok_or_else(|| failure("temporary exits SSA false source end", exhausted))?;
        let drop = *exit_ops
            .get(1)
            .ok_or_else(|| failure("temporary exits SSA false owner Drop", exhausted))?;
        need(
            end_source.operation
                == Operation::BorrowEnd {
                    loan: false_roles.source,
                }
                && end_source.origin == Origin::Source(site.span)
                && end_source.results.is_empty()
                && drop.operation
                    == Operation::Drop {
                        owner: false_roles.owner,
                    }
                && drop.origin == Origin::Source(site.drop)
                && drop.results.is_empty(),
            "temporary exits SSA false exact current source end then owner Drop",
            (end_source, drop),
        )?;
        uses.permit(EntityId::Loan(false_roles.source), end_source);
        uses.permit(EntityId::Value(false_roles.owner), drop);
        need(
            !exit_ops.iter().any(|i| {
                (i.origin == Origin::Source(site.span) && provider(&i.operation))
                    || i.origin == Origin::Source(site.read_call)
                    || i.origin == Origin::Source(site.guard.construction)
                    || i.origin == Origin::Source(site.guard.marker)
            }),
            "temporary exits SSA false has no this-site payload, consumer, Guard or Next",
            &exit_ops,
        )?;
        if self.f.exit == Exit::Break {
            need(
                exit_ops.len() == 2,
                "temporary exits SSA Break false cleanup exact two emitted operations",
                &exit_ops,
            )?;
        }
        if self.f.exit == Exit::Return {
            query_callers::zero_return(g, self.f, exhausted, site.function)?;
        }
        let mut providers = vec![first.id, has.id];
        providers.extend(end.providers);
        need(
            providers.len() == if self.f.exit == Exit::Continue { 5 } else { 4 }
                && g.instructions
                    .iter()
                    .filter(|i| provider(&i.operation) && i.origin == Origin::Source(site.span))
                    .all(|i| providers.contains(&i.id)),
            "temporary exits SSA complete actual site provider set",
            &providers,
        )?;
        Ok(Shape {
            blocks,
            edges,
            pre,
            after,
            call: call.id,
            factory: factory.id,
            consumer: end.call,
            guard: end.guard,
            protected,
            providers,
            condition: *condition,
            returned: end.returned.map(|v| (body, v)),
        })
    }
}

pub(super) fn condition_uses(
    g: &Graph<'_>,
    block: BlockId,
    condition: ValueId,
) -> Result<(), Failure> {
    let entity = EntityId::Value(condition);
    need(
        g.instructions
            .iter()
            .all(|i| !i.operation.entities().contains(&entity)),
        "temporary exits SSA condition has no instruction use",
        condition,
    )?;
    for &b in &g.blocks {
        let term = g.term(b)?;
        if term.kind.entities().contains(&entity) {
            need(
                b == block
                    && matches!(&term.kind,TerminatorKind::Conditional{condition:actual,when_true,when_false} if *actual==condition&&!when_true.arguments.contains(&entity)&&!when_false.arguments.contains(&entity)),
                "temporary exits SSA condition only its actual branch",
                term,
            )?;
        }
    }
    Ok(())
}
pub(super) fn full_graph(g: &Graph<'_>) -> Result<(), Failure> {
    let entities = g
        .function
        .blocks
        .iter()
        .flat_map(|b| b.parameters.iter().copied())
        .chain(
            g.instructions
                .iter()
                .flat_map(|i| i.results.iter().copied()),
        )
        .collect::<Vec<_>>();
    need(
        g.blocks.len() == g.function.blocks.len()
            && g.instructions.len() == g.function.instructions.len()
            && entities.len() == entities.iter().copied().collect::<BTreeSet<_>>().len()
            && entities.len()
                == g.function.values.len() + g.function.places.len() + g.function.loans.len()
            && g.instructions.iter().all(|i| {
                i.results.iter().all(|&entity| {
                    g.function
                        .entity(entity)
                        .is_some_and(|data| data.origin == i.origin)
                })
            }),
        "temporary exits SSA complete reachable block/instruction/entity arenas and result origins",
        g.function,
    )
}

fn finish(g: &Graph<'_>, shapes: &[&Shape], uses: &Uses) -> Result<(), Failure> {
    full_graph(g)?;
    let blocks = shapes
        .iter()
        .flat_map(|s| s.blocks.iter().copied())
        .collect::<Vec<_>>();
    let edges = shapes
        .iter()
        .flat_map(|s| s.edges.iter().copied())
        .collect::<Vec<_>>();
    let actual = g
        .edges
        .iter()
        .map(|(from, e)| (*from, e.target))
        .collect::<Vec<_>>();
    let providers = shapes
        .iter()
        .flat_map(|s| s.providers.iter().copied())
        .collect::<Vec<_>>();
    let actual_providers = g
        .instructions
        .iter()
        .filter(|i| provider(&i.operation))
        .map(|i| i.id)
        .collect::<Vec<_>>();
    let calls = g
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
        .map(|i| i.id)
        .collect::<Vec<_>>();
    let expected_calls = shapes
        .iter()
        .flat_map(|s| [s.call, s.consumer])
        .collect::<Vec<_>>();
    need(
        set(&g.blocks) == set(&blocks)
            && actual.len() == edges.len()
            && set(&actual) == set(&edges)
            && set(&providers) == set(&actual_providers)
            && providers.len() == actual_providers.len()
            && set(&calls) == set(&expected_calls)
            && calls.len() == expected_calls.len(),
        "temporary exits SSA all actual role blocks/edges/providers/calls closed",
        (&g.blocks, &actual, &actual_providers, &calls),
    )?;
    let returns = shapes.iter().filter_map(|s| s.returned).collect::<Vec<_>>();
    need(
        returns.len() <= 1,
        "temporary exits SSA per-function protected Return",
        &returns,
    )?;
    uses.check(g, returns.first().copied())?;
    for entity in shapes.iter().flat_map(|s| s.protected.iter()) {
        let data = g
            .function
            .entity(*entity)
            .ok_or_else(|| failure("temporary exits SSA protected definition", entity))?;
        match data.definition {
            Definition::InstructionResult { .. } => need(
                data.origin == g.definition(*entity)?.origin,
                "temporary exits SSA actual protected producer origin",
                (entity, data),
            )?,
            Definition::BlockParameter { block, index } => need(
                g.blocks.contains(&block) && g.block(block)?.parameters.get(index) == Some(entity),
                "temporary exits SSA actual protected block parameter",
                (entity, data),
            )?,
        }
    }
    Ok(())
}

pub(super) fn check(
    program: &Program,
    entry_id: FunctionId,
    f: &Input,
    input: &Inputs,
) -> Result<(), Failure> {
    let module = one(
        program.modules.iter(),
        "temporary exits SSA sole actual module",
    )?;
    let entry = function(module, input.entry)?;
    need(
        entry.id == entry_id
            && module.functions.len() == if f.exit == Exit::Return { 8 } else { 5 },
        "temporary exits SSA returned entry and complete source callable set",
        (entry_id, &module.functions),
    )?;
    let [a, b] = &input.sites;
    need(
        a.source != b.source
            && a.factory != b.factory
            && a.read_call != b.read_call
            && a.guard.construction != b.guard.construction
            && a.guard.marker != b.guard.marker
            && a.span.end() <= b.span.start(),
        "temporary exits SSA independent actual bridged two sites",
        input,
    )?;
    let mut all = Vec::new();
    if f.exit == Exit::Return {
        need(
            a.function != b.function && a.function != input.entry && b.function != input.entry,
            "temporary exits SSA two independent scanners",
            input,
        )?;
        for site in &input.sites {
            let scan = function(module, site.function)?;
            let g = Graph::new(scan)?;
            query_callers::signature(scan, &[], scan.return_types.as_slice())?;
            need(
                matches!(scan.return_types.as_slice(),[t] if module.type_kind(*t)==Some(&SsaTypeKind::Integer{bits:64,signed:true})),
                "temporary exits SSA scanner sole Int result",
                scan,
            )?;
            let mut uses = Uses::default();
            let shape = Context {
                graph: &g,
                module,
                f,
                unit: input.unit.is_some(),
            }
            .site(site, &mut uses)?;
            need(
                Some(shape.pre) == scan.entry_block(),
                "temporary exits SSA actual scanner source birth entry",
                &shape,
            )?;
            g.incoming_from(shape.pre, &[])?;
            finish(&g, &[&shape], &uses)?;
            all.push(shape);
        }
    } else {
        query_callers::signature(entry, &[], &[])?;
        let g = Graph::new(entry)?;
        let mut uses = Uses::default();
        let c = Context {
            graph: &g,
            module,
            f,
            unit: input.unit.is_some(),
        };
        let first = c.site(a, &mut uses)?;
        let second = c.site(b, &mut uses)?;
        need(
            Some(first.pre) == entry.entry_block() && first.after == second.pre,
            "temporary exits SSA first terminal continuation before second actual birth",
            (&first, &second),
        )?;
        g.incoming_from(first.pre, &[])?;
        need(
            matches!(&g.term(second.after)?.kind,TerminatorKind::Return{values} if values.is_empty())
                && g.term(second.after)?.origin == Origin::Source(input.entry),
            "temporary exits SSA sequential sites normal entry return",
            g.term(second.after)?,
        )?;
        finish(&g, &[&first, &second], &uses)?;
        all.extend([first, second]);
    }
    need(
        all[0].protected.is_disjoint(&all[1].protected)
            && all[0].call != all[1].call
            && all[0].factory != all[1].factory
            && all[0].guard != all[1].guard
            && all[0].condition != all[1].condition,
        "temporary exits SSA complete owner/lease/cursor/KV/Guard identities disjoint",
        &all,
    )?;
    for shape in &all {
        let mut calls = Vec::new();
        for function in &module.functions {
            let g = Graph::new(function)?;
            full_graph(&g)?;
            calls.extend(g.instructions.iter().filter_map(|i| {
                matches!(i.operation,Operation::DirectCall{callee,..} if callee==shape.factory)
                    .then_some(i.id)
            }));
        }
        need(
            calls.as_slice() == [shape.call],
            "temporary exits SSA whole actual program has this unique factory source call",
            calls,
        )?;
    }
    query_callers::check(module, f, input)?;
    println!(
        "SPEC0331 TEMPORARY EXITS IDENTITY SSA entry={entry_id:?} input={input:#?} shapes={all:#?}"
    );
    Ok(())
}
