//! Existing normal provider consumes the real post-argument-CFG owner, with every role explicit.
use super::*;
use lang_frontend::parser::{Item, NameMarker};
use std::collections::BTreeMap;
fn via(
    g: &Graph<'_>,
    edge: &Edge,
    frame: [EntityId; 3],
    flag: Option<EntityId>,
) -> Result<([EntityId; 3], Option<EntityId>), Failure> {
    let expected = frame.into_iter().chain(flag).collect::<BTreeSet<_>>();
    need(
        edge.arguments
            .iter()
            .all(|argument| expected.contains(argument))
            && edge
                .arguments
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                == edge.arguments.len(),
        "complete actual owner/source/cursor and original flag edge",
        (edge, frame, flag),
    )?;
    let carry = |entity: EntityId| -> Result<EntityId, Failure> {
        if edge.arguments.contains(&entity) {
            g.transported(edge, entity)
        } else {
            need(
                inherits(g, entity, edge.target)?,
                "argument CFG inherited provider role still dominates target",
                (edge, entity),
            )?;
            Ok(entity)
        }
    };
    Ok((
        [carry(frame[0])?, carry(frame[1])?, carry(frame[2])?],
        flag.map(carry).transpose()?,
    ))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn check<'p>(
    sources: &SourceMap,
    parsed: &ParsedFile,
    input: &ast::Input,
    site: &ast::Loop,
    module: &Module,
    g: &Graph<'p>,
    call: &Instruction,
    argument_end: &Instruction,
    drop: &Instruction,
    owners: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<&'p Instruction, Failure> {
    let owner = value(call)?;
    let root = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::RootPlace { owner }),
        "provider actual sole returned-owner root",
    )?;
    let EntityId::Place(place) = result(root)? else {
        return Err(failure("provider actual root place", root));
    };
    let begin = one(
        g.instructions.iter().copied().filter(|i| {
            i.operation
                == Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                }
        }),
        "provider actual source lease",
    )?;
    let first = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationFirst { .. })
    })?;
    let initial = [EntityId::Value(owner), result(begin)?, result(first)?];
    need(
        first.operation
            == Operation::MapIterationFirst {
                source: loan(begin)?,
            }
            && root.origin == Origin::Source(site.span)
            && begin.origin == root.origin
            && [root, begin, first].iter().all(|i| i.block == call.block)
            && g.position(argument_end)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(first)?,
        "argument CallReturn cleanup before real returned-owner acquisition in actual merge preheader",
        (argument_end, root, begin, first),
    )?;
    uses.permit(initial[0], root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.permit(initial[1], first);
    let parameter = one(
        g.block(g.function.entry_block().unwrap())?
            .parameters
            .iter()
            .copied(),
        "original scanner flag ABI parameter",
    )?;
    let flags = family(g, parameter)?;
    let current_flags = g
        .block(call.block)?
        .parameters
        .iter()
        .copied()
        .filter(|e| flags.contains(e))
        .collect::<Vec<_>>();
    // Both input routes now use Unit. Refinement SSA threads the merge flag only when
    // the provider actually reads it again; otherwise it stays a dominating definition.
    need(
        current_flags.len() <= 1,
        "Unit provider has one current argument-merge flag",
        &current_flags,
    )?;
    let initial_flag = current_flags.first().copied();
    let entry = g.branch(first.block)?;
    let (header, header_flag) = via(g, entry, initial, initial_flag)?;
    let has = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationHasNext { .. })
    })?;
    need(
        has.block == entry.target
            && has.operation.entities() == [header[1], header[2]]
            && g.ops(has.block)?
                .as_slice()
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>()
                == [has.id],
        "actual sole current HasNext",
        has,
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("actual provider Conditional", has));
    };
    need(
        *condition == value(has)? && g.term(has.block)?.origin == Origin::Source(site.span),
        "actual HasNext condition identity",
        has,
    )?;
    let (body, body_flag) = via(g, when_true, header, header_flag)?;
    let (exit, exit_flag) = via(g, when_false, header, header_flag)?;
    let next = body_query(
        sources,
        parsed,
        site,
        module,
        g,
        when_true.target,
        body,
        uses,
    )?;
    let back = g.branch(when_true.target)?;
    need(
        back.target == has.block
            && via(g, back, [body[0], body[1], result(next)?], body_flag)? == (header, header_flag),
        "every normal backedge role preserves exact current column",
        back,
    )?;
    g.incoming_from(has.block, &[first.block, when_true.target])?;
    g.incoming_from(when_true.target, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    for flag in [initial_flag, header_flag, body_flag, exit_flag]
        .into_iter()
        .flatten()
    {
        need(
            flags.contains(&flag),
            "extra provider column is original flag family",
            flag,
        )?;
        g.anchored(flag, &[parameter])?;
        uses.protect(flag);
    }
    let map = g.function.entity(initial[0]).unwrap().ty.semantic_type();
    for frame in [initial, header, body, exit] {
        let cursor_type = g.function.entity(frame[2]).unwrap().ty;
        let cursor_is_u64 = matches!(cursor_type, EntityType::Value(t)
            if module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 64, signed: false }));
        need(
            owners.contains(&frame[0])
                && g.function.entity(frame[0]).unwrap().ty == EntityType::Value(map)
                && g.function.entity(frame[1]).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map,
                    }
                && cursor_is_u64,
            "every current provider role has exact represented type",
            frame,
        )?;
        g.anchored(frame[0], &[initial[0]])?;
        g.anchored(frame[1], &[initial[1]])?;
        g.anchored(frame[2], &[initial[2], result(next)?])?;
        for e in frame {
            uses.protect(e);
        }
    }
    uses.permit(header[1], has);
    uses.permit(header[2], has);
    let source_end = one(
        g.instructions.iter().copied().filter(
            |i| matches!(i.operation,Operation::BorrowEnd {loan} if EntityId::Loan(loan)==exit[1]),
        ),
        "actual exhausted source End",
    )?;
    let ops = g.ops(when_false.target)?;
    need(
        source_end.block == when_false.target
            && source_end.origin == Origin::Source(site.span)
            && drop.block == source_end.block
            && drop.origin == Origin::Source(input.call)
            && matches!(drop.operation,Operation::Drop {owner} if EntityId::Value(owner)==exit[0])
            && ops.first().map(|i| i.id) == Some(source_end.id)
            && ops.get(1).map(|i| i.id) == Some(drop.id)
            && matches!(&g.term(drop.block)?.kind,TerminatorKind::Return {values} if values.is_empty()),
        "false-edge source End then exact current hidden owner Drop then scanner Return",
        (source_end, drop),
    )?;
    uses.permit(exit[1], source_end);
    uses.permit(exit[0], drop);
    let condition_entity = result(has)?;
    need(
        g.instructions
            .iter()
            .all(|i| !i.operation.entities().contains(&condition_entity))
            && g.edges
                .iter()
                .all(|(_, edge)| !edge.arguments.contains(&condition_entity))
            && g.blocks.iter().all(|&b| {
                b == has.block
                    || !g
                        .term(b)
                        .unwrap()
                        .kind
                        .entities()
                        .contains(&condition_entity)
            }),
        "HasNext result only used by original guard",
        has,
    )?;
    need(
        g.instructions
            .iter()
            .filter(|i| iteration(&i.operation))
            .count()
            == 5,
        "all First/HasNext/K/V/Next operations accounted",
        g.function.id,
    )?;
    // No provider source/cursor is transported beyond its original three-role edges.
    let leases = family(g, initial[1])?;
    let cursors = [initial[2], header[2], body[2], exit[2], result(next)?]
        .into_iter()
        .collect::<BTreeSet<_>>();
    for (from, edge) in &g.edges {
        for e in &edge.arguments {
            if leases.contains(e) || cursors.contains(e) {
                need(
                    (*from == first.block && edge.target == has.block)
                        || (*from == has.block
                            && [when_true.target, when_false.target].contains(&edge.target))
                        || (*from == when_true.target && edge.target == has.block),
                    "provider state never escapes its exact live edges",
                    (from, edge, e),
                )?;
            }
        }
    }
    let mut expected = BTreeSet::from([EntityId::Place(place)]);
    for frame in [initial, header, body, exit] {
        need(
            expected.insert(frame[0]) && expected.insert(frame[1]),
            "Unit CFG provider has distinct current owners and source leases",
            frame,
        )?;
    }
    super::super::query_inventory::check(g, &[map], &expected)?;
    Ok(source_end)
}

