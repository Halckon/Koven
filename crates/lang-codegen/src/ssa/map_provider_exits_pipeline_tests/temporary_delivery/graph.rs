//! The selected ordinary Call delivers real argument owners before its returned Map is used.
mod arguments {
    //! Source-order evaluation and declared-slot ABI preserve Borrow versus Value delivery.
    use super::*;
    use lang_frontend::type_checking::ParameterMode;

    pub(super) struct Delivery<'ir> {
        pub call: &'ir Instruction,
        pub cleanup: Vec<&'ir Instruction>,
        pub argument_owners: BTreeSet<ValueId>,
        pub map_capabilities: BTreeSet<EntityId>,
        pub map_types: Vec<SsaTypeId>,
    }

    pub(super) fn check<'ir>(
        g: &Graph<'ir>,
        module: &Module,
        factory: &Function,
        parsed: &ParsedFile,
        input: &ast::Input,
        uses: &mut Uses,
    ) -> Result<Delivery<'ir>, Failure> {
        let call = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation,Operation::DirectCall {callee,..} if callee==factory.id),
            ),
            "delivery selected original factory called exactly once",
        )?;
        let Operation::DirectCall {
            receiver: None,
            ref arguments,
            ..
        } = call.operation
        else {
            return Err(failure("delivery direct factory ABI", call));
        };
        let formals = &factory
            .block(factory.entry_block().unwrap())
            .unwrap()
            .parameters;
        need(
            call.origin == Origin::Source(ast::expression(parsed, input.call))
                && arguments.len() == input.arguments.len()
                && formals.len() == arguments.len(),
            "delivery actual Callspan and complete declared ABI",
            call,
        )?;
        let mut cleanup = Vec::new();
        let mut argument_owners = BTreeSet::new();
        let mut map_capabilities = BTreeSet::new();
        let mut map_types = Vec::new();
        let mut previous = None;
        for actual in &input.arguments {
            let evaluator = function(module, actual.function.span)?;
            let evaluate = one(
                g.instructions.iter().copied().filter(
                    |i| matches!(i.operation,Operation::DirectCall {callee,..} if callee==evaluator.id),
                ),
                "delivery actual argument evaluator occurs once",
            )?;
            let owner = value(evaluate)?;
            let owner_entity = EntityId::Value(owner);
            let ty = g.function.entity(owner_entity).unwrap().ty.semantic_type();
            need(
                evaluate.origin == Origin::Source(ast::expression(parsed, actual.expression))
                    && matches!(&evaluate.operation,Operation::DirectCall {receiver:None,arguments,..} if arguments.is_empty())
                    && evaluator.return_types.as_slice() == [ty]
                    && evaluator
                        .block(evaluator.entry_block().unwrap())
                        .unwrap()
                        .parameters
                        .is_empty()
                    && evaluate.block == call.block
                    && g.position(evaluate)? < g.position(call)?
                    && previous.is_none_or(|position| position < g.position(evaluate).unwrap())
                    && argument_owners.insert(owner),
                "delivery real argument evaluation once in source order",
                evaluate,
            )?;
            previous = Some(g.position(evaluate)?);
            uses.protect(owner_entity);
            if module.map_container(ty).is_some() {
                map_types.push(ty);
                map_capabilities.insert(owner_entity);
            }
            let mode = input.factory.parameters[actual.parameter].mode;
            if mode == ParameterMode::Value {
                need(
                    arguments[actual.parameter] == owner_entity
                        && factory.entity(formals[actual.parameter]).unwrap().ty
                            == EntityType::Value(ty)
                        && matches!(module.type_kind(ty), Some(SsaTypeKind::HeapOwner { .. })),
                    "delivery own Resource goes to its actual declared Value slot",
                    (evaluate, call, actual.parameter),
                )?;
                uses.permit(owner_entity, call);
            } else {
                let EntityId::Loan(current) = arguments[actual.parameter] else {
                    return Err(failure("delivery actual Borrow argument is loan", call));
                };
                let begin = g.definition(EntityId::Loan(current))?;
                let Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                } = begin.operation
                else {
                    return Err(failure("delivery actual shared argument loan", begin));
                };
                let root = g.definition(EntityId::Place(place))?;
                let end = one(
                    g.instructions
                        .iter()
                        .copied()
                        .filter(|i| i.operation == Operation::BorrowEnd { loan: current }),
                    "delivery actual synchronous argument End",
                )?;
                let drop = one(
                    g.instructions
                        .iter()
                        .copied()
                        .filter(|i| i.operation == Operation::Drop { owner }),
                    "delivery borrowed temporary unique original Drop",
                )?;
                let formal = EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                };
                need(
                    root.operation == Operation::RootPlace { owner }
                        && root.origin == Origin::Source(actual.span)
                        && begin.origin == evaluate.origin
                        && drop.origin == evaluate.origin
                        && end.origin == call.origin
                        && [root, begin, end, drop]
                            .iter()
                            .all(|i| i.block == call.block)
                        && g.position(evaluate)? < g.position(root)?
                        && g.position(root)? < g.position(begin)?
                        && g.position(begin)? < g.position(call)?
                        && g.position(call)? < g.position(end)?
                        && g.position(end)? < g.position(drop)?
                        && g.function.entity(EntityId::Place(place)).unwrap().ty
                            == EntityType::Place(ty)
                        && g.function.entity(EntityId::Loan(current)).unwrap().ty == formal
                        && factory.entity(formals[actual.parameter]).unwrap().ty == formal,
                    "delivery evaluator owner/Root/SharedBorrow/correct ABI/End/CallReturn Drop",
                    (evaluate, root, begin, call, end, drop),
                )?;
                uses.permit(owner_entity, root);
                uses.permit(owner_entity, drop);
                let capabilities = [
                    owner_entity,
                    EntityId::Place(place),
                    EntityId::Loan(current),
                ];
                need(
                    g.edges.iter().all(|(_, edge)| {
                        edge.arguments
                            .iter()
                            .all(|entity| !capabilities.contains(entity))
                    }),
                    "Unit synchronous argument capabilities never escape after CallReturn retirement",
                    capabilities,
                )?;
                if module.map_container(ty).is_some() {
                    map_capabilities.extend(capabilities);
                }
                uses.protect(EntityId::Place(place));
                uses.permit(EntityId::Place(place), begin);
                uses.protect(EntityId::Loan(current));
                uses.permit(EntityId::Loan(current), call);
                uses.permit(EntityId::Loan(current), end);
                cleanup.push(drop);
            }
        }
        Ok(Delivery {
            call,
            cleanup,
            argument_owners,
            map_capabilities,
            map_types,
        })
    }
}
mod control {
    //! No-For controls keep their real returned owner through size and lexical cleanup.
    use super::*;

