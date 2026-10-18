//! The actual source Call, receiver projection and disjoint epoch owners in reachable SSA.
use super::{RouteCase, RouteProof, route_call};
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, function, result, value},
        query_inputs::one,
    },
    model::*,
};
use lang_frontend::{
    parser::{Expression, Item, NameMarker, ParsedFile},
    source::{SourceMap, Span},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn implementation(
    sources: &SourceMap,
    parsed: &ParsedFile,
) -> Result<(Span, Span), Failure> {
    let class = one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            let Item::Classifier(class) = node.payload() else {
                return None;
            };
            matches!(class.name, NameMarker::Present(name) if sources.slice(name) == Ok("Factory"))
                .then_some(node.span())
        }),
        "ordinary original Factory declaration",
    )?;
    one(
        parsed.ast().items().iter().filter_map(|(_, node)| {
            let Item::Function {
                name: NameMarker::Present(name),
                ..
            } = node.payload()
            else {
                return None;
            };
            (sources.slice(*name) == Ok("other")
                && node.span().start() > class.start()
                && node.span().end() < class.end())
            .then_some((node.span(), *name))
        }),
        "ordinary original Factory.other implementation",
    )
}

// Only actual block-parameter transport is accepted. Every incoming column must
// terminate at this producer; a cycle with no producer cannot prove identity.
fn transport(g: &Graph<'_>, current: EntityId, anchor: EntityId) -> Result<(), Failure> {
    let mut pending = vec![current];
    let mut seen = BTreeSet::new();
    let mut dependencies = BTreeMap::new();
    while let Some(entity) = pending.pop() {
        if entity == anchor {
            continue;
        }
        if !seen.insert(entity) {
            continue;
        }
        let data = g
            .function
            .entity(entity)
            .ok_or_else(|| failure("ordinary transported entity", entity))?;
        let Definition::BlockParameter { block, .. } = data.definition else {
            return Err(failure(
                "ordinary epoch identity reaches another producer",
                (entity, anchor, data),
            ));
        };
        let incoming = g.incoming(block);
        need(
            !incoming.is_empty(),
            "ordinary full incoming transport exists",
            entity,
        )?;
        let arguments = incoming
            .into_iter()
            .map(|(_, edge)| g.argument(edge, entity))
            .collect::<Result<Vec<_>, _>>()?;
        pending.extend(arguments.iter().copied());
        dependencies.insert(entity, arguments);
    }
    let mut anchored = BTreeSet::from([anchor]);
    loop {
        let before = anchored.len();
        for (entity, arguments) in &dependencies {
            if arguments.iter().any(|argument| anchored.contains(argument)) {
                anchored.insert(*entity);
            }
        }
        if anchored.len() == before {
            break;
        }
    }
    need(
        seen.is_subset(&anchored),
        "ordinary every incoming transport can reach original producer",
        (current, anchor, seen, anchored),
    )
}

fn ast_call(
    parsed: &ParsedFile,
    id: lang_frontend::ast::ExpressionId,
) -> Result<(Span, &Expression), Failure> {
    let node = parsed
        .ast()
        .expressions()
        .get(id)
        .map_err(|e| failure("ordinary original Call node", e))?;
    Ok((node.span(), node.payload()))
}

fn allocation<'a>(g: &Graph<'a>, span: Span) -> Result<&'a Instruction, Failure> {
    one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(span)
                && matches!(i.operation, Operation::HeapAllocate { .. })
        }),
        "ordinary actual source receiver allocation",
    )
}