#[allow(clippy::too_many_arguments)]
fn body_query<'p>(
    sources: &SourceMap,
    parsed: &ParsedFile,
    site: &ast::Loop,
    module: &Module,
    g: &Graph<'p>,
    block: BlockId,
    frame: [EntityId; 3],
    uses: &mut Uses,
) -> Result<&'p Instruction, Failure> {
    let reader_span = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Function {
                name: NameMarker::Present(name),
                ..
            } = n.payload()
            else {
                return None;
            };
            (sources.slice(*name) == Ok("read")).then_some(n.span())
        }),
        "original reader declaration",
    )?;
    let reader = function(module, reader_span)?;
    let key = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationKeyLoan { .. })
    })?;
    let val = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationValueLoan { .. })
    })?;
    let next = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationNext { .. })
    })?;
    let args = [loan(key)?, loan(val)?];
    need(
        [key, val, next]
            .iter()
            .all(|i| i.block == block && i.operation.entities() == [frame[1], frame[2]])
            && reader.return_types.is_empty(),
        "body provider uses exact current source and cursor",
        (key, val, next),
    )?;
    let params = &reader
        .block(reader.entry_block().unwrap())
        .unwrap()
        .parameters;
    let shared_string_parameters = args.iter().zip(params).all(|(&l, &p)| {
        let ty = g.function.entity(EntityId::Loan(l)).unwrap().ty;
        matches!(ty, EntityType::Loan { kind: LoanKind::Shared, target }
            if module.type_kind(target) == Some(&SsaTypeKind::StringOwner))
            && reader.entity(p).unwrap().ty == ty
    });
    need(
        params.len() == 2 && shared_string_parameters,
        "original reader has exact Shared String K/V ABI",
        reader.id,
    )?;
    let mut lineage = BTreeMap::from([(args[0], (0usize, None)), (args[1], (1usize, None))]);
    let mut live = BTreeSet::new();
    let mut ended = Vec::new();
    let mut called = false;
    for i in g.ops(block)? {
        match &i.operation {
            Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. } => {
                let l = loan(i)?;
                need(
                    args.contains(&l) && live.insert(l),
                    "actual component birth once",
                    i,
                )?;
                uses.protect(EntityId::Loan(l));
                uses.permit(frame[1], i);
                uses.permit(frame[2], i);
            }
            Operation::SharedReborrow { source } => {
                let (slot, _) = *lineage
                    .get(source)
                    .ok_or_else(|| failure("actual reader reborrow parent", i))?;
                let child = loan(i)?;
                need(
                    live.contains(source)
                        && i.origin == Origin::Source(site.arguments[slot])
                        && g.function.entity(EntityId::Loan(child)).unwrap().ty
                            == g.function.entity(EntityId::Loan(*source)).unwrap().ty
                        && lineage.insert(child, (slot, Some(*source))).is_none()
                        && live.insert(child),
                    "real ordered typed child loan",
                    i,
                )?;
                uses.permit(EntityId::Loan(*source), i);
                uses.protect(EntityId::Loan(child));
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
                        && i.origin == Origin::Source(site.read)
                        && arguments.len() == 2
                        && i.results.is_empty(),
                    "actual source read Call exactly once",
                    i,
                )?;
                for (slot, e) in arguments.iter().enumerate() {
                    let EntityId::Loan(l) = *e else {
                        return Err(failure("reader actual loan argument", e));
                    };
                    need(
                        live.contains(&l) && lineage.get(&l).is_some_and(|(n, _)| *n == slot),
                        "actual K/V caller lineage order",
                        e,
                    )?;
                    uses.permit(*e, i);
                }
                called = true;
            }
            Operation::BorrowEnd { loan } => {
                let (_, parent) = *lineage
                    .get(loan)
                    .ok_or_else(|| failure("ended actual component loan", i))?;
                need(
                    called
                        && live.contains(loan)
                        && !lineage
                            .iter()
                            .any(|(child, (_, p))| *p == Some(*loan) && live.contains(child))
                        && i.origin
                            == Origin::Source(if parent.is_some() {
                                site.read
                            } else {
                                site.span
                            }),
                    "actual child End before component End after read",
                    i,
                )?;
                live.remove(loan);
                if parent.is_none() {
                    ended.push(*loan);
                }
                uses.permit(EntityId::Loan(*loan), i);
            }
            Operation::MapIterationNext { .. } => {
                need(
                    i.id == next.id && called && live.is_empty() && ended == [args[1], args[0]],
                    "read and complete reverse component cleanup before Next",
                    i,
                )?;
                uses.permit(frame[1], i);
                uses.permit(frame[2], i);
                uses.protect(result(i)?);
            }
            _ => {
                return Err(failure(
                    "unexpected ordinary normal provider body operation",
                    i,
                ));
            }
        }
    }
    let ops = g.ops(block)?;
    need(
        called
            && live.is_empty()
            && ended == [args[1], args[0]]
            && ops.first().map(|i| i.id) == Some(key.id)
            && ops.last().map(|i| i.id) == Some(next.id)
            && g.position(key)? < g.position(val)?
            && g.edges.iter().all(|(_, e)| {
                e.arguments
                    .iter()
                    .all(|v| !matches!(v,EntityId::Loan(l) if lineage.contains_key(l)))
            }),
        "complete original body has no component loan escape",
        ended,
    )?;
    // The body anchor is taken from the original For, never a coincidentally nearby call.
    need(
        parsed.ast().statements().get(site.body).is_ok_and(|n| {
            n.span().start() < site.read.start() && site.read.end() < n.span().end()
        }),
        "actual read belongs to original For body",
        site.read,
    )?;
    Ok(next)
}

