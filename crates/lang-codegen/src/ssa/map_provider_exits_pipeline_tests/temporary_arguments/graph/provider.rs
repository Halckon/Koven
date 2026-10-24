//! One normal provider with complete current-owner/source/cursor transport and component uses.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Frame {
    owner: ValueId,
    source: LoanId,
    cursor: ValueId,
}

impl Frame {
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
            "argument provider full current transport columns",
            (edge, self),
        )?;
        let EntityId::Value(owner) = g.transported(edge, self.entities()[0])? else {
            return Err(failure("argument provider owner column", edge));
        };
        let EntityId::Loan(source) = g.transported(edge, self.entities()[1])? else {
            return Err(failure("argument provider source column", edge));
        };
        let EntityId::Value(cursor) = g.transported(edge, self.entities()[2])? else {
            return Err(failure("argument provider cursor column", edge));
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
    site: &ast::Loop,
    selected: (ValueId, &Instruction, &Instruction, &Instruction),
    uses: &mut Uses,
) -> Result<(), Failure> {
    let (owner, call, argument_end, after) = selected;
    let root = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::RootPlace { owner }),
        "argument provider real factory-result root",
    )?;
    let EntityId::Place(place) = result(root)? else {
        return Err(failure("argument provider actual root place", root));
    };
    let begin = one(
        g.instructions.iter().copied().filter(|i| {
            i.operation
                == Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                }
        }),
        "argument provider actual source lease",
    )?;
    let initial = Frame {
        owner,
        source: loan(begin)?,
        cursor: value(at(g, site.span, |o| {
            matches!(o, Operation::MapIterationFirst { .. })
        })?)?,
    };
    let first = g.definition(EntityId::Value(initial.cursor))?;
    need(
        first.operation
            == Operation::MapIterationFirst {
                source: initial.source,
            }
            && root.origin == Origin::Source(site.span)
            && begin.origin == root.origin
            && [root, begin, first].iter().all(|i| i.block == call.block)
            && g.position(argument_end)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(first)?,
        "argument factory Borrow ends before this provider borrows its returned owner",
        (argument_end, root, begin, first),
    )?;
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.permit(EntityId::Loan(initial.source), first);
    let entry = g.branch(first.block)?;
    let header = initial.via(g, entry)?;
    let has = one(
        g.ops(entry.target)?.into_iter(),
        "argument provider actual sole HasNext",
    )?;
    need(
        has.operation
            == Operation::MapIterationHasNext {
                source: header.source,
                cursor: header.cursor,
            }
            && has.origin == Origin::Source(site.span),
        "argument provider actual current HasNext",
        has,
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("argument actual provider guard", has));
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
        "argument provider HasNext has only its actual condition use",
        has,
    )?;
    let body_frame = header.via(g, when_true)?;
    let exit = header.via(g, when_false)?;
    let next = body(
        g,
        module,
        reader,
        parsed,
        site,
        (when_true.target, body_frame),
        uses,
    )?;
    let back = g.branch(when_true.target)?;
    need(
        back.target == has.block
            && Frame {
                cursor: value(next)?,
                ..body_frame
            }
            .via(g, back)?
                == header,
        "argument provider real complete normal backedge",
        back,
    )?;
    g.incoming_from(has.block, &[first.block, when_true.target])?;
    g.incoming_from(when_true.target, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let map = g
        .function
        .entity(EntityId::Value(owner))
        .unwrap()
        .ty
        .semantic_type();
    for frame in [initial, header, body_frame, exit] {
        for entity in frame.entities() {
            uses.protect(entity);
        }
        anchored(g, EntityId::Value(frame.owner), &[EntityId::Value(owner)])?;
        anchored(
            g,
            EntityId::Loan(frame.source),
            &[EntityId::Loan(initial.source)],
        )?;
        anchored(
            g,
            EntityId::Value(frame.cursor),
            &[EntityId::Value(initial.cursor), result(next)?],
        )?;
        need(
            g.function.entity(EntityId::Value(frame.owner)).unwrap().ty == EntityType::Value(map)
                && g.function.entity(EntityId::Loan(frame.source)).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map,
                    }
                && matches!(g.function.entity(EntityId::Value(frame.cursor)).unwrap().ty,EntityType::Value(ty) if module.type_kind(ty)==Some(&SsaTypeKind::Integer {bits:64,signed:false})),
            "argument provider every actual column has correct represented type",
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
        "argument provider actual false source End",
    )?;
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner: exit.owner }),
        "argument provider unique actual current Map Drop",
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
        "argument provider exact End/Call-origin current Drop/after/only Return",
        (end, drop, after),
    )?;
    uses.permit(EntityId::Loan(exit.source), end);
    uses.permit(EntityId::Value(exit.owner), drop);
    let mut expected = BTreeSet::from([EntityId::Place(place)]);
    for frame in [initial, header, body_frame, exit] {
        need(
            expected.insert(EntityId::Value(frame.owner))
                && expected.insert(EntityId::Loan(frame.source)),
            "Unit temporary owner and source are distinct current roles at every boundary",
            frame,
        )?;
    }
    // The old Always/empty-phi FE obligations are now these nonoptional current
    // columns, all incoming edges, exact retirement, and the complete definition inventory.
    crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(g, &[map], &expected)
}

