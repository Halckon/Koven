//! The actual output owner and lease keep exact identities through every provider edge.
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
            "birth complete current three-column provider edge",
            (self, edge),
        )?;
        let EntityId::Value(owner) = g.transported(edge, self.entities()[0])? else {
            return Err(failure("birth actual owner column", edge));
        };
        let EntityId::Loan(source) = g.transported(edge, self.entities()[1])? else {
            return Err(failure("birth actual source column", edge));
        };
        let EntityId::Value(cursor) = g.transported(edge, self.entities()[2])? else {
            return Err(failure("birth actual cursor column", edge));
        };
        Ok(Self {
            owner,
            source,
            cursor,
        })
    }
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) fn check(
    g: &Graph<'_>,
    module: &Module,
    reader: &Function,
    parsed: &ParsedFile,
    selected: (&ast::Site, EntityId, &Instruction, &Instruction),
    uses: &mut Uses,
) -> Result<(), Failure> {
    let (site, seed, drop, after) = selected;
    let EntityId::Value(owner) = seed else {
        return Err(failure(
            "birth provider source must be an owner Value",
            seed,
        ));
    };
    let map = g.function.entity(seed).unwrap().ty.semantic_type();
    let (_, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("birth provider actual map storage", seed))?;
    let root = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::RootPlace { owner }),
        "birth provider roots the actual current seed",
    )?;
    let EntityId::Place(place) = result(root)? else {
        return Err(failure("birth provider actual root Place", root));
    };
    let begin = one(
        g.instructions.iter().copied().filter(|i| {
            i.operation
                == Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                }
        }),
        "birth actual provider Shared lease",
    )?;
    let source = loan(begin)?;
    let first = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationFirst { .. })
    })?;
    let seed_definition = g.definition(seed)?;
    need(
        root.origin == Origin::Source(site.span)
            && begin.origin == root.origin
            && first.origin == root.origin
            && first.operation == Operation::MapIterationFirst { source }
            && [seed_definition, root, begin]
                .iter()
                .all(|i| i.block == first.block)
            && g.position(seed_definition)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(first)?,
        "birth actual producer/current Root/Shared/First sequence",
        (seed_definition, root, begin, first),
    )?;
    uses.permit(seed, root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.permit(EntityId::Loan(source), first);
    let initial = Roles {
        owner,
        source,
        cursor: value(first)?,
    };
    let entry = g.branch(first.block)?;
    let header = initial.via(g, entry)?;
    let has = one(
        g.ops(entry.target)?.into_iter(),
        "birth header sole HasNext",
    )?;
    need(
        has.operation
            == Operation::MapIterationHasNext {
                source: header.source,
                cursor: header.cursor,
            }
            && has.origin == Origin::Source(site.span),
        "birth actual current header operands",
        has,
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("birth actual HasNext Conditional", has));
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
                block == has.block
                    || !g
                        .term(block)
                        .unwrap()
                        .kind
                        .entities()
                        .contains(&EntityId::Value(*condition))
            })
            && !when_true.arguments.contains(&EntityId::Value(*condition))
            && !when_false.arguments.contains(&EntityId::Value(*condition)),
        "birth HasNext Boolean has only its actual guard use",
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
            read: ast::span(parsed, site.read),
            arguments: site.arguments.map(|id| ast::span(parsed, id)),
            types: [key, val],
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
        "birth complete actual current backedge",
        back,
    )?;
    g.incoming_from(has.block, &[first.block, when_true.target])?;
    g.incoming_from(when_true.target, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    for roles in [initial, header, current, exit] {
        for e in roles.entities() {
            uses.protect(e);
        }
        g.anchored(EntityId::Value(roles.owner), &[seed])?;
        g.anchored(EntityId::Loan(roles.source), &[EntityId::Loan(source)])?;
        g.anchored(
            EntityId::Value(roles.cursor),
            &[result(first)?, result(next)?],
        )?;
        need(
            g.function.entity(EntityId::Value(roles.owner)).unwrap().ty == EntityType::Value(map)
                && g.function.entity(EntityId::Loan(roles.source)).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map,
                    },
            "birth every owner/source role retains exact type",
            roles,
        )?;
    }
    uses.permit(EntityId::Loan(header.source), has);
    uses.permit(EntityId::Value(header.cursor), has);
    let end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::BorrowEnd { loan: exit.source }),
        "birth unique current source End",
    )?;
    let ops = g.ops(when_false.target)?;
    need(
        end.block == when_false.target
            && end.origin == Origin::Source(site.span)
            && drop.block == end.block
            && after.block == end.block
            && drop.operation == Operation::Drop { owner: exit.owner }
            && ops.first().map(|i| i.id) == Some(end.id)
            && ops.get(1).map(|i| i.id) == Some(drop.id)
            && g.position(drop)? < g.position(after)?
            && matches!(&g.term(end.block)?.kind,TerminatorKind::Return{values}if values.is_empty()),
        "birth source End/current output Drop/after/normal Return exact order",
        (end, drop, after),
    )?;
    uses.permit(EntityId::Loan(exit.source), end);
    uses.permit(EntityId::Value(exit.owner), drop);
    Ok(())
}
