//! Complete actual provider plus the original conditional update and later source delivery.
use crate::test_support::diagnostics::diagnostic;
mod conditional {
    //! Named-source carried columns, with the implicit false edge kept distinct.
    use super::*;
    pub(super) struct Joined<'a> {
        pub owner: EntityId,
        pub block: BlockId,
        pub blocks: [BlockId; 4],
        pub old_drop: Option<&'a Instruction>,
    }
    pub(super) fn check<'a>(
        g: &Graph<'a>,
        p: &ParsedFile,
        i: &Input,
        (old, new): (EntityId, EntityId),
        old_origin: Option<Span>,
        uses: &mut Uses,
    ) -> Result<Joined<'a>, Failure> {
        let (expr, condition) = i
            .conditional
            .ok_or_else(|| failure("update original conditional AST", i))?;
        let header = one(
            g.blocks.iter().copied().filter(|&b| {
                g.term(b).is_ok_and(|t| {
                    t.origin == Origin::Source(span(p, expr))
                        && matches!(t.kind, TerminatorKind::Conditional { .. })
                })
            }),
            "update original conditional write header",
        )?;
        let TerminatorKind::Conditional {
            condition: actual,
            when_true,
            when_false,
        } = &g.term(header)?.kind
        else {
            unreachable!()
        };
        let read = g.definition(EntityId::Value(*actual))?;
        let Operation::Read {
            source: PlaceAccess::Loan(flag),
        } = read.operation
        else {
            return Err(failure("update actual original flag Read", read));
        };
        let original = *one(
            g.block(g.function.entry_block().unwrap())?
                .parameters
                .iter(),
            "update actual scanner flag parameter",
        )?;
        g.anchored(EntityId::Loan(flag), &[original])?;
        need(
            read.origin == Origin::Source(span(p, condition)) && read.block == header,
            "update condition preserves original flag identity and polarity",
            read,
        )?;
        for &block in &g.blocks {
            for &parameter in &g.block(block)?.parameters {
                if g.function.entity(parameter).unwrap().ty
                    == g.function.entity(original).unwrap().ty
                {
                    g.anchored(parameter, &[original])?;
                    uses.protect(parameter);
                }
            }
        }
        uses.permit(EntityId::Loan(flag), read);
        let new_definition = g.definition(new)?;
        need(
            new_definition.block == when_true.target,
            "update only true arm installs the actual new Call/put value",
            new_definition,
        )?;
        if let Operation::MapPut { owner, .. } = new_definition.operation {
            g.anchored(EntityId::Value(owner), &[old])?;
            need(
                new != EntityId::Value(owner),
                "update put yields distinct SSA current owner",
                new_definition,
            )?;
        }
        let yes = g.branch(when_true.target)?;
        let no = g.branch(when_false.target)?;
        need(
            yes.target == no.target,
            "update actual named carried true/implicit-false join",
            (yes, no),
        )?;
        g.incoming_from(when_true.target, &[header])?;
        g.incoming_from(when_false.target, &[header])?;
        g.incoming_from(yes.target, &[when_true.target, when_false.target])?;
        let owner = one(
            g.block(yes.target)?
                .parameters
                .iter()
                .copied()
                .filter(|&param| {
                    g.argument(yes, param).is_ok_and(|arg| arg == new)
                        && g.argument(no, param)
                            .is_ok_and(|arg| g.anchored(arg, &[old]).is_ok())
                }),
            "update exact carried named-source column, never arbitrary join0",
        )?;
        g.anchored(owner, &[old, new])?;
        let old_drop = if let Some(origin) = old_origin {
            let drop=one(g.instructions.iter().copied().filter(|op|op.block==when_true.target&&op.origin==Origin::Source(origin)&&matches!(op.operation,Operation::Drop{owner}if g.anchored(EntityId::Value(owner),&[old]).is_ok())),"update conditional true replacement old current owner Drop")?;
            need(
                g.position(new_definition)? < g.position(drop)?
                    && g.ops(drop.block)?.last().map(|op| op.id) == Some(drop.id),
                "update replacement RHS completes before old Drop and actual new incoming installation",
                (new_definition, drop),
            )?;
            let Operation::Drop { owner } = drop.operation else {
                unreachable!()
            };
            uses.permit(EntityId::Value(owner), drop);
            Some(drop)
        } else {
            None
        };
        Ok(Joined {
            owner,
            block: yes.target,
            blocks: [header, when_true.target, when_false.target, yes.target],
            old_drop,
        })
    }
}
mod provider {
    //! Full current owner/source/cursor and independent future-owner transport.
    use super::*;
    #[derive(Debug)]
    pub(super) struct Carried<'a> {
        pub owner: EntityId,
        pub extra: Option<EntityId>,
        pub block: BlockId,
        pub blocks: [BlockId; 4],
        pub end: &'a Instruction,
    }
    fn via(g: &Graph<'_>, edge: &Edge, input: &[EntityId]) -> Result<Vec<EntityId>, Failure> {
        need(
            edge.arguments.len() == input.len()
                && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                    == input.iter().copied().collect(),
            "update complete provider edge contains exactly all actual carried roles",
            (edge, input),
        )?;
        input.iter().map(|&e| g.transported(edge, e)).collect()
    }
    /// The exhaustion edge: the refinement layer drops roles dead after the loop (the cursor,
    /// a carried flag), the per-family path carries every role (SPEC-0394). Owner, source and
    /// the extra future owner stay mandatory.
    fn via_exhausted(
        g: &Graph<'_>,
        edge: &Edge,
        input: &[EntityId],
        optional: &[usize],
    ) -> Result<Vec<Option<EntityId>>, Failure> {
        let carried = edge.arguments.iter().copied().collect::<BTreeSet<_>>();
        need(
            edge.arguments.len() == carried.len()
                && carried.iter().all(|e| input.contains(e))
                && input
                    .iter()
                    .enumerate()
                    .all(|(index, e)| optional.contains(&index) || carried.contains(e)),
            "update exhaustion edge carries every live role",
            (edge, input),
        )?;
        input
            .iter()
            .map(|&e| {
                if carried.contains(&e) {
                    g.transported(edge, e).map(Some)
                } else {
                    Ok(None)
                }
            })
            .collect()
    }
    pub(super) fn check<'a>(
        g: &Graph<'a>,
        module: &Module,
        p: &ParsedFile,
        i: &Input,
        (owner, extra, flag_in_provider): (EntityId, Option<EntityId>, bool),
        uses: &mut Uses,
    ) -> Result<Carried<'a>, Failure> {
        let site = super::super::ast::site(p, i)?;
        let at = |predicate: fn(&Operation) -> bool| {
            one(
                g.instructions.iter().copied().filter(|op| {
                    op.origin == Origin::Source(site.span) && predicate(&op.operation)
                }),
                "update exact original provider instruction",
            )
        };
        let root = at(|op| matches!(op, Operation::RootPlace { .. }))?;
        let begin = at(|op| matches!(op, Operation::BorrowBegin { .. }))?;
        let first = at(|op| matches!(op, Operation::MapIterationFirst { .. }))?;
        let has = at(|op| matches!(op, Operation::MapIterationHasNext { .. }))?;
        let next = at(|op| matches!(op, Operation::MapIterationNext { .. }))?;
        let EntityId::Value(seed) = owner else {
            return Err(failure("update actual provider source owner Value", owner));
        };
        let EntityId::Place(place) = result(root)? else {
            return Err(failure("update provider RootPlace result", root));
        };
        let lease = loan(begin)?;
        need(
            root.operation == Operation::RootPlace { owner: seed }
                && begin.operation
                    == Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    }
                && first.operation == Operation::MapIterationFirst { source: lease }
                && root.block == first.block
                && begin.block == first.block
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(first)?,
            "update exact current Root/Shared/First sequence",
            (root, begin, first),
        )?;
        let map = g.function.entity(owner).unwrap().ty.semantic_type();
        let (_, key, val) = module
            .map_container(map)
            .ok_or_else(|| failure("update actual provider map storage", map))?;
        let mut initial = vec![owner, EntityId::Loan(lease), result(first)?];
        let extra_index = if let Some(extra) = extra {
            initial.push(extra);
            Some(initial.len() - 1)
        } else {
            None
        };
        if flag_in_provider {
            need(
                i.flag.is_some(),
                "update carried flag has the original Boolean formal",
                i,
            )?;
            let original = *one(
                g.block(g.function.entry_block().unwrap())?
                    .parameters
                    .iter(),
                "update provider original flag parameter",
            )?;
            let carried = g
                .block(first.block)?
                .parameters
                .iter()
                .copied()
                .filter(|&e| g.anchored(e, &[original]).is_ok())
                .collect::<Vec<_>>();
            match carried.as_slice() {
                [current] => initial.push(*current),
                // The refinement layer stops carrying the Borrow flag after its last use, the
                // `if` test: no provider block may still name it (SPEC-0394).
                [] => need(
                    g.blocks.iter().all(|&block| {
                        g.block(block).is_ok_and(|data| {
                            data.parameters
                                .iter()
                                .all(|&e| e == original || g.anchored(e, &[original]).is_err())
                        })
                    }) && !g.instructions.iter().any(|instruction| {
                        instruction.block != g.function.entry_block().unwrap()
                            && instruction.operation.entities().contains(&original)
                    }),
                    "update Borrow flag flow ends at its last use before the provider",
                    original,
                )?,
                _ => {
                    return Err(failure(
                        "update actual current flag carried into provider",
                        &carried,
                    ));
                }
            }
        }
        let entry = g.branch(first.block)?;
        let header = via(g, entry, &initial)?;
        let [
            EntityId::Value(_),
            EntityId::Loan(hs),
            EntityId::Value(hc),
            ..,
        ] = header.as_slice()
        else {
            return Err(failure("update actual header roles", &header));
        };
        need(
            has.block == entry.target
                && has.operation
                    == Operation::MapIterationHasNext {
                        source: *hs,
                        cursor: *hc,
                    }
                && g.ops(has.block)?.len() == 1,
            "update exact current header source/cursor",
            has,
        )?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &g.term(has.block)?.kind
        else {
            return Err(failure("update actual HasNext guard", has));
        };
        need(
            *condition == value(has)?
                && g.term(has.block)?.origin == Origin::Source(site.span)
                && !g.instructions.iter().any(|op| {
                    op.operation
                        .entities()
                        .contains(&EntityId::Value(*condition))
                })
                && g.blocks.iter().all(|&b| {
                    b == has.block
                        || !g
                            .term(b)
                            .unwrap()
                            .kind
                            .entities()
                            .contains(&EntityId::Value(*condition))
                })
                && !when_true.arguments.contains(&EntityId::Value(*condition))
                && !when_false.arguments.contains(&EntityId::Value(*condition)),
            "update HasNext Boolean has only its real guard use",
            has,
        )?;
        let current = via(g, when_true, &header)?;
        let flag_index = (flag_in_provider
            && initial.len() > 3 + usize::from(extra_index.is_some()))
        .then(|| initial.len() - 1);
        let optional = std::iter::once(2).chain(flag_index).collect::<Vec<_>>();
        let exhausted = via_exhausted(g, when_false, &header, &optional)?;
        let mandatory = |index: usize| exhausted[index].expect("mandatory exhaustion role");
        let [_, EntityId::Loan(bs), EntityId::Value(bc), ..] = current.as_slice() else {
            return Err(failure("update actual body source/cursor roles", &current));
        };
        let reader = function(
            module,
            i.reader
                .ok_or_else(|| failure("update actual For reader AST", i))?,
        )?;
        let checked = body::check(
            g,
            reader,
            body::Body {
                block: when_true.target,
                span: site.span,
                read: span(p, site.read),
                arguments: site.arguments.map(|e| span(p, e)),
                types: [key, val],
            },
            *bs,
            *bc,
            uses,
        )?;
        need(
            checked.id == next.id,
            "update actual full body reader/loan cleanup/Next",
            checked,
        )?;
        let mut back_input = current.clone();
        back_input[2] = result(next)?;
        let back = g.branch(next.block)?;
        need(
            back.target == has.block && via(g, back, &back_input)? == header,
            "update complete current owner/source/next/flag/future-owner backedge",
            back,
        )?;
        g.incoming_from(has.block, &[first.block, next.block])?;
        g.incoming_from(next.block, &[has.block])?;
        g.incoming_from(when_false.target, &[has.block])?;
        let EntityId::Loan(es) = mandatory(1) else {
            unreachable!()
        };
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::BorrowEnd { loan: es }),
            "update unique actual current provider EndSource",
        )?;
        need(
            end.block == when_false.target
                && end.origin == Origin::Source(site.span)
                && g.ops(end.block)?.first().map(|op| op.id) == Some(end.id),
            "update current source lease ends first at real exhaustion",
            end,
        )?;
        let source_entities = [initial[1], header[1], current[1], mandatory(1)]
            .into_iter()
            .collect::<BTreeSet<_>>();
        need(g.instructions.iter().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if source_entities.contains(&EntityId::Loan(loan)))).count()==1,"update no stale pre/header/body source loan End",&source_entities)?;
        let all = |roles: &[EntityId]| roles.iter().copied().map(Some).collect::<Vec<_>>();
        for roles in [
            all(&initial),
            all(&header),
            all(&current),
            exhausted.clone(),
        ] {
            let (Some(owner_role), Some(source_role)) = (roles[0], roles[1]) else {
                unreachable!()
            };
            g.anchored(owner_role, &[owner])?;
            g.anchored(source_role, &[initial[1]])?;
            if let Some(cursor) = roles[2] {
                g.anchored(cursor, &[result(first)?, result(next)?])?;
            }
            need(
                g.function.entity(owner_role).unwrap().ty == EntityType::Value(map)
                    && g.function.entity(source_role).unwrap().ty
                        == EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: map,
                        }
                    && roles[2].is_none_or(|cursor| matches!(g.function.entity(cursor).unwrap().ty,EntityType::Value(t)if module.type_kind(t)==Some(&SsaTypeKind::Integer{bits:64,signed:false}))),
                "update exact represented owner/source/cursor types",
                &roles,
            )?;
            for (index, entity) in roles.iter().enumerate() {
                let Some(entity) = *entity else { continue };
                uses.protect(entity);
                if index >= 3 {
                    g.anchored(entity, &[initial[index]])?;
                }
            }
        }
        uses.permit(owner, root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        for op in [first, has, end] {
            for entity in op.operation.entities() {
                uses.permit(entity, op);
            }
        }
        Ok(Carried {
            owner: mandatory(0),
            extra: extra_index.map(mandatory),
            block: when_false.target,
            blocks: [first.block, has.block, next.block, when_false.target],
            end,
        })
    }
}
use super::super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
    query_map_normal_body as body,
};
use super::{
    Case,
    ast::{Input, span},
    facts::Proof,
};
use crate::ssa::model::*;
use lang_frontend::{parser::ParsedFile, source::Span};
use std::collections::BTreeSet;
pub(super) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    proof: &Proof,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "update For actual module")?;
    let scan = function(module, i.scan)?;
    let g = Graph::new(scan)?;
    diagnostic!(
        "SCOPE C UPDATE PROVIDER SSA label={} function={scan:#?} edges={:#?}",
        c.label,
        g.edges
    );
    let mut uses = Uses::default();
    let mut calls = Vec::new();
    let mut anchors = Vec::new();
    for b in &i.births {
        let factory = function(module, b.factory)?;
        let call = one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, b.expression))
                    && op.operation
                        == Operation::DirectCall {
                            callee: factory.id,
                            receiver: None,
                            arguments: vec![],
                        }
            }),
            "update For exact selected epoch Call",
        )?;
        let owner = result(call)?;
        need(
            factory.return_types == [scan.entity(owner).unwrap().ty.semantic_type()]
                && matches!(owner, EntityId::Value(_)),
            "update For actual owned Call ABI",
            call,
        )?;
        calls.push(call);
        anchors.push(owner);
    }
    let site = super::ast::site(p, i)?;
    let provider_root = one(
        g.instructions.iter().copied().filter(|op| {
            op.origin == Origin::Source(site.span)
                && matches!(op.operation, Operation::RootPlace { .. })
        }),
        "update actual provider RootPlace",
    )?;
    let Operation::RootPlace { owner: seed } = provider_root.operation else {
        unreachable!()
    };
    let seed = EntityId::Value(seed);
    let put = if let Some(expr) = i.put {
        super::graph::put_clones(&g, p, expr)?;
        Some(one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, expr))
                    && matches!(op.operation, Operation::MapPut { .. })
            }),
            "update original conditional put SSA",
        )?)
    } else {
        None
    };
    if let Some(put) = put {
        anchors.push(result(put)?);
    }
    let before = if matches!(c.code, 7 | 9) {
        Some(conditional::check(
            &g,
            p,
            i,
            (
                anchors[0],
                if c.code == 9 {
                    result(put.unwrap())?
                } else {
                    anchors[1]
                },
            ),
            proof.old_origin,
            &mut uses,
        )?)
    } else {
        None
    };
    let expected = if let Some(before) = &before {
        before.owner
    } else if c.code == 8 {
        anchors[0]
    } else {
        anchors[1]
    };
    need(
        seed == expected,
        "update provider borrows exact current source epoch rather than another Map",
        (seed, expected),
    )?;
    if i.flag.is_some() && !proof.flag_in_provider {
        let prior = before
            .as_ref()
            .ok_or_else(|| failure("update exact entry-only prior If flag proof", i))?;
        need(
            prior.blocks[0] == scan.entry_block().unwrap() && prior.block == provider_root.block,
            "update entry flag If dominates the provider through both complete original arms",
            prior.block,
        )?;
    }
    let extra = if c.code == 11 { Some(anchors[0]) } else { None };
    let carried = provider::check(
        &g,
        module,
        p,
        i,
        (seed, extra, proof.flag_in_provider),
        &mut uses,
    )?;
    let after = if c.code == 8 {
        Some(conditional::check(
            &g,
            p,
            i,
            (carried.owner, anchors[1]),
            proof.old_origin,
            &mut uses,
        )?)
    } else {
        None
    };
    let final_owner = if let Some(after) = &after {
        after.owner
    } else if c.code == 11 {
        carried
            .extra
            .ok_or_else(|| failure("update independently carried future next owner", &carried))?
    } else {
        carried.owner
    };
    let final_block = after.as_ref().map_or(carried.block, |q| q.block);
    let observer = function(module, i.observer)?;
    let (call, arg) = *one(i.observes.iter(), "update For exact final observe AST")?;
    let observed = one(
        g.instructions.iter().copied().filter(|op| {
            op.origin == Origin::Source(span(p, call))
                && matches!(op.operation,Operation::DirectCall{callee,..}if callee==observer.id)
        }),
        "update For final actual observe Call",
    )?;
    let Operation::DirectCall {
        receiver: None,
        arguments,
        ..
    } = &observed.operation
    else {
        return Err(failure("update For actual observe ABI", observed));
    };
    let [EntityId::Loan(observe_loan)] = arguments.as_slice() else {
        return Err(failure(
            "update For actual current Map observe loan",
            observed,
        ));
    };
    let begin = g.definition(EntityId::Loan(*observe_loan))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("update For actual final Shared loan birth", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let EntityId::Value(final_value) = final_owner else {
        return Err(failure("update For final owner Value", final_owner));
    };
    need(
        root.operation == Operation::RootPlace { owner: final_value }
            && [root, begin, observed]
                .iter()
                .all(|op| op.block == final_block)
            && root.origin == Origin::Source(span(p, arg))
            && begin.origin == Origin::Source(span(p, arg))
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(observed)?,
        "update final observe borrows exact post-provider/assignment current owner",
        (root, begin, observed),
    )?;
    let end = one(
        g.instructions.iter().copied().filter(|op| {
            op.operation
                == Operation::BorrowEnd {
                    loan: *observe_loan,
                }
        }),
        "update exact final observe End",
    )?;
    need(
        end.block == final_block
            && end.origin == Origin::Source(span(p, call))
            && g.position(end)? == g.position(observed)? + 1,
        "update final actual CallReturn loan cleanup",
        end,
    )?;
    uses.permit(final_owner, root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.protect(EntityId::Loan(*observe_loan));
    uses.permit(EntityId::Loan(*observe_loan), observed);
    uses.permit(EntityId::Loan(*observe_loan), end);
    let final_drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.operation == Operation::Drop { owner: final_value }),
        "update exact unique final current owner Drop",
    )?;
    need(
        final_drop.block == final_block
            && final_drop.origin == Origin::Source(proof.final_origin)
            && g.position(final_drop)? == g.position(end)? + 1
            && g.ops(final_block)?.last().map(|op| op.id) == Some(final_drop.id)
            && matches!(&g.term(final_block)?.kind,TerminatorKind::Return{values}if values.is_empty()),
        "update exact public final owner Drop after loan End before normal Return",
        final_drop,
    )?;
    uses.permit(final_owner, final_drop);
    let old_drop = if let Some(q) = before.as_ref().or(after.as_ref()) {
        q.old_drop
    } else if c.code == 10 {
        let EntityId::Value(old) = anchors[0] else {
            unreachable!()
        };
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::Drop { owner: old }),
            "update moved old declaration epoch unique Drop",
        )?;
        need(
            drop.origin == Origin::Source(proof.old_origin.unwrap())
                && drop.block == calls[0].block
                && drop.block == calls[1].block
                && g.position(calls[0])? < g.position(drop)?
                && g.position(drop)? < g.position(calls[1])?,
            "update P10 old declaration AfterStatement Drop precedes the second make",
            drop,
        )?;
        uses.permit(anchors[0], drop);
        Some(drop)
    } else if c.code == 11 {
        let EntityId::Value(old) = carried.owner else {
            unreachable!()
        };
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::Drop { owner: old }),
            "update actual old source LoopExit Drop",
        )?;
        need(
            drop.block == carried.block
                && drop.origin == Origin::Source(proof.old_origin.unwrap())
                && g.position(drop)? == g.position(carried.end)? + 1
                && g.position(drop)? < g.position(root)?,
            "update P11 old source End/Drop precedes exact future Name owner observe",
            drop,
        )?;
        uses.permit(carried.owner, drop);
        Some(drop)
    } else {
        None
    };
    let map_drops=g.instructions.iter().filter(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).collect::<Vec<_>>();
    need(
        map_drops.len() == 1 + usize::from(old_drop.is_some())
            && map_drops
                .iter()
                .all(|op| op.id == final_drop.id || old_drop.is_some_and(|old| old.id == op.id)),
        "update no extra or moved-name duplicate Map Drop sites",
        &map_drops,
    )?;
    if let Some(put) = put {
        let Operation::MapPut { owner, .. } = put.operation else {
            unreachable!()
        };
        uses.permit(EntityId::Value(owner), put);
    }
    let mut blocks = BTreeSet::from([scan.entry_block().unwrap()]);
    blocks.extend(carried.blocks);
    if let Some(q) = before.as_ref().or(after.as_ref()) {
        blocks.extend(q.blocks);
    }
    need(
        blocks == g.blocks.iter().copied().collect(),
        "update complete original conditional/provider CFG, no hidden path",
        &g.blocks,
    )?;
    for &block in &g.blocks {
        for entity in g.block(block)?.parameters.iter().copied().chain(
            g.ops(block)?
                .into_iter()
                .flat_map(|op| op.results.iter().copied()),
        ) {
            if matches!(scan.entity(entity).unwrap().ty,EntityType::Value(t)if module.map_container(t).is_some())
            {
                g.anchored(entity, &anchors)?;
                uses.protect(entity);
            }
        }
    }
    crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
        &g,
        module,
        &anchors.iter().copied().collect(),
    )?;
    uses.check(&g, None)
}
