//! The hidden returned owner keeps exact three-column identity through every loop edge.
use super::*;
use crate::ssa::map_provider_exits_pipeline_tests::query_map_normal_body as body;

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
    fn via(self, g: &Graph<'_>, edge: &Edge) -> Result<Self, Failure> {
        need(
            edge.arguments.len() == 3
                && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                    == self.entities().into_iter().collect(),
            "delivery complete current three-column edge",
            (self, edge),
        )?;
        let EntityId::Value(owner) = g.transported(edge, self.entities()[0])? else {
            return Err(failure("delivery owner column", edge));
        };
        let EntityId::Loan(source) = g.transported(edge, self.entities()[1])? else {
            return Err(failure("delivery source column", edge));
        };
        let EntityId::Value(cursor) = g.transported(edge, self.entities()[2])? else {
            return Err(failure("delivery cursor column", edge));
        };
        Ok(Self {
            owner,
            source,
            cursor,
        })
    }
}
pub(super) fn check<'ir>(
    g: &Graph<'ir>,
    module: &'ir Module,
    reader: &'ir Function,
    parsed: &ParsedFile,
    site: &ast::Site,
    selected: (&arguments::Delivery<'ir>, &Instruction),
    uses: &mut Uses,
) -> Result<(), Failure> {
    let (delivered, after) = selected;
    let call = delivered.call;
    let owner = value(call)?;
    let map = g
        .function
        .entity(EntityId::Value(owner))
        .unwrap()
        .ty
        .semantic_type();
    let (_, key_type, value_type) = module
        .map_container(map)
        .ok_or_else(|| failure("delivery provider output Map", map))?;
    let root = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::RootPlace { owner }),
        "delivery actual returned owner root",
    )?;
    let EntityId::Place(place) = result(root)? else {
        return Err(failure("delivery actual provider root place", root));
    };
    let begin = one(
        g.instructions.iter().copied().filter(|i| {
            i.operation
                == Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                }
        }),
        "delivery actual returned source loan",
    )?;
    let source = loan(begin)?;
    let first = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationFirst { .. })
    })?;
    need(
        first.operation == Operation::MapIterationFirst { source }
            && [root, begin, first]
                .iter()
                .all(|i| i.block == call.block && i.origin == Origin::Source(site.span))
            && g.position(call)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(first)?
            && delivered.cleanup.iter().all(|drop| {
                drop.block == root.block && g.position(drop).unwrap() < g.position(root).unwrap()
            }),
        "delivery all argument cleanup precedes output provider Root/Begin/First",
        (root, begin, first),
    )?;
    let initial = Roles {
        owner,
        source,
        cursor: value(first)?,
    };
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.permit(EntityId::Loan(source), first);
    let entry = g.branch(first.block)?;
    let header = initial.via(g, entry)?;
    let has = one(
        g.ops(entry.target)?.into_iter(),
        "delivery sole provider HasNext",
    )?;
    need(
        has.operation
            == Operation::MapIterationHasNext {
                source: header.source,
                cursor: header.cursor,
            }
            && has.origin == Origin::Source(site.span),
        "delivery current header source and cursor",
        has,
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("delivery actual HasNext branch", has));
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
                !g.term(block)
                    .unwrap()
                    .kind
                    .entities()
                    .contains(&EntityId::Value(*condition))
                    || block == has.block
                        && !when_true.arguments.contains(&EntityId::Value(*condition))
                        && !when_false.arguments.contains(&EntityId::Value(*condition))
            }),
        "delivery HasNext result has only its real condition use",
        has,
    )?;
    let current = header.via(g, when_true)?;
    let exit = header.via(g, when_false)?;
    let next = body::check(
        g,
        reader,
        body::Body {
            block: when_true.target,
            span: site.span,
            read: ast::expression(parsed, site.read),
            arguments: site.arguments.map(|id| ast::expression(parsed, id)),
            types: [key_type, value_type],
        },
        current.source,
        current.cursor,
        uses,
    )?;
    let back = g.branch(when_true.target)?;
    need(
        back.target == has.block
            && Roles {
                cursor: value(next)?,
                ..current
            }
            .via(g, back)?
                == header,
        "delivery complete actual normal backedge",
        back,
    )?;
    g.incoming_from(has.block, &[first.block, when_true.target])?;
    g.incoming_from(when_true.target, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    for frame in [initial, header, current, exit] {
        for entity in frame.entities() {
            uses.protect(entity);
        }
        anchored(g, EntityId::Value(frame.owner), &[EntityId::Value(owner)])?;
        anchored(g, EntityId::Loan(frame.source), &[EntityId::Loan(source)])?;
        anchored(
            g,
            EntityId::Value(frame.cursor),
            &[result(first)?, result(next)?],
        )?;
        need(
            !delivered.argument_owners.contains(&frame.owner)
                && g.function.entity(EntityId::Value(frame.owner)).unwrap().ty
                    == EntityType::Value(map)
                && g.function.entity(EntityId::Loan(frame.source)).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map,
                    }
                && matches!(g.function.entity(EntityId::Value(frame.cursor)).unwrap().ty,EntityType::Value(ty) if module.type_kind(ty)==Some(&SsaTypeKind::Integer {bits:64,signed:false})),
            "delivery every transported output role has exact type and disjoint identity",
            frame,
        )?;
    }
    uses.permit(EntityId::Loan(header.source), has);
    uses.permit(EntityId::Value(header.cursor), has);
    let end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::BorrowEnd { loan: exit.source }),
        "delivery output source unique End",
    )?;
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner: exit.owner }),
        "delivery current output unique Drop",
    )?;
    let ops = g.ops(when_false.target)?;
    need(
        end.block == when_false.target
            && drop.block == end.block
            && after.block == end.block
            && end.origin == Origin::Source(site.span)
            && drop.origin == call.origin
            && ops.first().map(|i| i.id) == Some(end.id)
            && ops.get(1).map(|i| i.id) == Some(drop.id)
            && g.position(drop)? < g.position(after)?
            && g.blocks.len() == 4
            && g.edges.len() == 4
            && matches!(&g.term(end.block)?.kind,TerminatorKind::Return {values} if values.is_empty())
            && g.blocks
                .iter()
                .filter(|&&b| matches!(g.term(b).unwrap().kind, TerminatorKind::Return { .. }))
                .count()
                == 1,
        "delivery output End/current Drop/after/unique normal Return",
        (end, drop, after),
    )?;
    uses.permit(EntityId::Loan(exit.source), end);
    uses.permit(EntityId::Value(exit.owner), drop);
    let output_owners = [initial.owner, header.owner, current.owner, exit.owner];
    for (index, data) in g.function.values.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            need(
                output_owners.iter().any(|owner| owner.index() == index)
                    || delivered
                        .argument_owners
                        .iter()
                        .any(|owner| owner.index() == index),
                "delivery provider has only the two proven disjoint owner families",
                (index, data),
            )?;
        }
    }
    Ok(())
}