fn receiver(
    g: &Graph<'_>,
    parsed: &ParsedFile,
    call: &Instruction,
    original: &Expression,
    proof: &RouteProof,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let Expression::Call { callee, .. } = original else {
        return Err(failure("ordinary original Call", original));
    };
    let Expression::Member { receiver, .. } =
        parsed.ast().expressions().get(*callee).unwrap().payload()
    else {
        return Err(failure("ordinary actual member receiver AST", callee));
    };
    let (receiver_span, constructor) = ast_call(parsed, *receiver)?;
    let alloc = allocation(g, receiver_span)?;
    let owner = value(alloc)?;
    let Operation::DirectCall {
        receiver: Some(EntityId::Loan(actual)),
        ..
    } = call.operation
    else {
        return Err(failure("ordinary actual shared receiver operand", call));
    };
    let mut receiver = actual;
    let mut ends = Vec::new();
    uses.protect(EntityId::Loan(actual));
    uses.permit(EntityId::Loan(actual), call);
    if let Some(expected_field) = proof.field {
        let projection = g.definition(EntityId::Loan(actual))?;
        let Operation::SharedHeapFieldLoan { base, field } = projection.operation else {
            return Err(failure(
                "ordinary delegation really projects the heap field",
                projection,
            ));
        };
        need(
            field == expected_field && projection.origin == Origin::Source(proof.begin),
            "ordinary resolved delegation field index",
            projection,
        )?;
        receiver = base;
        uses.protect(EntityId::Loan(base));
        uses.permit(EntityId::Loan(base), projection);
        let Expression::Call { arguments, .. } = constructor else {
            return Err(failure("ordinary Host constructor", constructor));
        };
        let [argument] = arguments.as_slice() else {
            return Err(failure(
                "ordinary one delegate constructor argument",
                arguments,
            ));
        };
        let (delegate_span, _) = ast_call(parsed, argument.value)?;
        let delegate = allocation(g, delegate_span)?;
        let Operation::HeapAllocate { payload, .. } = alloc.operation else {
            unreachable!()
        };
        let payload = g.definition(EntityId::Value(payload))?;
        need(
            matches!(&payload.operation, Operation::AggregateConstruct {fields, ..}
            if fields.as_slice() == [value(delegate)?]),
            "ordinary Host stores exact Factory owner in delegate field",
            payload,
        )?;
        need(
            g.function
                .entity(EntityId::Loan(actual))
                .unwrap()
                .ty
                .semantic_type()
                == g.function
                    .entity(result(delegate)?)
                    .unwrap()
                    .ty
                    .semantic_type(),
            "ordinary projected receiver is actual Factory allocation type",
            projection,
        )?;
        uses.protect(result(delegate)?);
        uses.permit(result(delegate)?, payload);
        ends.push(actual);
    }
    let begin = g.definition(EntityId::Loan(receiver))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure(
            "ordinary outer receiver real shared BorrowBegin",
            begin,
        ));
    };
    let root = g.definition(EntityId::Place(place))?;
    need(
        root.operation == Operation::RootPlace { owner }
            && root.origin == Origin::Source(proof.begin)
            && begin.origin == Origin::Source(proof.begin)
            && root.block == alloc.block
            && begin.block == alloc.block
            && call.block == alloc.block
            && g.position(alloc)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(call)?,
        "ordinary receiver borrows exact source construction",
        (alloc, root, begin, call),
    )?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    ends.push(receiver);
    let mut previous = call;
    for current in ends {
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: current }),
            "ordinary unique actual receiver End",
        )?;
        need(
            end.origin == Origin::Source(proof.end)
                && end.block == call.block
                && g.position(previous)? < g.position(end)?,
            "ordinary call then delegate End then outer End",
            (previous, end),
        )?;
        uses.permit(EntityId::Loan(current), end);
        previous = end;
    }
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner }),
        "ordinary unique temporary receiver Drop",
    )?;
    need(
        drop.block == call.block && g.position(previous)? < g.position(drop)?,
        "ordinary receiver Drop follows all receiver Ends",
        drop,
    )?;
    uses.permit(EntityId::Value(owner), drop);
    Ok(())
}