fn body<'ir>(
    g: &Graph<'ir>,
    module: &Module,
    reader: &Function,
    parsed: &ParsedFile,
    site: &ast::Loop,
    current: (BlockId, Frame),
    uses: &mut Uses,
) -> Result<&'ir Instruction, Failure> {
    let (block, frame) = current;
    let key = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationKeyLoan { .. })
    })?;
    let val = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationValueLoan { .. })
    })?;
    let next = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationNext { .. })
    })?;
    let roots = [loan(key)?, loan(val)?];
    let parameters = &reader
        .block(reader.entry_block().unwrap())
        .unwrap()
        .parameters;
    need(key.operation == Operation::MapIterationKeyLoan {source:frame.source,cursor:frame.cursor}
        && val.operation == Operation::MapIterationValueLoan {source:frame.source,cursor:frame.cursor}
        && next.operation == Operation::MapIterationNext {source:frame.source,cursor:frame.cursor}
        && [key,val,next].iter().all(|i|i.block==block) && parameters.len()==2 && reader.return_types.is_empty()
        && roots.iter().zip(parameters).all(|(&root,&parameter)| {
            let ty=g.function.entity(EntityId::Loan(root)).unwrap().ty;
            matches!(ty,EntityType::Loan {kind:LoanKind::Shared,target} if module.type_kind(target)==Some(&SsaTypeKind::StringOwner))
                && reader.entity(parameter).unwrap().ty==ty
        }),"argument provider actual K/V Shared reader ABI",(key,val,reader.id))?;
    let mut lineage = BTreeMap::<LoanId, (usize, Option<LoanId>)>::new();
    let mut live = BTreeSet::new();
    let mut ended = Vec::new();
    let mut called = false;
    let ops = g.ops(block)?;
    for instruction in &ops {
        match &instruction.operation {
            Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. } => {
                let current = loan(instruction)?;
                let index = if current == roots[0] { 0 } else { 1 };
                need(
                    roots.contains(&current)
                        && lineage.insert(current, (index, None)).is_none()
                        && live.insert(current),
                    "argument actual K/V loan birth",
                    instruction,
                )?;
                uses.protect(EntityId::Loan(current));
                uses.permit(EntityId::Loan(frame.source), instruction);
                uses.permit(EntityId::Value(frame.cursor), instruction);
            }
            Operation::SharedReborrow { source } => {
                let (index, _) = *lineage.get(source).ok_or_else(|| {
                    failure("argument actual component reborrow parent", instruction)
                })?;
                let current = loan(instruction)?;
                need(
                    live.contains(source)
                        && instruction.origin
                            == Origin::Source(ast::span(parsed, site.arguments[index]))
                        && g.function.entity(EntityId::Loan(current)).unwrap().ty
                            == g.function.entity(EntityId::Loan(*source)).unwrap().ty
                        && lineage.insert(current, (index, Some(*source))).is_none()
                        && live.insert(current),
                    "argument actual live typed K/V reborrow",
                    instruction,
                )?;
                uses.permit(EntityId::Loan(*source), instruction);
                uses.protect(EntityId::Loan(current));
            }
            Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } => {
                need(
                    !called
                        && *callee == reader.id
                        && receiver.is_none()
                        && arguments.len() == 2
                        && instruction.results.is_empty()
                        && instruction.origin == Origin::Source(ast::span(parsed, site.read)),
                    "argument real component read Call",
                    instruction,
                )?;
                for (index, &entity) in arguments.iter().enumerate() {
                    let EntityId::Loan(current) = entity else {
                        return Err(failure("argument reader actual Shared operand", entity));
                    };
                    need(
                        live.contains(&current)
                            && lineage
                                .get(&current)
                                .is_some_and(|(slot, _)| *slot == index),
                        "argument actual ordered K/V lineage",
                        entity,
                    )?;
                    uses.permit(entity, instruction);
                }
                called = true;
            }
            Operation::BorrowEnd { loan } => {
                let (_, parent) = *lineage.get(loan).ok_or_else(|| {
                    failure("argument actual ended component lineage", instruction)
                })?;
                need(
                    called
                        && live.contains(loan)
                        && !lineage.iter().any(|(child, (_, parent))| {
                            *parent == Some(*loan) && live.contains(child)
                        })
                        && instruction.origin
                            == Origin::Source(if parent.is_some() {
                                ast::span(parsed, site.read)
                            } else {
                                site.span
                            }),
                    "argument child End before parent after real read",
                    instruction,
                )?;
                live.remove(loan);
                if parent.is_none() {
                    ended.push(*loan);
                }
                uses.permit(EntityId::Loan(*loan), instruction);
            }
            Operation::MapIterationNext { .. } => {
                need(
                    instruction.id == next.id
                        && called
                        && live.is_empty()
                        && ended == [roots[1], roots[0]],
                    "argument read/child Ends/VEnd/KEnd/Next",
                    instruction,
                )?;
                uses.permit(EntityId::Loan(frame.source), instruction);
                uses.permit(EntityId::Value(frame.cursor), instruction);
                uses.protect(result(instruction)?);
            }
            _ => {
                return Err(failure(
                    "argument unexpected provider body operation",
                    instruction,
                ));
            }
        }
    }
    need(
        called
            && live.is_empty()
            && ended == [roots[1], roots[0]]
            && ops.first().map(|i| i.id) == Some(key.id)
            && g.position(key)? < g.position(val)?
            && ops.last().map(|i| i.id) == Some(next.id)
            && g.edges.iter().all(|(_, edge)| {
                edge.arguments.iter().all(
                    |entity| !matches!(entity,EntityId::Loan(loan) if lineage.contains_key(loan)),
                )
            }),
        "argument complete body and no component loan escape",
        ended,
    )?;
    Ok(next)
}
