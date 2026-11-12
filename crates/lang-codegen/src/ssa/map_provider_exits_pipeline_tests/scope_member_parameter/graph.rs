//! The hidden receiver and explicit source keep independent entry/caller identities.
mod provider {
    //! Complete P24 method provider columns; the normal body reuses the existing K/V checker.
    use super::super::super::query_map_normal_body;
    use super::*;
    pub(super) fn check(
        p: &ParsedFile,
        i: &Input,
        module: &Module,
        g: &Graph<'_>,
        sources: &BTreeSet<EntityId>,
        uses: &mut Uses,
    ) -> Result<(), Failure> {
        let site = i
            .site
            .as_ref()
            .ok_or_else(|| failure("P24 original method provider site", i))?;
        let first = at(g, site.span, |op| {
            matches!(op, Operation::MapIterationFirst { .. })
        })?;
        let has = at(g, site.span, |op| {
            matches!(op, Operation::MapIterationHasNext { .. })
        })?;
        let key = at(g, site.span, |op| {
            matches!(op, Operation::MapIterationKeyLoan { .. })
        })?;
        let next = at(g, site.span, |op| {
            matches!(op, Operation::MapIterationNext { .. })
        })?;
        let Operation::MapIterationFirst { source } = first.operation else {
            unreachable!()
        };
        let begin = g.definition(EntityId::Loan(source))?;
        let Operation::SharedReborrow { source: parent } = begin.operation else {
            return Err(failure("P24 source lease is a real reborrow", begin));
        };
        need(
            sources.contains(&EntityId::Loan(parent))
                && begin.origin == Origin::Source(site.span)
                && begin.block == first.block
                && g.position(begin)? < g.position(first)?,
            "P24 provider lease descends from actual method Map parameter",
            begin,
        )?;
        uses.permit(EntityId::Loan(parent), begin);
        let leases = family(g, &[EntityId::Loan(source)])?;
        let cursors = family(g, &[result(first)?, result(next)?])?;
        protect(uses, &leases);
        protect(uses, &cursors);
        let Operation::MapIterationHasNext {
            source: hs,
            cursor: hc,
        } = has.operation
        else {
            unreachable!()
        };
        let Operation::MapIterationKeyLoan {
            source: bs,
            cursor: bc,
        } = key.operation
        else {
            unreachable!()
        };
        let Operation::MapIterationNext {
            source: ns,
            cursor: nc,
        } = next.operation
        else {
            unreachable!()
        };
        let entered = g.branch(first.block)?;
        let back = g.branch(next.block)?;
        need(
            entered.target == has.block
                && back.target == has.block
                && next.block == key.block
                && g.argument(entered, EntityId::Loan(hs))? == EntityId::Loan(source)
                && g.argument(entered, EntityId::Value(hc))? == result(first)?
                && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(ns)
                && g.argument(back, EntityId::Value(hc))? == result(next)?
                && [hs, bs, ns]
                    .iter()
                    .all(|loan| leases.contains(&EntityId::Loan(*loan)))
                && [hc, bc, nc]
                    .iter()
                    .all(|cursor| cursors.contains(&EntityId::Value(*cursor))),
            "P24 actual provider preheader and backedge use current independent source/cursor columns",
            (entered, back),
        )?;
        g.incoming_from(has.block, &[first.block, next.block])?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &g.term(has.block)?.kind
        else {
            return Err(failure("P24 actual provider guard", has));
        };
        need(
            *condition == value(has)?
                && when_true.target == key.block
                && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
            "P24 true guard transports exact current source/cursor",
            has,
        )?;
        g.incoming_from(key.block, &[has.block])?;
        g.incoming_from(when_false.target, &[has.block])?;
        let exit = g.transported(when_false, EntityId::Loan(hs))?;
        let end=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if leases.contains(&EntityId::Loan(loan)))),"P24 sole provider source lease End")?;
        need(
            end.block == when_false.target
                && matches!(end.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==exit)
                && end.origin == Origin::Source(site.span)
                && g.ops(end.block)?.first().map(|op| op.id) == Some(end.id)
                && g.blocks.iter().copied().collect::<BTreeSet<_>>()
                    == BTreeSet::from([first.block, has.block, key.block, end.block]),
            "P24 exhaustion ends only actual current provider lease in complete four-block CFG",
            end,
        )?;
        let (_, key_type, value_type) = module
            .map_container(
                g.function
                    .entity(EntityId::Loan(source))
                    .unwrap()
                    .ty
                    .semantic_type(),
            )
            .ok_or_else(|| failure("P24 actual provider Map type", source))?;
        let checked = query_map_normal_body::check(
            g,
            function(module, i.reader)?,
            query_map_normal_body::Body {
                block: key.block,
                span: site.span,
                read: ast::span(p, site.read),
                arguments: [
                    ast::span(p, site.arguments[0]),
                    ast::span(p, site.arguments[1]),
                ],
                types: [key_type, value_type],
            },
            bs,
            bc,
            uses,
        )?;
        need(
            checked.id == next.id,
            "P24 original normal body ends both components before this exact Next",
            checked,
        )?;
        for op in [first, has, end] {
            for entity in op.operation.entities() {
                uses.permit(entity, op);
            }
        }
        let actual_maps = g
            .function
            .loans
            .iter()
            .enumerate()
            .filter_map(|(index, data)| {
                module
                    .map_container(data.ty.semantic_type())
                    .is_some()
                    .then_some(EntityId::Loan(LoanId {
                        function: g.function.id,
                        index,
                    }))
            })
            .collect::<BTreeSet<_>>();
        need(
            actual_maps == sources.union(&leases).copied().collect(),
            "P24 all actual method Map loans belong to original parameter or its provider lease",
            actual_maps,
        )
    }
}
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