pub(super) fn check(
    case: &RouteCase,
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
    proof: &RouteProof,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "ordinary actual source module")?;
    let entry_span = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            matches!(n.payload(), Item::Function {name:NameMarker::Present(name),..}
            if sources.slice(*name) == Ok("entry"))
            .then_some(n.span())
        }),
        "ordinary original entry",
    )?;
    let entry = function(module, entry_span)?;
    let g = Graph::with_abort(entry)?;
    let original = route_call(case, sources, parsed)?;
    let (span, ast) = ast_call(parsed, original)?;
    let implementation = function(module, proof.implementation)?;
    let call = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(span) && matches!(i.operation, Operation::DirectCall { .. })
        }),
        "ordinary unique actual Callspan DirectCall",
    )?;
    let Operation::DirectCall {
        callee,
        receiver: actual_receiver,
        ref arguments,
    } = call.operation
    else {
        unreachable!()
    };
    need(
        callee == implementation.id,
        "ordinary actual callee has original Factory.other origin",
        (call, implementation),
    )?;
    let [argument] = arguments.as_slice() else {
        return Err(failure("ordinary actual single ABI argument", call));
    };
    let Expression::Call {
        arguments: original_arguments,
        ..
    } = ast
    else {
        unreachable!()
    };
    let argument_span = parsed
        .ast()
        .expressions()
        .get(original_arguments[0].value)
        .unwrap()
        .span();
    let EntityId::Loan(argument_loan) = *argument else {
        return Err(failure("ordinary Borrow Int ABI loan", call));
    };
    let argument_begin = g.definition(*argument)?;
    let Operation::BorrowBegin {
        place: argument_place,
        kind: LoanKind::Shared,
    } = argument_begin.operation
    else {
        return Err(failure("ordinary argument shared loan", argument_begin));
    };
    let argument_root = g.definition(EntityId::Place(argument_place))?;
    let Operation::RootPlace {
        owner: argument_owner,
    } = argument_root.operation
    else {
        return Err(failure("ordinary Int argument actual root", argument_root));
    };
    let literal = g.definition(EntityId::Value(argument_owner))?;
    need(
        literal.operation == Operation::Constant(ScalarConstant::Integer(1))
            && literal.origin == Origin::Source(argument_span)
            && argument_root.origin == Origin::Source(argument_span)
            && argument_begin.origin == Origin::Source(argument_span),
        "ordinary real Int1 through shared ABI reaches implementation",
        (literal, argument_root, argument_begin),
    )?;
    let argument_end = one(
        g.instructions.iter().copied().filter(|i| {
            i.operation
                == (Operation::BorrowEnd {
                    loan: argument_loan,
                })
        }),
        "ordinary argument loan ends exactly once",
    )?;
    need(
        argument_begin.block == call.block
            && call.block == argument_end.block
            && g.position(argument_begin)? < g.position(call)?
            && g.position(call)? < g.position(argument_end)?,
        "ordinary argument complete Borrow delivery and end order",
        (argument_begin, call, argument_end),
    )?;
    let parameters = &implementation
        .block(implementation.entry_block().unwrap())
        .unwrap()
        .parameters;
    let expected = actual_receiver
        .into_iter()
        .chain(arguments.iter().copied())
        .collect::<Vec<_>>();
    need(
        parameters.len() == expected.len()
            && parameters.iter().zip(&expected).all(|(p, a)| {
                implementation.entity(*p).unwrap().ty == entry.entity(*a).unwrap().ty
            }),
        "ordinary complete selected receiver and argument ABI",
        (parameters, expected),
    )?;
    let mut uses = Uses::default();
    uses.protect(*argument);
    uses.permit(*argument, call);
    uses.permit(*argument, argument_end);
    uses.protect(EntityId::Place(argument_place));
    uses.permit(EntityId::Place(argument_place), argument_begin);
    uses.protect(EntityId::Value(argument_owner));
    uses.permit(EntityId::Value(argument_owner), argument_root);
    receiver(&g, parsed, call, ast, proof, &mut uses)?;
    uses.check(&g, None)?;
    let ordinary = result(call)?;
    let map_type = entry.entity(ordinary).unwrap().ty.semantic_type();
    need(
        matches!(ordinary, EntityId::Value(_))
            && module.map_container(map_type).is_some()
            && implementation.return_types.as_slice() == [map_type],
        "ordinary actual owned Map result",
        call,
    )?;
    let mut births = vec![call];
    if case.provider {
        let make_span = one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                matches!(node.payload(), Item::Function {name:NameMarker::Present(name),..}
                if sources.slice(*name) == Ok("make"))
                .then_some(node.span())
            }),
            "ordinary current-provider original make",
        )?;
        let make = function(module, make_span)?;
        let original_make = one(
            parsed.ast().expressions().iter().filter_map(|(_, node)| {
                let Expression::Call {
                    callee, arguments, ..
                } = node.payload()
                else {
                    return None;
                };
                let callee = parsed.ast().expressions().get(*callee).ok()?;
                (sources.slice(callee.span()) == Ok("make")
                    && arguments.is_empty()
                    && node.span().start() > entry_span.start()
                    && node.span().end() < entry_span.end())
                .then_some(node.span())
            }),
            "ordinary original current-provider make Call",
        )?;
        let current = one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(original_make) && matches!(&i.operation,
            Operation::DirectCall {callee, receiver:None, arguments} if *callee == make.id && arguments.is_empty())),
            "ordinary current provider zeroarg make Call")?;
        let original_for = one(
            parsed.ast().statements().iter().filter_map(|(_, node)| {
                matches!(node.payload(), lang_frontend::parser::Statement::For { .. })
                    .then_some(node.span())
            }),
            "ordinary original For span",
        )?;
        let first = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(original_for)
                    && matches!(i.operation, Operation::MapIterationFirst { .. })
            }),
            "ordinary actual current provider First",
        )?;
        let Operation::MapIterationFirst { source } = first.operation else {
            unreachable!()
        };
        let begin = g.definition(EntityId::Loan(source))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return Err(failure("ordinary provider source shared lease", begin));
        };
        let root = g.definition(EntityId::Place(place))?;
        let Operation::RootPlace { owner } = root.operation else {
            return Err(failure("ordinary provider source actual root", root));
        };
        transport(&g, EntityId::Value(owner), result(current)?)?;
        births.push(current);
    }
    let drops = g.instructions.iter().copied().filter(|i| matches!(i.operation,
        Operation::Drop {owner} if entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type() == map_type)).collect::<Vec<_>>();
    need(
        drops.len() == births.len(),
        "ordinary exactly one Map Drop per actual binding epoch",
        &drops,
    )?;
    for birth in &births {
        let anchor = result(birth)?;
        let matching = drops
            .iter()
            .copied()
            .filter(|drop| {
                let Operation::Drop { owner } = drop.operation else {
                    unreachable!()
                };
                transport(&g, EntityId::Value(owner), anchor).is_ok()
            })
            .collect::<Vec<_>>();
        need(
            matching.len() == 1,
            "ordinary each Map Drop traces every incoming to its own epoch Call",
            (birth, matching),
        )?;
    }
    paths(&g, &births, &drops)
}