    pub(super) fn check(
        case: &Case,
        g: &Graph<'_>,
        module: &Module,
        parsed: &ParsedFile,
        input: &ast::Input,
        selected: (&arguments::Delivery<'_>, &Instruction),
        uses: &mut Uses,
    ) -> Result<(), Failure> {
        let (delivered, after) = selected;
        let owner = value(delivered.call)?;
        let anchor = EntityId::Value(owner);
        let size = at(g, ast::expression(parsed, input.size.unwrap()), |o| {
            matches!(o, Operation::MapSize { .. })
        })?;
        need(
            size.operation == Operation::MapSize { owner: anchor }
                && size.block == delivered.call.block
                && g.position(delivered.call)? < g.position(size)?
                && delivered.cleanup.iter().all(|i| {
                    i.block == size.block && g.position(i).unwrap() < g.position(size).unwrap()
                }),
            "delivery control actual output size after synchronous argument cleanup",
            size,
        )?;
        let map = g.function.entity(anchor).unwrap().ty.semantic_type();
        let mut family = BTreeSet::from([anchor]);
        loop {
            let before = family.len();
            for (_, edge) in &g.edges {
                for (&argument, &parameter) in
                    edge.arguments.iter().zip(&g.block(edge.target)?.parameters)
                {
                    if family.contains(&argument) {
                        family.insert(parameter);
                    }
                }
            }
            if before == family.len() {
                break;
            }
        }
        for &entity in &family {
            anchored(g, entity, &[anchor])?;
            uses.protect(entity);
        }
        uses.permit(anchor, size);
        let drop=one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop {owner} if family.contains(&EntityId::Value(owner)))),
            "delivery control unique current output Drop")?;
        let Operation::Drop { owner: current } = drop.operation else {
            unreachable!()
        };
        need(
            drop.origin == Origin::Source(input.control_root.unwrap())
                && !delivered.argument_owners.contains(&current),
            "delivery control lexical binding origin and independent output owner",
            drop,
        )?;
        uses.permit(EntityId::Value(current), drop);
        if matches!(case.delivery, Delivery::SameMapBorrow) {
            need(
                drop.block == size.block && g.position(drop)? == g.position(size)? + 1,
                "delivery Pure control drops immediately after last actual size",
                drop,
            )?;
        } else {
            need(
                drop.block == after.block && g.position(after)? < g.position(drop)?,
                "delivery Resource control retains output until after marker",
                (after, drop),
            )?;
            if drop.block != delivered.call.block {
                let actual = one(
                    g.block(drop.block)?
                        .parameters
                        .iter()
                        .copied()
                        .filter(|e| family.contains(e)),
                    "delivery lexical cleanup actual carried owner column",
                )?;
                need(
                    actual == EntityId::Value(current),
                    "delivery lexical Drop consumes terminal current owner, not stale ancestor",
                    (actual, drop),
                )?;
            }
        }
        need(
            g.blocks
                .iter()
                .filter(|&&b| matches!(g.term(b).unwrap().kind, TerminatorKind::Return { .. }))
                .count()
                == 1
                && !g.instructions.iter().any(|i| {
                    matches!(
                        i.operation,
                        Operation::MapIterationFirst { .. }
                            | Operation::MapIterationHasNext { .. }
                            | Operation::MapIterationNext { .. }
                            | Operation::MapIterationKeyLoan { .. }
                            | Operation::MapIterationValueLoan { .. }
                    )
                }),
            "delivery no-For has one normal Return and no provider operations",
            g.function.id,
        )?;
        // Rebuild the old current-owner/availability obligation from actual Unit
        // values on every path and every edge, never from an undirected alias set.
        let mut proven = BTreeSet::new();
        let mut pending = vec![(
            g.function.entry_block().unwrap(),
            None,
            0usize,
            BTreeSet::new(),
        )];
        while let Some((block, mut current, mut count, mut visited)) = pending.pop() {
            need(
                visited.insert(block),
                "delivery control graph is acyclic",
                block,
            )?;
            for instruction in g.ops(block)? {
                for entity in instruction.operation.entities() {
                    if family.contains(&entity) {
                        need(
                            current == Some(entity),
                            "Unit control operand is the actual live current returned owner",
                            (instruction, current),
                        )?;
                    }
                }
                if instruction.id == delivered.call.id {
                    need(
                        current.is_none(),
                        "Unit control unique owner birth",
                        instruction,
                    )?;
                    current = Some(anchor);
                    proven.insert(anchor);
                }
                if instruction.id == drop.id {
                    need(
                        matches!(drop.operation, Operation::Drop { owner } if current == Some(EntityId::Value(owner))),
                        "Unit control Drop consumes its exact current owner",
                        (instruction, current),
                    )?;
                    current = None;
                    count += 1;
                }
            }
            match &g.term(block)?.kind {
                TerminatorKind::Return { values } => need(
                    count == 1 && current.is_none() && values.is_empty(),
                    "Unit control every normal Return retires the original output once",
                    (block, count, current),
                )?,
                TerminatorKind::Abort => {}
                _ => {
                    for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
                        let carried = edge
                            .arguments
                            .iter()
                            .copied()
                            .filter(|entity| family.contains(entity))
                            .collect::<Vec<_>>();
                        need(
                            carried == current.into_iter().collect::<Vec<_>>(),
                            "Unit control edge carries only the live current owner, never a retired ancestor",
                            (edge, current, &carried),
                        )?;
                        let next = current
                            .map(|entity| g.transported(edge, entity))
                            .transpose()?;
                        proven.extend(next);
                        pending.push((edge.target, next, count, visited.clone()));
                    }
                }
            }
        }
        need(
            proven == family,
            "Unit control entire owner family is directionally proven",
            (&proven, &family),
        )?;
        need(
            family
                .iter()
                .all(|e| g.function.entity(*e).unwrap().ty == EntityType::Value(map)),
            "delivery output family preserves actual map representation",
            &family,
        )?;
        // Any other owned Map in this caller must be the borrowed argument's actual
        // evaluator result, which has its separate CallReturn cleanup proof.
        for (index, data) in g.function.values.iter().enumerate() {
            if module.map_container(data.ty.semantic_type()).is_some() {
                need(
                    family
                        .iter()
                        .any(|e| matches!(e,EntityId::Value(v) if v.index()==index))
                        || delivered.argument_owners.iter().any(|v| v.index() == index),
                    "delivery no third or swapped owning Map family",
                    (index, data),
                )?;
            }
        }
        need(
            proven.is_disjoint(&delivered.map_capabilities),
            "Unit control returned owner is separate from every synchronous Map argument capability",
            (&proven, &delivered.map_capabilities),
        )?;
        proven.extend(delivered.map_capabilities.iter().copied());
        let types = std::iter::once(map)
            .chain(delivered.map_types.iter().copied())
            .collect::<Vec<_>>();
        crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(g, &types, &proven)
    }
}
#[allow(unused_imports)] pub(in crate::ssa) use crate::ssa::lower_frontend_tests::owned_source_return_tests::native::factory as factory;
mod provider {
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
        let mut expected = BTreeSet::from([EntityId::Place(place)]);
        for frame in [initial, header, current, exit] {
            need(
                expected.insert(EntityId::Value(frame.owner))
                    && expected.insert(EntityId::Loan(frame.source)),
                "Unit delivered Map has distinct current owner and source roles",
                frame,
            )?;
        }
        need(
            expected.is_disjoint(&delivered.map_capabilities),
            "Unit returned Map never aliases the independently evaluated Map argument",
            (&expected, &delivered.map_capabilities),
        )?;
        expected.extend(delivered.map_capabilities.iter().copied());
        let types = std::iter::once(map)
            .chain(delivered.map_types.iter().copied())
            .collect::<Vec<_>>();
        crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(g, &types, &expected)
    }
}
use super::{Case, Delivery, ast};
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
        "delivery actual original SSA operation",
    )
}

