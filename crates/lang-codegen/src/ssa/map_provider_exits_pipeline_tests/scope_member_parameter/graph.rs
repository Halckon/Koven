//! The hidden receiver and explicit source keep independent entry/caller identities.
mod provider;
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, result, value},
    query_inputs::one,
};
use super::{
    Case,
    ast::{self, Input},
    facts::Proof,
};
use crate::ssa::model::*;
use lang_frontend::{parser::ParsedFile, source::Span};
use std::collections::BTreeSet;
fn at<'a>(
    g: &Graph<'a>,
    span: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> Result<&'a Instruction, Failure> {
    one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.origin == Origin::Source(span) && predicate(&op.operation)),
        "P24 actual operation at original source span",
    )
}
fn family(g: &Graph<'_>, anchors: &[EntityId]) -> Result<BTreeSet<EntityId>, Failure> {
    let mut found = anchors.iter().copied().collect::<BTreeSet<_>>();
    loop {
        let before = found.len();
        for (_, edge) in &g.edges {
            for (&argument, &parameter) in
                edge.arguments.iter().zip(&g.block(edge.target)?.parameters)
            {
                if found.contains(&argument) {
                    found.insert(parameter);
                }
            }
        }
        if found.len() == before {
            break;
        }
    }
    for &entity in &found {
        g.anchored(entity, anchors)?;
    }
    Ok(found)
}
fn protect(uses: &mut Uses, entities: &BTreeSet<EntityId>) {
    for &entity in entities {
        uses.protect(entity);
    }
}
pub(super) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    proof: &Proof,
    program: &Program,
    entry: FunctionId,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "P24 original module")?;
    need(
        function(module, i.entry)?.id == entry,
        "P24 returned entry is the original AST callable",
        entry,
    )?;
    let method = function(module, i.method_span)?;
    let g = Graph::new(method)?;
    let entry = g.block(method.entry_block().unwrap())?;
    let [receiver, source] = entry.parameters.as_slice() else {
        return Err(failure(
            "P24 actual receiver prefix plus one source slot",
            entry,
        ));
    };
    let receiver_type = method.entity(*receiver).unwrap().ty;
    let source_type = method.entity(*source).unwrap().ty;
    need(
        method.receiver == Some(receiver_type)
            && matches!(
                receiver_type,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    ..
                }
            )
            && matches!(source_type,EntityType::Loan{kind:LoanKind::Shared,target}if module.map_container(target).is_some_and(|(kind,k,v)|kind==MapContainerKind::Map&&module.type_kind(k)==Some(&SsaTypeKind::StringOwner)&&module.type_kind(v)==Some(&SsaTypeKind::StringOwner)))
            && receiver_type != source_type
            && method.return_types.is_empty()
            && method.borrow_return.is_none()
            && method.carrier_return.is_none(),
        "P24 actual instance Borrow receiver is distinct from explicit Borrow Map ABI",
        method,
    )?;
    let receivers = family(&g, &[*receiver])?;
    let sources = family(&g, &[*source])?;
    let mut uses = Uses::default();
    protect(&mut uses, &receivers);
    protect(&mut uses, &sources);
    if c.provider {
        provider::check(p, i, module, &g, &sources, &mut uses)?;
    } else {
        let (control, argument) = i
            .control
            .ok_or_else(|| failure("P24 actual original control Call", i))?;
        let call = at(&g, ast::span(p, control), |op| {
            matches!(op, Operation::DirectCall { .. })
        })?;
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments,
        } = &call.operation
        else {
            return Err(failure("P24 control observe ABI", call));
        };
        let [actual] = arguments.as_slice() else {
            return Err(failure("P24 actual one observe argument", call));
        };
        need(
            *callee == function(module, i.observe)?.id && g.blocks.len() == 1 && g.edges.is_empty(),
            "P24 exact straight-line original observe control",
            call,
        )?;
        if sources.contains(actual) {
            g.anchored(*actual, &[*source])?;
            uses.permit(*actual, call);
        } else {
            let begin = g.definition(*actual)?;
            let Operation::SharedReborrow { source: parent } = begin.operation else {
                return Err(failure("P24 independent control argument reborrow", begin));
            };
            need(
                sources.contains(&EntityId::Loan(parent))
                    && begin.origin == Origin::Source(ast::span(p, argument)),
                "P24 observe child borrows current original parameter",
                begin,
            )?;
            uses.permit(EntityId::Loan(parent), begin);
            uses.protect(*actual);
            uses.permit(*actual, call);
            let end=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==*actual)),"P24 unique control child End")?;
            need(
                end.block == call.block
                    && end.origin == Origin::Source(ast::span(p, control))
                    && g.position(begin)? < g.position(call)?
                    && g.position(call)? < g.position(end)?,
                "P24 observe CallReturn closes only its child loan",
                end,
            )?;
            uses.permit(*actual, end);
        }
        need(
            !g.instructions.iter().any(|op| {
                matches!(
                    op.operation,
                    Operation::MapIterationFirst { .. }
                        | Operation::MapIterationHasNext { .. }
                        | Operation::MapIterationKeyLoan { .. }
                        | Operation::MapIterationValueLoan { .. }
                        | Operation::MapIterationNext { .. }
                )
            }),
            "P24 control contains no provider",
            method.id,
        )?;
    }
    need(method.values.iter().all(|v|module.map_container(v.ty.semantic_type()).is_none())
        &&!g.instructions.iter().any(|op|matches!(op.operation,Operation::Drop{..}|Operation::HeapAllocate{..}|Operation::MapConstruct{..}|Operation::MapConsume{..}))
        &&!g.instructions.iter().any(|op|matches!(op.operation,Operation::BorrowEnd{loan}if sources.contains(&EntityId::Loan(loan))||receivers.contains(&EntityId::Loan(loan))))
        &&g.blocks.iter().all(|&block|!matches!(&g.term(block).unwrap().kind,TerminatorKind::Return{values}if !values.is_empty())),
        "P24 method never creates/drops caller owners or ends either original parameter family",method.id)?;
    uses.check(&g, None)?;
    caller(p, i, proof, module, method)
}
fn caller(
    p: &ParsedFile,
    i: &Input,
    proof: &Proof,
    module: &Module,
    method: &Function,
) -> Result<(), Failure> {
    let entry = function(module, i.entry)?;
    let g = Graph::new(entry)?;
    let make = at(&g, ast::span(p, i.source_birth), |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    need(
        make.operation
            == Operation::DirectCall {
                callee: function(module, i.make)?.id,
                receiver: None,
                arguments: vec![],
            },
        "P24 caller constructs original Map through actual make",
        make,
    )?;
    let map = value(make)?;
    let allocation = at(&g, ast::span(p, i.receiver_birth), |op| {
        matches!(op, Operation::HeapAllocate { .. })
    })?;
    let reader = value(allocation)?;
    need(
        g.position(allocation)? < g.position(make)?,
        "P24 original receiver and Map initializer evaluation order",
        (allocation, make),
    )?;
    let call = at(&g, ast::span(p, i.member_call), |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    let Operation::DirectCall {
        callee,
        receiver: Some(EntityId::Loan(receiver)),
        arguments,
    } = &call.operation
    else {
        return Err(failure("P24 actual caller member receiver operand", call));
    };
    let [EntityId::Loan(source)] = arguments.as_slice() else {
        return Err(failure("P24 member receives one real Map loan", arguments));
    };
    let parameters = &method
        .block(method.entry_block().unwrap())
        .unwrap()
        .parameters;
    need(
        *callee == method.id
            && call.results.is_empty()
            && entry.receiver.is_none()
            && g.blocks.len() == 1
            && g.edges.is_empty()
            && entry.entity(EntityId::Loan(*receiver)).unwrap().ty
                == method.entity(parameters[0]).unwrap().ty
            && entry.entity(EntityId::Loan(*source)).unwrap().ty
                == method.entity(parameters[1]).unwrap().ty,
        "P24 selected original method and independent receiver/source ABI operands",
        call,
    )?;
    let mut uses = Uses::default();
    let mut ends = Vec::new();
    for (loan, owner, origin) in [
        (*receiver, reader, proof.receiver_begin),
        (*source, map, ast::span(p, i.argument)),
    ] {
        let begin = g.definition(EntityId::Loan(loan))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return Err(failure("P24 actual Shared BorrowBegin", begin));
        };
        let root = g.definition(EntityId::Place(place))?;
        need(
            root.operation == Operation::RootPlace { owner }
                && root.origin == Origin::Source(origin)
                && begin.origin == Origin::Source(origin)
                && root.block == call.block
                && begin.block == call.block
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(call)?,
            "P24 member operand borrows its exact original allocated/factory owner",
            (root, begin, call),
        )?;
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::BorrowEnd { loan }),
            "P24 unique actual caller loan End",
        )?;
        let end_origin = if loan == *receiver {
            proof.receiver_end
        } else {
            ast::span(p, i.member_call)
        };
        need(
            end.block == call.block
                && end.origin == Origin::Source(end_origin)
                && g.position(call)? < g.position(end)?,
            "P24 actual synchronous member call ends both caller loans",
            end,
        )?;
        uses.protect(EntityId::Value(owner));
        uses.permit(EntityId::Value(owner), root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        uses.protect(EntityId::Loan(loan));
        uses.permit(EntityId::Loan(loan), call);
        uses.permit(EntityId::Loan(loan), end);
        ends.push(end);
    }
    for (owner, fact, birth) in [
        (map, proof.map_drop, make),
        (reader, proof.receiver_drop, allocation),
    ] {
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::Drop { owner }),
            "P24 original caller owner unique Drop",
        )?;
        need(
            drop.origin == Origin::Source(fact.value_origin())
                && drop.block == call.block
                && g.position(birth)? < g.position(call)?
                && ends
                    .iter()
                    .all(|end| g.position(end).unwrap() < g.position(drop).unwrap()),
            "P24 actual caller owner survives member call and both Ends before its published Drop",
            drop,
        )?;
        uses.permit(EntityId::Value(owner), drop);
    }
    need(g.instructions.iter().filter(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count()==1
        &&g.block(entry.entry_block().unwrap())?.parameters.is_empty()
        &&matches!(&g.term(call.block)?.kind,TerminatorKind::Return{values}if values.is_empty()),"P24 caller keeps sole actual Map cleanup and void Return",entry.id)?;
    uses.check(&g, None)
}
