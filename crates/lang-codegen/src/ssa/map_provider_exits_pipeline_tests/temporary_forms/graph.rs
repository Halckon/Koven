//! Each actual factory Call keeps its own hidden owner through every provider edge.
mod representation;
use super::{Case, ast};
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, function, loan, result, value},
        query_inputs::one,
        query_map_normal_body as body,
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
    capabilities: BTreeSet<EntityId>,
    map: SsaTypeId,
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
            g,
            self.reader,
            body::Body {
                block: when_true.target,
                span: site.span,
                read: ast::expression(self.parsed, site.read),
                arguments: site.arguments.map(|id| ast::expression(self.parsed, id)),
                types: [key_type, value_type],
            },
            body_roles.source,
            body_roles.cursor,
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
        let mut capabilities = BTreeSet::from([EntityId::Place(place)]);
        for current in roles {
            need(
                capabilities.insert(EntityId::Value(current.owner))
                    && capabilities.insert(EntityId::Loan(current.source)),
                "Unit represented temporary has distinct current owner and lease roles",
                current,
            )?;
        }
        Ok(Shape {
            capabilities,
            map: map_type,
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
    need(
        first.capabilities.is_disjoint(&second.capabilities),
        "Unit distinct Call owners never share storage or provider capabilities",
        (&first.capabilities, &second.capabilities),
    )?;
    let expected = first
        .capabilities
        .union(&second.capabilities)
        .copied()
        .collect();
    // A set union of already directed, disjoint roles is only an inventory, never an alias proof.
    crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(
        &g,
        &[first.map, second.map],
        &expected,
    )?;
    uses.check(&g, None)
}

pub(in crate::ssa) mod structure {
//! Inspect only real-source SSA. No IR mutation or synthetic fixtures.
use crate::ssa::model::{
    BlockId, Definition, Edge, EntityId, Function, Instruction, LoanKind, Operation, Origin,
    Program, TerminatorKind,
};
use lang_frontend::source::Span;

fn edges(function: &Function) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            let edges = match &block.terminator.as_ref().unwrap().kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => vec![when_true, when_false],
                _ => Vec::new(),
            };
            edges.into_iter().map(move |edge| (block.id, edge))
        })
        .collect()
}

// The selected fixture has no intervening Map update: forwarding must terminate
// at the exact operation result, not merely at any owner of the same type.
fn forwards(function: &Function, value: EntityId, expected: EntityId, depth: usize) -> bool {
    if value == expected {
        return true;
    }
    if depth > function.blocks.len() {
        return false;
    }
    let Definition::BlockParameter { block, index } = function.entity(value).unwrap().definition
    else {
        return false;
    };
    let inputs = edges(function)
        .into_iter()
        .filter(|(_, edge)| edge.target == block)
        .collect::<Vec<_>>();
    !inputs.is_empty()
        && inputs
            .iter()
            .all(|(_, edge)| forwards(function, edge.arguments[index], expected, depth + 1))
}

