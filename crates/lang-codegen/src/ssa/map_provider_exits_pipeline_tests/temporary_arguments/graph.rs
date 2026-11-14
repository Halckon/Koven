//! Selected argument evaluation, ordinary Borrow ABI and the actual returned owner.
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod factory {
    //! The selected Int factory's successful path returns its actual populated owner chain.
    use super::*;
    use lang_frontend::parser::{Expression, Item, NameMarker};

    fn original_call(
        sources: &SourceMap,
        parsed: &ParsedFile,
        scope: Span,
        wanted: &str,
        member: bool,
    ) -> Result<(lang_frontend::ast::ExpressionId, Span), Failure> {
        one(parsed.ast().expressions().iter().filter_map(|(id,node)|{
            let Expression::Call {callee,..}=node.payload() else{return None;};
            if !ast::inside(node.span(),scope){return None;}
            let callee_node=parsed.ast().expressions().get(*callee).ok()?;
            let matches=if member {
                matches!(callee_node.payload(),Expression::Member {name_span,..} if sources.slice(*name_span)==Ok(wanted))
            }else{matches!(callee_node.payload(),Expression::Name)&&sources.slice(callee_node.span())==Ok(wanted)};
            matches.then_some((id,node.span()))
        }),"argument factory original selected body Call")
    }

    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn check(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        module: &Module,
        f: &Function,
        map: SsaTypeId,
    ) -> Result<(), Failure> {
        let scope = f.origin.span();
        let (constructor_id, constructor_span) =
            original_call(sources, parsed, scope, "mutableMapOf", false)?;
        let (put_id, put_span) = original_call(sources, parsed, scope, "put", true)?;
        let local = |initializer| {
            one(
                parsed.ast().items().iter().filter_map(|(_, node)| {
                    let Item::Variable {
                        name: NameMarker::Present(name),
                        initializer: actual,
                        ..
                    } = node.payload()
                    else {
                        return None;
                    };
                    (ast::inside(node.span(), scope) && *actual == initializer).then_some(*name)
                }),
                "argument factory actual local binding",
            )
        };
        let root_marker = local(constructor_id)?;
        let root = sources
            .slice(root_marker)
            .map_err(|e| failure("argument factory actual root", e))?;
        let receiver = |id| -> Result<(), Failure> {
            let Expression::Call { callee, .. } =
                parsed.ast().expressions().get(id).unwrap().payload()
            else {
                return Err(failure("argument factory source member Call", id));
            };
            let Expression::Member { receiver, .. } =
                parsed.ast().expressions().get(*callee).unwrap().payload()
            else {
                return Err(failure("argument factory source receiver", id));
            };
            need(
                matches!(
                    parsed.ast().expressions().get(*receiver).unwrap().payload(),
                    Expression::Name
                ) && sources.slice(ast::span(parsed, *receiver)) == Ok(root),
                "argument factory put/consume use original constructed root",
                id,
            )
        };
        receiver(put_id)?;
        let (consume_span, returned_marker) = if case.mutable {
            (None, root_marker)
        } else {
            let (id, span) = original_call(sources, parsed, scope, "consume", true)?;
            receiver(id)?;
            (Some(span), local(id)?)
        };
        let returned = one(
            parsed.ast().expressions().iter().filter_map(|(_, node)| {
                let Expression::Return {
                    value: Some(value), ..
                } = node.payload()
                else {
                    return None;
                };
                ast::inside(node.span(), scope).then_some((node.span(), *value))
            }),
            "argument selected factory original Return",
        )?;
        need(
            matches!(
                parsed
                    .ast()
                    .expressions()
                    .get(returned.1)
                    .unwrap()
                    .payload(),
                Expression::Name
            ) && sources.slice(ast::span(parsed, returned.1)) == sources.slice(returned_marker),
            "argument factory actual Return operand names current map result",
            returned,
        )?;
        let g = Graph::with_abort(f)?;
        let construct = at(&g, constructor_span, |o| {
            matches!(o, Operation::MapConstruct { .. })
        })?;
        let put = at(&g, put_span, |o| matches!(o, Operation::MapPut { .. }))?;
        let Operation::MapConstruct { map_type: mutable } = construct.operation else {
            unreachable!()
        };
        let (_, key, val) = module
            .map_container(map)
            .ok_or_else(|| failure("argument factory returned Map representation", map))?;
        need(
            module.map_container(mutable) == Some((MapContainerKind::MutableMap, key, val))
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
                    .count()
                    == 1
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
                    .count()
                    == 1
                && construct.block == put.block
                && g.position(construct)? < g.position(put)?,
            "argument selected factory creates and mutates only original storage",
            (construct, put),
        )?;
        let initial = value(construct)?;
        let Operation::MapPut { owner, .. } = put.operation else {
            unreachable!()
        };
        need(
            owner == initial
                && f.entity(result(construct)?).unwrap().ty == EntityType::Value(mutable)
                && f.entity(result(put)?).unwrap().ty == EntityType::Value(mutable),
            "argument actual put consumes constructed owner and returns next owner",
            put,
        )?;
        let mut uses = Uses::default();
        uses.protect(EntityId::Value(initial));
        uses.permit(EntityId::Value(initial), put);
        let mut current = value(put)?;
        uses.protect(EntityId::Value(current));
        let mut owners = BTreeSet::from([initial.index(), current.index()]);
        if let Some(span) = consume_span {
            let consume = at(&g, span, |o| matches!(o, Operation::MapConsume { .. }))?;
            need(
                consume.operation
                    == Operation::MapConsume {
                        owner: current,
                        map_type: map,
                    }
                    && consume.block == construct.block
                    && g.position(put)? < g.position(consume)?
                    && g.instructions
                        .iter()
                        .filter(|i| matches!(i.operation, Operation::MapConsume { .. }))
                        .count()
                        == 1,
                "argument actual readonly result consumes current populated owner",
                consume,
            )?;
            uses.permit(EntityId::Value(current), consume);
            current = value(consume)?;
            owners.insert(current.index());
            uses.protect(EntityId::Value(current));
        } else {
            need(
                mutable == map
                    && !g
                        .instructions
                        .iter()
                        .any(|i| matches!(i.operation, Operation::MapConsume { .. })),
                "argument mutable factory returns its actual current put owner",
                f.id,
            )?;
        }
        let block = construct.block;
        need(
            matches!(&g.term(block)?.kind,TerminatorKind::Return {values} if values.as_slice()==[current])
                && g.term(block)?.origin == Origin::Source(returned.0)
                && f.entity(EntityId::Value(current)).unwrap().ty == EntityType::Value(map)
                && g.blocks
                    .iter()
                    .filter(|&&block| {
                        matches!(g.term(block).unwrap().kind, TerminatorKind::Return { .. })
                    })
                    .count()
                    == 1,
            "argument actual sole normal Return carries current put/consume owner",
            (g.term(block)?, returned),
        )?;
        let all = f
            .values
            .iter()
            .enumerate()
            .filter_map(|(index, data)| {
                module
                    .map_container(data.ty.semantic_type())
                    .is_some()
                    .then_some(index)
            })
            .collect::<BTreeSet<_>>();
        need(
            all == owners
                && f.places
                    .iter()
                    .chain(&f.loans)
                    .all(|data| module.map_container(data.ty.semantic_type()).is_none())
                && !g.instructions.iter().any(|i| {
                    matches!(
                        i.operation,
                        Operation::DirectCall { .. } | Operation::BorrowCall { .. }
                    )
                }),
            "argument factory complete owned map chain and no alternative map producer/place/loan",
            (all, owners),
        )?;
        uses.check(&g, Some((block, current)))
    }
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod provider {
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
                g.function.entity(EntityId::Value(frame.owner)).unwrap().ty
                    == EntityType::Value(map)
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
}
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