pub(in crate::ssa) mod continuation {
//! Independent size-result, condition, both-arm and current-owner continuation proof.
use crate::ssa::map_provider_exits_pipeline_tests::future_use::graph::*;

// These are the original AST/SSA products already held by the parent query;
// keeping the roles explicit makes the cross-product identity checks reviewable.
#[allow(clippy::too_many_arguments)]
pub(in crate::ssa) fn check(
    g: &Graph<'_>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    scan: Span,
    form: usize,
    sizes: &[&Instruction],
    calls: &[(&Instruction, &Instruction)],
    uses: &mut Uses,
) -> Result<(), Failure> {
    let conditions = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            let Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } = node.payload()
            else {
                return None;
            };
            inside(node.span(), scan).then_some((
                node.span(),
                *condition,
                *then_branch,
                *else_branch,
            ))
        })
        .collect::<Vec<_>>();
    if form == 1 || form == 3 {
        need(
            conditions.is_empty() && sizes.is_empty(),
            "call/repeat form has no invented size condition",
            &conditions,
        )?;
        need(
            calls.len() == if form == 1 { 2 } else { 0 },
            "call/repeat original reachable Borrow count",
            calls,
        )?;
        return Ok(());
    }
    let [(if_span, condition, then_branch, Some(else_branch))] = conditions.as_slice() else {
        return Err(failure(
            "one original size If with both real arms",
            &conditions,
        ));
    };
    let comparison = parsed.ast().expressions().get(*condition).unwrap();
    let Expression::Binary {
        left,
        operator,
        right,
        ..
    } = comparison.payload()
    else {
        return Err(failure("original size equality AST", comparison));
    };
    need(
        *operator == lang_frontend::parser::BinaryOperator::Equal,
        "original size condition is equality",
        comparison,
    )?;
    let zero_ast = parsed.ast().expressions().get(*right).unwrap();
    need(
        sources.slice(zero_ast.span()) == Ok("0"),
        "original right operand zero",
        zero_ast,
    )?;
    let [size] = sizes else {
        return Err(failure("one actual source size result", sizes));
    };
    let left_ast = parsed.ast().expressions().get(*left).unwrap();
    if form == 0 {
        need(
            left_ast.payload() == &Expression::Name
                && sources.slice(left_ast.span()) == Ok("count"),
            "later condition reads original count binding",
            left_ast,
        )?;
        let initializer = one(
            parsed.ast().items().iter().filter_map(|(_, node)| {
                let Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                (inside(node.span(), scan) && sources.slice(*name) == Ok("count"))
                    .then_some(*initializer)
            }),
            "original count declaration initializer",
        )?;
        let initializer = parsed.ast().expressions().get(initializer).unwrap();
        need(
            sources.slice(initializer.span()) == Ok("source.size")
                && size.origin == Origin::Source(initializer.span()),
            "count stores exact actual source.size result",
            size,
        )?;
        need(
            calls.is_empty(),
            "count-only form has no fabricated future Borrow",
            calls,
        )?;
    } else {
        need(
            form == 2 && calls.len() == 1,
            "CFG form has one real future Borrow",
            calls,
        )?;
        need(
            sources.slice(left_ast.span()) == Ok("source.size")
                && size.origin == Origin::Source(left_ast.span()),
            "condition reads exact original source.size",
            size,
        )?;
    }
    let zero = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(zero_ast.span())
                && i.operation == Operation::Constant(ScalarConstant::Integer(0))
        }),
        "actual original zero constant",
    )?;
    let compare = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(comparison.span())
                && i.operation
                    == Operation::Compare {
                        operator: ComparisonOperator::Equal,
                        left: value(size).unwrap(),
                        right: value(zero).unwrap(),
                    }
        }),
        "actual exact size/zero equality",
    )?;
    need(
        size.block == compare.block
            && zero.block == compare.block
            && g.position(size)? < g.position(zero)?
            && g.position(zero)? < g.position(compare)?,
        "actual size then original zero then equality",
        (size, zero, compare),
    )?;
    for i in [*size, zero] {
        need(g.function.entity(result(i)?).is_some_and(|e| matches!(e.ty, EntityType::Value(t) if module.type_kind(t) == Some(&SsaTypeKind::Integer {bits:32,signed:true}))), "size and original zero use Int storage", i)?;
        uses.protect(result(i)?);
        uses.permit(result(i)?, compare);
    }
    need(g.function.entity(result(compare)?).is_some_and(|e| matches!(e.ty,EntityType::Value(t) if module.type_kind(t)==Some(&SsaTypeKind::Boolean))), "comparison actual Boolean storage", compare)?;
    let term = g.term(compare.block)?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &term.kind
    else {
        return Err(failure("original If actual Conditional", term));
    };
    need(
        *condition == value(compare)?
            && term.origin == Origin::Source(*if_span)
            && when_true.target != when_false.target,
        "exact original If Boolean/Conditional origin",
        term,
    )?;
    control_use(g, compare)?;
    g.incoming_from(when_true.target, &[compare.block])?;
    g.incoming_from(when_false.target, &[compare.block])?;
    arm(
        g,
        sources,
        parsed,
        *then_branch,
        when_true.target,
        "zero",
        uses,
    )?;
    arm(
        g,
        sources,
        parsed,
        *else_branch,
        when_false.target,
        "nonzero",
        uses,
    )?;
    let true_exit = g.branch(when_true.target)?;
    let false_exit = g.branch(when_false.target)?;
    let merge = true_exit.target;
    need(
        false_exit.target == merge
            && g.term(when_true.target)?.origin == Origin::Source(*if_span)
            && g.term(when_false.target)?.origin == Origin::Source(*if_span),
        "both actual source arms merge at original If",
        (true_exit, false_exit),
    )?;
    g.incoming_from(merge, &[when_true.target, when_false.target])?;
    if form == 2 {
        let Operation::MapSize { owner: current } = size.operation else {
            unreachable!()
        };
        let then_owner = g.transported(when_true, current)?;
        let else_owner = g.transported(when_false, current)?;
        let merged_owner = g.transported(true_exit, then_owner)?;
        need(
            g.argument(false_exit, merged_owner)? == else_owner,
            "all current-owner If merge columns match actual size owner",
            (then_owner, else_owner, merged_owner),
        )?;
        let (call, end) = calls[0];
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        let [argument] = arguments.as_slice() else {
            return Err(failure("postmerge actual Borrow argument", call));
        };
        let birth = g.definition(*argument)?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("postmerge actual source loan", birth));
        };
        let root = g.definition(EntityId::Place(place))?;
        need(
            root.operation
                == match merged_owner {
                    EntityId::Value(owner) => Operation::RootPlace { owner },
                    _ => return Err(failure("actual merged owner Value", merged_owner)),
                }
                && root.block == merge
                && birth.block == merge
                && call.block == merge
                && end.block == merge,
            "actual later Borrow reads rebound owner only after complete merge",
            (root, birth, call, end),
        )?;
    } else {
        // count's source owner was consumed before this If; no Map/loan column
        // may silently survive into either arm or their merge.
        for block in [when_true.target, when_false.target, merge] {
            need(
                g.block(block)?.parameters.iter().all(|entity| {
                    !matches!(
                        module.type_kind(g.function.entity(*entity).unwrap().ty.semantic_type()),
                        Some(SsaTypeKind::MapContainer { .. })
                    )
                }),
                "count-only If carries no residual source owner/loan",
                g.block(block)?,
            )?;
        }
    }
    Ok(())
}