fn paths(g: &Graph<'_>, births: &[&Instruction], drops: &[&Instruction]) -> Result<(), Failure> {
    let mut pending = vec![(
        g.function.entry_block().unwrap(),
        vec![false; births.len()],
        0usize,
    )];
    let mut seen = BTreeSet::new();
    let mut returns = 0;
    while let Some((block, mut formed, mut count)) = pending.pop() {
        if !seen.insert((block, formed.clone(), count)) {
            continue;
        }
        for instruction in g.ops(block)? {
            if let Some(index) = births.iter().position(|birth| birth.id == instruction.id) {
                need(
                    !formed[index],
                    "ordinary epoch factory cannot repeat in a CFG cycle",
                    instruction,
                )?;
                formed[index] = true;
            }
            if drops.iter().any(|drop| drop.id == instruction.id) {
                let Operation::Drop { owner } = instruction.operation else {
                    unreachable!()
                };
                let index = one(
                    births.iter().enumerate().filter_map(|(index, birth)| {
                        transport(g, EntityId::Value(owner), result(birth).ok()?)
                            .is_ok()
                            .then_some(index)
                    }),
                    "ordinary Drop belongs to exactly one complete epoch",
                )?;
                need(
                    formed[index] && count & (1 << index) == 0,
                    "ordinary Drop after formation and never repeated",
                    instruction,
                )?;
                count |= 1 << index;
            }
        }
        if matches!(g.term(block)?.kind, TerminatorKind::Return { .. }) {
            need(
                formed.iter().all(|formed| *formed) && count == (1 << births.len()) - 1,
                "ordinary every normal Return drops each independent epoch exactly once",
                (block, &formed, count),
            )?;
            returns += 1;
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, formed.clone(), count));
        }
    }
    need(
        returns > 0,
        "ordinary actual normal Return exists",
        g.function.id,
    )
}
