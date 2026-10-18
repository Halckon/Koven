//! Each actual factory Call keeps its own hidden owner through every provider edge.
mod body;
mod representation;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Roles {
    owner: ValueId,
    source: LoanId,
    cursor: ValueId,
}

impl Roles {
    fn entities(self) -> [EntityId; 3] {
        [
            EntityId::Value(self.owner),
            EntityId::Loan(self.source),
            EntityId::Value(self.cursor),
        ]
    }
    fn protect(self, uses: &mut Uses) {
        for entity in self.entities() {
            uses.protect(entity);
        }
    }
    fn transported(self, g: &Graph<'_>, edge: &Edge) -> Result<Self, Failure> {
        let entities = self.entities();
        need(
            edge.arguments.len() == 3
                && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                    == entities.into_iter().collect(),
            "temporary form exact complete three-column edge",
            (edge, self),
        )?;
        let EntityId::Value(owner) = g.transported(edge, entities[0])? else {
            return Err(failure("temporary form owner column", edge));
        };
        let EntityId::Loan(source) = g.transported(edge, entities[1])? else {
            return Err(failure("temporary form source column", edge));
        };
        let EntityId::Value(cursor) = g.transported(edge, entities[2])? else {
            return Err(failure("temporary form cursor column", edge));
        };
        Ok(Self {
            owner,
            source,
            cursor,
        })
    }
}

// Every visited node must be anchored. A separate incoming cycle cannot borrow
// the proof from a different incoming path that happens to reach the producer.
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
            .ok_or_else(|| failure("temporary form transported entity", entity))?;
        let Definition::BlockParameter { block, .. } = data.definition else {
            return Err(failure(
                "temporary form transport reaches another producer",
                (entity, data),
            ));
        };
        let incoming = g.incoming(block);
        need(
            !incoming.is_empty(),
            "temporary form complete incoming exists",
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
        "temporary form every incoming node anchored to actual producer",
        (current, seen, reached),
    )
}

struct Shape<'a> {
    call: &'a Instruction,
    drop: &'a Instruction,
    after: &'a Instruction,
    pre: BlockId,
    exit: BlockId,
    owners: BTreeSet<EntityId>,
}

struct Query<'a, 'ir> {
    g: &'a Graph<'ir>,
    module: &'ir Module,
    reader: &'ir Function,
    case: &'a Case,
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
}

fn marker<'a>(g: &Graph<'a>, site: &ast::Site) -> Result<&'a Instruction, Failure> {
    one(g.instructions.iter().copied().filter(|i| {
        if i.origin != Origin::Source(site.after) { return false; }
        match &i.operation {
            Operation::PrintLiteral {bytes} => bytes.as_slice() == format!("{}\n", site.marker).as_bytes(),
            Operation::PrintString {value} => {
                let Ok(begin) = g.definition(EntityId::Loan(*value)) else { return false; };
                let Operation::BorrowBegin {place, kind:LoanKind::Shared} = begin.operation else { return false; };
                let Ok(root) = g.definition(EntityId::Place(place)) else { return false; };
                let Operation::RootPlace {owner} = root.operation else { return false; };
                g.definition(EntityId::Value(owner)).is_ok_and(|literal|
                    matches!(&literal.operation, Operation::StringLiteral {bytes, ..} if bytes.as_slice() == site.marker.as_bytes()))
            }
            _ => false,
        }
    }), "temporary form original after-marker actual bytes")
}