/// Reuse the shared read-only complete-incoming proof for all delivery roles.
fn anchored(g: &Graph<'_>, current: EntityId, anchors: &[EntityId]) -> Result<(), Failure> {
    g.anchored(current, anchors)
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
            g.definition(EntityId::Value(owner)).is_ok_and(
                |i| matches!(&i.operation,Operation::StringLiteral {bytes,..} if bytes==b"after"),
            )
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
    let input = ast::input(case, sources, parsed, control)?;
    let module = one(program.modules.iter(), "delivery original module")?;
    let entry = function(module, input.entry.span)?;
    let factory = function(module, input.factory.span)?;
    let g = Graph::with_abort(entry)?;
    need(
        g.block(entry.entry_block().unwrap())?.parameters.is_empty(),
        "delivery entry has no parameter substitutes",
        entry.id,
    )?;
    let mut uses = Uses::default();
    let delivered = arguments::check(&g, module, factory, parsed, &input, &mut uses)?;
    let owner = value(delivered.call)?;
    let map = entry
        .entity(EntityId::Value(owner))
        .unwrap()
        .ty
        .semantic_type();
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("delivery returned owning Map", map))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
            && if matches!(case.delivery, Delivery::SameMapBorrow) {
                module.type_kind(val) == Some(&SsaTypeKind::StringOwner)
            } else {
                matches!(module.type_kind(val), Some(SsaTypeKind::HeapOwner { .. }))
            }
            && factory.return_types.as_slice() == [map]
            && factory.receiver.is_none()
            && factory.borrow_return.is_none()
            && factory.carrier_return.is_none(),
        "delivery selected result kind and actual Pure/Resource representation",
        (factory.id, map),
    )?;
    factory::check(case, sources, parsed, module, factory, &input, map)?;
    let after = marker(&g, input.after)?;
    if let Some(site) = &input.site {
        provider::check(
            &g,
            module,
            function(module, input.reader.span)?,
            parsed,
            site,
            (&delivered, after),
            &mut uses,
        )?;
    } else {
        control::check(
            case,
            &g,
            module,
            parsed,
            &input,
            (&delivered, after),
            &mut uses,
        )?;
    }
    need(
        !delivered.argument_owners.contains(&owner),
        "delivery result cannot alias any argument owner",
        owner,
    )?;
    // Same-K/V input Map has its own synchronous Drop; count both distinct families.
    let map_drops=g.instructions.iter().filter(|i|matches!(i.operation,Operation::Drop {owner}
        if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count();
    need(
        map_drops == 1 + usize::from(matches!(case.delivery, Delivery::SameMapBorrow)),
        "delivery exact disjoint argument/output Map cleanup count",
        map_drops,
    )?;
    uses.check(&g, None)
}