pub(in crate::ssa) mod argument {
    //! Both original argument arms and all scalar/loan uses through the actual selected Call.
    use super::super::*;
    /// Actual source anchors shared by the original temporary and named-local adapters.
    pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Input {
        pub call: Span,
        pub argument: Span,
        pub flag: Span,
        pub arms: [Span; 2],
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn check_unit<'p>(
        sources: &SourceMap,
        parsed: &ParsedFile,
        input: &Input,
        module: &Module,
        g: &Graph<'p>,
        factory: &Function,
        uses: &mut Uses,
    ) -> Result<(&'p Instruction, &'p Instruction), Failure> {
        let call = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation,Operation::DirectCall {callee,..} if callee==factory.id),
            ),
            "exactly one selected factory Call",
        )?;
        need(
            call.origin == Origin::Source(input.call),
            "selected factory actual source Call origin",
            call,
        )?;
        let Operation::DirectCall {
            receiver: None,
            arguments,
            ..
        } = &call.operation
        else {
            return Err(failure("ordinary factory direct ABI", call));
        };
        let [arg @ EntityId::Loan(actual_loan)] = arguments.as_slice() else {
            return Err(failure("factory argument actual Shared Int loan", call));
        };
        let begin = g.definition(*arg)?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return Err(failure("actual argument Shared birth", begin));
        };
        let root = g.definition(EntityId::Place(place))?;
        let Operation::RootPlace { owner: phi } = root.operation else {
            return Err(failure("actual argument RootPlace", root));
        };
        let int = g
            .function
            .entity(EntityId::Value(phi))
            .unwrap()
            .ty
            .semantic_type();
        let factory_parameters = &factory
            .block(factory.entry_block().unwrap())
            .unwrap()
            .parameters;
        let [factory_parameter] = factory_parameters.as_slice() else {
            return Err(failure(
                "factory actual one Borrow Int parameter",
                factory_parameters,
            ));
        };
        let shared_int = EntityType::Loan {
            kind: LoanKind::Shared,
            target: int,
        };
        need(
            module.type_kind(int)
                == Some(&SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                })
                && g.function.entity(*arg).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: int,
                    }
                && g.function.entity(EntityId::Place(place)).unwrap().ty == EntityType::Place(int)
                && factory.entity(*factory_parameter).unwrap().ty == shared_int,
            "actual original factory Borrow Int32 entry ABI",
            call,
        )?;
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: *actual_loan }),
            "actual argument unique CallReturn End",
        )?;
        need(
            root.origin == Origin::Source(input.argument)
                && begin.origin == root.origin
                && end.origin == Origin::Source(input.call)
                && [root, begin, end].iter().all(|i| i.block == call.block)
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(call)?
                && g.position(call)? < g.position(end)?,
            "If result Root/Borrow/actual Call/End full order",
            (root, begin, call, end),
        )?;
        let data = g.function.entity(EntityId::Value(phi)).unwrap();
        need(
            matches!(data.definition,Definition::BlockParameter {block,..} if block==call.block),
            "factory argument is actual merge Value parameter",
            data,
        )?;
        let yes = at(g, input.arms[0], |o| {
            *o == Operation::Constant(ScalarConstant::Integer(1))
        })?;
        let no = at(g, input.arms[1], |o| {
            *o == Operation::Constant(ScalarConstant::Integer(2))
        })?;
        need(
            [yes, no].iter().all(|i| {
                g.function.entity(result(i).unwrap()).unwrap().ty == EntityType::Value(int)
            }),
            "both literal arm results have exact Int32 storage",
            (yes, no),
        )?;
        let flag = at(g, input.flag, |o| matches!(o, Operation::Read { .. }))?;
        let Operation::Read {
            source: PlaceAccess::Loan(flag_source),
        } = flag.operation
        else {
            return Err(failure("argument flag reads actual Borrow Boolean", flag));
        };
        let params = &g.block(g.function.entry_block().unwrap())?.parameters;
        let [parameter] = params.as_slice() else {
            return Err(failure("actual scanner one Borrow Boolean entry", params));
        };
        let boolean = g.function.entity(*parameter).unwrap().ty.semantic_type();
        need(
            module.type_kind(boolean) == Some(&SsaTypeKind::Boolean)
                && g.function.entity(*parameter).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: boolean,
                    }
                && g.function.entity(result(flag)?).unwrap().ty == EntityType::Value(boolean),
            "actual original scanner Borrow Boolean ABI and Read result",
            flag,
        )?;
        g.anchored(EntityId::Loan(flag_source), &[*parameter])?;
        let term = g.term(flag.block)?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &term.kind
        else {
            return Err(failure("actual argument If Conditional", term));
        };
        need(
            *condition == value(flag)?
                && term.origin == Origin::Source(input.argument)
                && when_true.target == yes.block
                && when_false.target == no.block
                && yes.block != no.block,
            "true arm is original 1 and false arm original 2",
            term,
        )?;
        g.incoming_from(yes.block, &[flag.block])?;
        g.incoming_from(no.block, &[flag.block])?;
        let y = g.branch(yes.block)?;
        let n = g.branch(no.block)?;
        need(
            y.target == call.block
                && n.target == call.block
                && g.argument(y, EntityId::Value(phi))? == result(yes)?
                && g.argument(n, EntityId::Value(phi))? == result(no)?
                && g.term(yes.block)?.origin == Origin::Source(input.argument)
                && g.term(no.block)?.origin == Origin::Source(input.argument),
            "both exact original literal incoming columns feed actual Call argument Phi",
            (y, n, phi),
        )?;
        g.incoming_from(call.block, &[yes.block, no.block])?;
        g.anchored(EntityId::Value(phi), &[result(yes)?, result(no)?])?;
        let scalar_family = [result(yes)?, result(no)?, EntityId::Value(phi)]
            .into_iter()
            .collect::<BTreeSet<_>>();
        for &e in &scalar_family {
            uses.protect(e);
        }
        uses.permit(EntityId::Value(phi), root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        uses.protect(*arg);
        uses.permit(*arg, call);
        uses.permit(*arg, end);
        let flags = family(g, *parameter)?;
        for &e in &flags {
            uses.protect(e);
        }
        let current_flags = g
            .block(call.block)?
            .parameters
            .iter()
            .copied()
            .filter(|e| flags.contains(e))
            .collect::<Vec<_>>();
        // Refinement SSA keeps an unchanged caller flag as a dominating definition
        // instead of threading it through every arm; both shapes stay valid here.
        let threaded = !current_flags.is_empty();
        let current = match current_flags.as_slice() {
            [current] => *current,
            [] => {
                need(
                    inherits(g, *parameter, call.block)?,
                    "Unit argument merge flag still dominates call block",
                    (*parameter, call.block),
                )?;
                *parameter
            }
            _ => {
                return Err(failure(
                    "Unit argument merge duplicate flag columns",
                    current_flags,
                ));
            }
        };
        let yes_flag = if when_true.arguments.contains(parameter) {
            g.transported(when_true, *parameter)?
        } else {
            need(
                inherits(g, *parameter, when_true.target)?,
                "Unit argument true arm flag still dominates",
                (*parameter, when_true.target),
            )?;
            *parameter
        };
        let no_flag = if when_false.arguments.contains(parameter) {
            g.transported(when_false, *parameter)?
        } else {
            need(
                inherits(g, *parameter, when_false.target)?,
                "Unit argument false arm flag still dominates",
                (*parameter, when_false.target),
            )?;
            *parameter
        };
        let carries_y = y.arguments.contains(&current);
        let carries_n = n.arguments.contains(&current);
        let mut yes_expected = BTreeSet::from([result(yes)?]);
        let mut no_expected = BTreeSet::from([result(no)?]);
        let mut call_expected = BTreeSet::from([EntityId::Value(phi)]);
        if threaded {
            yes_expected.insert(current);
            no_expected.insert(current);
            call_expected.insert(current);
        }
        let arms_carry =
            |edge: &Edge| edge.arguments.is_empty() || edge.arguments.as_slice() == [*parameter];
        let yes_parameters = &g.block(yes.block)?.parameters;
        let no_parameters = &g.block(no.block)?.parameters;
        let arm_parameters =
            |parameters: &[EntityId], flag: EntityId| parameters.is_empty() || parameters == [flag];
        need(
            (!carries_y || g.argument(y, current)? == yes_flag)
                && (!carries_n || g.argument(n, current)? == no_flag)
                && arms_carry(when_true)
                && arms_carry(when_false)
                && arm_parameters(yes_parameters, yes_flag)
                && arm_parameters(no_parameters, no_flag)
                && y.arguments.iter().copied().collect::<BTreeSet<_>>() == yes_expected
                && n.arguments.iter().copied().collect::<BTreeSet<_>>() == no_expected
                && g.block(call.block)?
                    .parameters
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>()
                    == call_expected,
            "Unit argument merge keeps the exact complete flag and literal columns from both arms",
            (current, yes_flag, no_flag, threaded),
        )?;
        uses.permit(EntityId::Loan(flag_source), flag);
        let flag_ends = g
            .instructions
            .iter()
            .copied()
            .filter(|i| {
                matches!(i.operation, Operation::BorrowEnd { loan }
            if flags.contains(&EntityId::Loan(loan)))
            })
            .collect::<Vec<_>>();
        if let [end] = flag_ends.as_slice() {
            let Operation::BorrowEnd { loan } = end.operation else {
                unreachable!()
            };
            let candidates = g
                .block(end.block)?
                .parameters
                .iter()
                .copied()
                .filter(|e| flags.contains(e))
                .collect::<Vec<_>>();
            let current = one(
                candidates.into_iter(),
                "Unit flag End closes its actual current callable-Return column",
            )?;
            need(
                EntityId::Loan(loan) == current
                    && matches!(&g.term(end.block)?.kind, TerminatorKind::Return { values }
                    if values.is_empty()),
                "flag ABI End, when emitted, closes exact current loan only at callable Return",
                end,
            )?;
            uses.permit(current, end);
        } else {
            need(
                flag_ends.is_empty(),
                "flag ABI has no duplicate End sites",
                flag_ends,
            )?;
        }
        for (from, edge) in &g.edges {
            need(
                !edge.arguments.contains(arg) && !edge.arguments.contains(&EntityId::Place(place)),
                "argument synchronous loan/place never escapes merge block",
                edge,
            )?;
            for e in &edge.arguments {
                if scalar_family.contains(e) {
                    need(
                        (*from == yes.block && *e == result(yes)?)
                            || (*from == no.block && *e == result(no)?),
                        "argument scalar transport only its original two incoming edges",
                        (from, edge, e),
                    )?;
                }
            }
        }
        let read = result(flag)?;
        need(
            g.instructions
                .iter()
                .all(|i| !i.operation.entities().contains(&read))
                && g.edges.iter().all(|(_, e)| !e.arguments.contains(&read))
                && g.blocks.iter().all(|&b| {
                    b == flag.block || !g.term(b).unwrap().kind.entities().contains(&read)
                }),
            "flag Read has only actual original Conditional use",
            flag,
        )?;
        // Re-read original spans here so a source/IR mismatch cannot hide behind selected values.
        need(
            sources.slice(input.arms[0]) == Ok("1")
                && sources.slice(input.arms[1]) == Ok("2")
                && parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter(|(_, n)| n.span() == input.argument)
                    .count()
                    == 1,
            "same original argument AST identity",
            input.argument,
        )?;
        Ok((call, end))
    }
}