pub(in crate::ssa) fn assert_exit_structure(
    program: &Program,
    cleanup_span: Span,
    source_span: Span,
    live: bool,
    unit_cleanup: bool,
) {
    let functions = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| {
            function.instructions.iter().any(|instruction| {
                instruction.origin == Origin::Source(source_span)
                    && matches!(instruction.operation, Operation::ContainerConstruct { .. })
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        functions.len(),
        1,
        "the original source selects its real function"
    );
    let function = functions[0];
    let seed = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
        .collect::<Vec<_>>();
    assert_eq!(seed.len(), 1);
    let seed = seed[0].results[0];
    let all_edges = edges(function);
    let entries = all_edges
        .iter()
        .filter_map(|(block, edge)| {
            edge.arguments
                .iter()
                .position(|arg| *arg == seed)
                .map(|slot| (*block, *edge, slot))
        })
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    let (preheader, entry, map_slot) = entries[0];
    let header = function.block(entry.target).unwrap();
    let header_owner = header.parameters[map_slot];
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("real provider header")
    };
    assert_eq!(when_true.arguments[map_slot], header_owner);
    assert_eq!(when_false.arguments[map_slot], header_owner);
    let body = function.block(when_true.target).unwrap();
    let exhausted = function.block(when_false.target).unwrap();
    let body_owner = body.parameters[map_slot];
    let exhausted_owner = exhausted.parameters[map_slot];
    let updates = function
        .instructions
        .iter()
        .filter(|i| {
            matches!(
                i.operation,
                Operation::MapPut { .. } | Operation::MapRemove { .. }
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(updates.len(), 1);
    let owner = match updates[0].operation {
        Operation::MapPut { owner, .. } | Operation::MapRemove { owner, .. } => owner,
        _ => unreachable!(),
    };
    assert!(forwards(function, EntityId::Value(owner), body_owner, 0));
    let updated = updates[0].results[0];
    let back = all_edges
        .iter()
        .filter(|(block, edge)| edge.target == header.id && *block != preheader)
        .collect::<Vec<_>>();
    assert_eq!(back.len(), 1);
    assert!(forwards(
        function,
        back[0].1.arguments[map_slot],
        updated,
        0
    ));

    let header_incoming = all_edges
        .iter()
        .filter(|(_, edge)| edge.target == header.id)
        .collect::<Vec<_>>();
    assert_eq!(
        header_incoming.len(),
        2,
        "only entry and the actual fallthrough backedge"
    );
    assert!(
        header_incoming
            .iter()
            .any(|incoming| incoming.0 == preheader && incoming.1 == entry)
    );
    assert!(
        header_incoming
            .iter()
            .any(|incoming| incoming.0 == back[0].0 && incoming.1 == back[0].1)
    );

    let source_constructions = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(source_span)
                && matches!(instruction.operation, Operation::ContainerConstruct { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(source_constructions.len(), 1);
    let source_owner = source_constructions[0].results[0];
    assert_eq!(source_constructions[0].block, preheader);
    let definition = |entity| -> &Instruction {
        let Definition::InstructionResult { instruction, .. } =
            function.entity(entity).unwrap().definition
        else {
            panic!("provider identity must have an actual producing instruction")
        };
        function.instruction(instruction).unwrap()
    };
    let lengths = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::ContainerLength { .. }))
        .collect::<Vec<_>>();
    assert_eq!(lengths.len(), 1);
    assert_eq!(lengths[0].block, preheader);
    let Operation::ContainerLength {
        owner: initial_loan,
    } = lengths[0].operation
    else {
        unreachable!()
    };
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = definition(initial_loan).operation
    else {
        panic!("provider length reads its actual shared source loan")
    };
    let Operation::RootPlace { owner } = definition(EntityId::Place(place)).operation else {
        panic!("provider loan must name the constructed source owner")
    };
    assert_eq!(EntityId::Value(owner), source_owner);
    let loan_slots = entry
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(slot, entity)| (*entity == initial_loan).then_some(slot))
        .collect::<Vec<_>>();
    assert_eq!(loan_slots.len(), 1);
    let loan_slot = loan_slots[0];
    let header_loan = header.parameters[loan_slot];
    assert_eq!(when_true.arguments[loan_slot], header_loan);
    assert_eq!(when_false.arguments[loan_slot], header_loan);
    let body_loan = body.parameters[loan_slot];
    let exhausted_loan = exhausted.parameters[loan_slot];
    assert!(forwards(
        function,
        back[0].1.arguments[loan_slot],
        body_loan,
        0
    ));
    let source_slots = entry
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(slot, entity)| (*entity == source_owner).then_some(slot))
        .collect::<Vec<_>>();
    assert_eq!(
        source_slots.len(),
        1,
        "one exact hidden source-owner column"
    );
    let source_slot = source_slots[0];
    let header_source = header.parameters[source_slot];
    assert_eq!(when_true.arguments[source_slot], header_source);
    assert_eq!(when_false.arguments[source_slot], header_source);
    let body_source = body.parameters[source_slot];
    let exhausted_source = exhausted.parameters[source_slot];
    assert!(forwards(
        function,
        back[0].1.arguments[source_slot],
        body_source,
        0
    ));

    let source_drops = function
        .instructions
        .iter()
        .filter(|i| {
            i.origin == Origin::Source(source_span) && matches!(i.operation, Operation::Drop { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        source_drops.len(),
        2,
        "temporary source is consumed on each actual exit"
    );
    // Single and Unit cleanup use different source origins. Select the actual
    // break block by its source-owner drop, distinct from the provider false edge.
    let break_source_drops = source_drops
        .iter()
        .filter(|drop| drop.block != exhausted.id)
        .collect::<Vec<_>>();
    assert_eq!(break_source_drops.len(), 1);
    let break_block = function.block(break_source_drops[0].block).unwrap();
    let ends = function
        .instructions
        .iter()
        .filter(|i| {
            (!unit_cleanup || i.block == break_block.id)
                && i.origin == Origin::Source(cleanup_span)
                && matches!(i.operation, Operation::BorrowEnd { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), 2, "break ends element then provider source");
    assert_eq!(ends[0].block, ends[1].block);
    assert_eq!(ends[0].block, break_block.id);
    let element_place = function
        .instructions
        .iter()
        .find(|i| matches!(i.operation, Operation::ContainerElementPlace { .. }))
        .unwrap();
    let Operation::ContainerElementPlace {
        owner: source_loan, ..
    } = element_place.operation
    else {
        unreachable!()
    };
    assert_eq!(source_loan, body_loan);
    let element_loan = function.instructions.iter().find(|i| matches!(i.operation, Operation::BorrowBegin { place, .. } if EntityId::Place(place) == element_place.results[0])).unwrap().results[0];
    for (end, expected) in [(ends[0], element_loan), (ends[1], source_loan)] {
        let Operation::BorrowEnd { loan } = end.operation else {
            unreachable!()
        };
        assert!(forwards(function, EntityId::Loan(loan), expected, 0));
    }
    let position = |block: BlockId, id| {
        function
            .block(block)
            .unwrap()
            .instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    assert!(position(break_block.id, ends[0].id) < position(break_block.id, ends[1].id));

    for block in [break_block, exhausted] {
        let source_drop = source_drops
            .iter()
            .find(|drop| drop.block == block.id)
            .unwrap();
        let source_ends = block
            .instructions
            .iter()
            .filter_map(|id| function.instruction(*id))
            .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
            .collect::<Vec<_>>();
        assert_eq!(
            source_ends.len(),
            if block.id == break_block.id { 2 } else { 1 }
        );
        assert!(
            source_ends
                .iter()
                .all(|end| position(block.id, end.id) < position(block.id, source_drop.id))
        );
        let expected_loan = if block.id == break_block.id {
            body_loan
        } else {
            exhausted_loan
        };
        let Operation::BorrowEnd { loan } = source_ends.last().unwrap().operation else {
            unreachable!()
        };
        assert!(forwards(function, EntityId::Loan(loan), expected_loan, 0));
    }
    let map_drops = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::Drop { .. }) && !source_drops.contains(i))
        .collect::<Vec<_>>();
    // The String literal also has a drop after clone. Select the Map by actual
    // forwarding from the corresponding exit owner, never by SsaTypeId.
    let TerminatorKind::Branch(break_edge) = &break_block.terminator.as_ref().unwrap().kind else {
        panic!("break reaches exit merge")
    };
    let TerminatorKind::Branch(exhausted_edge) = &exhausted.terminator.as_ref().unwrap().kind
    else {
        panic!("exhaustion reaches exit merge")
    };
    assert_eq!(break_edge.target, exhausted_edge.target);
    assert_ne!(break_block.id, exhausted.id);
    let joined = function.block(break_edge.target).unwrap();
    let exit_incoming = all_edges
        .iter()
        .filter(|(_, edge)| edge.target == joined.id)
        .collect::<Vec<_>>();
    assert_eq!(
        exit_incoming.len(),
        2,
        "only break and exhaustion enter the exit merge"
    );
    assert!(
        exit_incoming
            .iter()
            .any(|incoming| incoming.0 == break_block.id && incoming.1 == break_edge)
    );
    assert!(
        exit_incoming
            .iter()
            .any(|incoming| incoming.0 == exhausted.id && incoming.1 == exhausted_edge)
    );
    for (block, expected) in [(break_block, body_source), (exhausted, exhausted_source)] {
        let source_drop = source_drops
            .iter()
            .find(|drop| drop.block == block.id)
            .unwrap();
        let Operation::Drop { owner } = source_drop.operation else {
            unreachable!()
        };
        assert!(forwards(function, EntityId::Value(owner), expected, 0));
    }
    if live {
        let slots = break_edge
            .arguments
            .iter()
            .zip(&exhausted_edge.arguments)
            .enumerate()
            .filter(|(_, (a, b))| forwards(function, **a, updated, 0) && **b == exhausted_owner)
            .collect::<Vec<_>>();
        assert_eq!(slots.len(), 1, "one exact current owner from each exit");
        let merged = joined.parameters[slots[0].0];
        let final_drops = map_drops.iter().filter(|i| matches!(i.operation, Operation::Drop { owner } if EntityId::Value(owner) == merged)).collect::<Vec<_>>();
        assert_eq!(final_drops.len(), 1);
        assert_eq!(final_drops[0].block, joined.id);
        assert!(
            !map_drops
                .iter()
                .any(|i| [break_block.id, exhausted.id].contains(&i.block))
        );
    } else {
        for (block, expected) in [(break_block, updated), (exhausted, exhausted_owner)] {
            let drops = map_drops.iter().filter(|i| i.block == block.id && matches!(i.operation, Operation::Drop { owner } if forwards(function, EntityId::Value(owner), expected, 0))).collect::<Vec<_>>();
            assert_eq!(drops.len(), 1);
            let source_drop = source_drops.iter().find(|i| i.block == block.id).unwrap();
            assert!(position(block.id, source_drop.id) < position(block.id, drops[0].id));
        }
        assert!(
            joined.parameters.is_empty(),
            "dead Map must not reappear in exit"
        );
        assert!(!map_drops.iter().any(|i| i.block == joined.id));
    }
}
}