// Uses::check accounts for data operands and edge columns. A Boolean control
// result has exactly one different legal use: this precise Conditional field.
fn control_use(g: &Graph<'_>, compare: &Instruction) -> Result<(), Failure> {
    let entity = result(compare)?;
    let boolean = value(compare)?;
    need(g.instructions.iter().all(|i| !i.operation.entities().contains(&entity))
        && g.edges.iter().all(|(_, edge)| !edge.arguments.contains(&entity))
        && g.blocks.iter().all(|&block| match g.term(block) {
            Ok(term) if block == compare.block => matches!(term.kind,TerminatorKind::Conditional {condition,..} if condition==boolean),
            Ok(term) => !term.kind.entities().contains(&entity),
            Err(_) => false,
        }), "complete Boolean uses are exactly original size Conditional", compare)
}

fn arm(
    g: &Graph<'_>,
    sources: &SourceMap,
    parsed: &ParsedFile,
    body: lang_frontend::ast::StatementId,
    block: BlockId,
    expected: &str,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let span = parsed.ast().statements().get(body).unwrap().span();
    let (call_span, argument_span) = one(
        parsed.ast().expressions().iter().filter_map(|(_, node)| {
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                return None;
            };
            if !inside(node.span(), span)
                || sources.slice(parsed.ast().expressions().get(*callee).unwrap().span())
                    != Ok("println")
            {
                return None;
            }
            let [argument] = arguments.as_slice() else {
                return None;
            };
            Some((
                node.span(),
                parsed
                    .ast()
                    .expressions()
                    .get(argument.value)
                    .unwrap()
                    .span(),
            ))
        }),
        "original actual arm println",
    )?;
    let raw = sources
        .slice(argument_span)
        .map_err(|e| failure("original arm literal", e))?;
    need(
        raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) == Some(expected),
        "original zero/nonzero branch bytes",
        raw,
    )?;
    let print = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(call_span)
                && matches!(
                    i.operation,
                    Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                )
        }),
        "actual original arm print",
    )?;
    need(
        print.block == block,
        "actual branch successor owns its original marker",
        print,
    )?;
    match &print.operation {
        Operation::PrintLiteral { bytes } => {
            let mut expected = expected.as_bytes().to_vec();
            expected.push(b'\n');
            need(bytes == &expected, "actual arm literal output bytes", print)?;
        }
        Operation::PrintString { value: loan } => {
            let birth = g.definition(EntityId::Loan(*loan))?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = birth.operation
            else {
                return Err(failure("arm actual String view", birth));
            };
            let root = g.definition(EntityId::Place(place))?;
            let Operation::RootPlace { owner } = root.operation else {
                return Err(failure("arm actual String root", root));
            };
            let literal = g.definition(EntityId::Value(owner))?;
            need(
                literal.origin == Origin::Source(argument_span)
                    && matches!(&literal.operation,Operation::StringLiteral {bytes,..} if bytes==expected.as_bytes()),
                "actual selected arm String bytes and original literal",
                literal,
            )?;
            let end = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: *loan }),
                "actual arm String loan End",
            )?;
            let drop = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::Drop { owner }),
                "actual arm String owner Drop",
            )?;
            need(
                [literal, root, birth, end, drop]
                    .iter()
                    .all(|i| i.block == block)
                    && g.position(literal)? < g.position(root)?
                    && g.position(root)? < g.position(birth)?
                    && g.position(birth)? < g.position(print)?
                    && g.position(print)? < g.position(end)?
                    && g.position(end)? < g.position(drop)?,
                "actual arm String creation/read/End/Drop sequence",
                print,
            )?;
            uses.protect(EntityId::Value(owner));
            uses.permit(EntityId::Value(owner), root);
            uses.permit(EntityId::Value(owner), drop);
            uses.protect(EntityId::Place(place));
            uses.permit(EntityId::Place(place), birth);
            uses.protect(EntityId::Loan(*loan));
            uses.permit(EntityId::Loan(*loan), print);
            uses.permit(EntityId::Loan(*loan), end);
        }
        _ => unreachable!(),
    }
    Ok(())
}
}