impl<'ir> Query<'_, 'ir> {
    fn site(&self, site: &ast::Site, empty: bool, uses: &mut Uses) -> Result<Shape<'ir>, Failure> {
        let g = self.g;
        let factory = function(self.module, site.factory.span)?;
        let call = one(g.instructions.iter().copied().filter(|i|
            matches!(i.operation, Operation::DirectCall {callee, ..} if callee == factory.id)), "temporary form unique selected factory evaluation")?;
        need(
            call.origin == Origin::Source(site.call)
                && matches!(&call.operation, Operation::DirectCall {receiver:None, arguments, ..} if arguments.is_empty()),
            "temporary form exact source Callspan and actual zeroarg callee",
            call,
        )?;
        let owner = value(call)?;
        let map_type = g
            .function
            .entity(EntityId::Value(owner))
            .unwrap()
            .ty
            .semantic_type();
        need(
            factory.return_types.as_slice() == [map_type]
                && factory.receiver.is_none()
                && factory.borrow_return.is_none()
                && factory.carrier_return.is_none()
                && factory
                    .block(factory.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .is_empty(),
            "temporary form selected factory returns real owned Map",
            factory,
        )?;
        let (key_type, value_type) = representation::check(
            self.case,
            self.module,
            self.sources,
            self.parsed,
            map_type,
            factory,
            empty,
        )?;
        let root = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::RootPlace { owner }),
            "temporary form original owner root",
        )?;
        let EntityId::Place(place) = result(root)? else {
            return Err(failure("temporary form actual place", root));
        };
        let begin = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    }
            }),
            "temporary form actual source shared lease",
        )?;
        let source = loan(begin)?;
        let first = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::MapIterationFirst { source }),
            "temporary form actual First source",
        )?;
        let initial = Roles {
            owner,
            source,
            cursor: value(first)?,
        };
        need(
            [root, begin, first]
                .iter()
                .all(|i| i.block == call.block && i.origin == Origin::Source(site.span))
                && g.ops(call.block)?
                    .into_iter()
                    .skip(g.position(call)?)
                    .map(|i| i.id)
                    .collect::<Vec<_>>()
                    == [call.id, root.id, begin.id, first.id]
                && g.function.entity(EntityId::Place(place)).unwrap().ty
                    == EntityType::Place(map_type)
                && g.function.entity(EntityId::Loan(source)).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map_type,
                    },
            "temporary form exact Call/Root/Borrow/First preheader suffix and types",
            (call, root, begin, first),
        )?;
        initial.protect(uses);
        uses.permit(EntityId::Value(owner), root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        uses.permit(EntityId::Loan(source), first);
        let entry = g.branch(call.block)?;
        let header = initial.transported(g, entry)?;
        header.protect(uses);
        let has = one(
            g.ops(entry.target)?.into_iter(),
            "temporary form header contains only HasNext",
        )?;
        need(
            has.operation
                == Operation::MapIterationHasNext {
                    source: header.source,
                    cursor: header.cursor,
                }
                && has.origin == Origin::Source(site.span),
            "temporary form real current source/cursor HasNext",
            has,
        )?;
        uses.permit(EntityId::Loan(header.source), has);
        uses.permit(EntityId::Value(header.cursor), has);
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &g.term(has.block)?.kind
        else {
            return Err(failure("temporary form actual HasNext conditional", has));
        };
        need(
            *condition == value(has)?
                && g.term(has.block)?.origin == Origin::Source(site.span)
                && g.instructions.iter().all(|i| {
                    !i.operation
                        .entities()
                        .contains(&EntityId::Value(*condition))
                })
                && g.blocks.iter().all(|&block| {
                    let term = g.term(block).unwrap();
                    !term.kind.entities().contains(&EntityId::Value(*condition))
                        || block == has.block
                            && !when_true.arguments.contains(&EntityId::Value(*condition))
                            && !when_false.arguments.contains(&EntityId::Value(*condition))
                }),
            "temporary form HasNext condition has only this actual branch use",
            has,
        )?;
        let body_roles = header.transported(g, when_true)?;
        let exit_roles = header.transported(g, when_false)?;
        body_roles.protect(uses);
        exit_roles.protect(uses);
        need(
            [call.block, has.block, when_true.target, when_false.target]
                .into_iter()
                .collect::<BTreeSet<_>>()
                .len()
                == 4,
            "temporary form distinct pre/header/body/false blocks",
            site,
        )?;
        for block in [has.block, when_true.target, when_false.target] {
            need(
                g.block(block)?.origin == Origin::Source(site.span)
                    && g.block(block)?.parameters.iter().all(|parameter| {
                        g.function.entity(*parameter).unwrap().origin == Origin::Source(site.span)
                    }),
                "temporary form actual For block and parameter provenance",
                block,
            )?;
        }
        g.incoming_from(has.block, &[call.block, when_true.target])?;
        g.incoming_from(when_true.target, &[has.block])?;
        g.incoming_from(when_false.target, &[has.block])?;
        let next = body::check(
            self,
            site,
            when_true.target,
            body_roles,
            key_type,
            value_type,
            uses,
        )?;
        let back = g.branch(when_true.target)?;
        let advanced = Roles {
            cursor: value(next)?,
            ..body_roles
        };
        need(
            back.target == has.block
                && advanced.transported(g, back)? == header
                && g.term(call.block)?.origin == Origin::Source(site.span)
                && g.term(when_true.target)?.origin == Origin::Source(site.span),
            "temporary form every backedge transports current owner/source and real Next",
            back,
        )?;
        let roles = [initial, header, body_roles, exit_roles];
        for current in roles {
            anchored(g, EntityId::Value(current.owner), &[EntityId::Value(owner)])?;
            anchored(g, EntityId::Loan(current.source), &[EntityId::Loan(source)])?;
            anchored(
                g,
                EntityId::Value(current.cursor),
                &[EntityId::Value(initial.cursor), result(next)?],
            )?;
            need(
                g.function
                    .entity(EntityId::Value(current.owner))
                    .unwrap()
                    .ty
                    == EntityType::Value(map_type)
                    && g.function
                        .entity(EntityId::Loan(current.source))
                        .unwrap()
                        .ty
                        == EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: map_type,
                        }
                    && matches!(g.function.entity(EntityId::Value(current.cursor)).unwrap().ty, EntityType::Value(ty)
                    if self.module.type_kind(ty) == Some(&SsaTypeKind::Integer {bits:64, signed:false})),
                "temporary form every transported column preserves actual representation",
                current,
            )?;
        }
        let end = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowEnd {
                        loan: exit_roles.source,
                    }
            }),
            "temporary form false actual EndSource",
        )?;
        let drop = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::Drop {
                        owner: exit_roles.owner,
                    }
            }),
            "temporary form unique current-owner Map Drop",
        )?;
        let prefix = g.ops(when_false.target)?;
        need(
            end.block == when_false.target
                && drop.block == end.block
                && end.origin == Origin::Source(site.span)
                && drop.origin == Origin::Source(site.call)
                && prefix.first().map(|i| i.id) == Some(end.id)
                && prefix.get(1).map(|i| i.id) == Some(drop.id),
            "temporary form false EndSource then precise Call-origin owner Drop",
            (end, drop),
        )?;
        uses.permit(EntityId::Loan(exit_roles.source), end);
        uses.permit(EntityId::Value(exit_roles.owner), drop);
        let after = marker(g, site)?;
        need(
            after.block == drop.block && g.position(drop)? < g.position(after)?,
            "temporary form owning cleanup before its exact after marker",
            (drop, after),
        )?;
        Ok(Shape {
            call,
            drop,
            after,
            pre: call.block,
            exit: drop.block,
            owners: roles
                .into_iter()
                .map(|r| EntityId::Value(r.owner))
                .collect(),
        })
    }
}

pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
) -> Result<(), Failure> {
    let input = ast::input(sources, parsed)?;
    let module = one(program.modules.iter(), "temporary forms actual module")?;
    let entry = function(module, input.entry.span)?;
    let reader = function(module, input.reader.span)?;
    let g = Graph::new(entry)?;
    need(
        g.block(entry.entry_block().unwrap())?.parameters.is_empty(),
        "temporary forms entry has no invented owner parameter",
        entry.id,
    )?;
    let query = Query {
        g: &g,
        module,
        reader,
        case,
        sources,
        parsed,
    };
    let mut uses = Uses::default();
    let first = query.site(&input.sites[0], false, &mut uses)?;
    let second = query.site(&input.sites[1], true, &mut uses)?;
    need(
        first.owners.is_disjoint(&second.owners)
            && first.exit == second.pre
            && g.position(first.drop)? < g.position(first.after)?
            && g.position(first.after)? < g.position(second.call)?
            && first.pre == entry.entry_block().unwrap()
            && g.blocks.len() == 7
            && g.edges.len() == 8,
        "temporary forms disjoint hidden-owner families and complete sequential CFG",
        (&first.owners, &second.owners),
    )?;
    let drops = g.instructions.iter().filter(|i| matches!(i.operation, Operation::Drop {owner}
        if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).map(|i| i.id).collect::<BTreeSet<_>>();
    need(
        drops == BTreeSet::from([first.drop.id, second.drop.id])
            && matches!(&g.term(second.exit)?.kind, TerminatorKind::Return {values} if values.is_empty())
            && g.blocks
                .iter()
                .filter(|&&block| {
                    matches!(g.term(block).unwrap().kind, TerminatorKind::Return { .. })
                })
                .count()
                == 1,
        "temporary forms exactly two distinct Map Drops before the only normal Return",
        drops,
    )?;
    uses.check(&g, None)
}
