//! Real read/operand Borrow consumers and zero-argument scanners' actual 7/0 caller checks.
use super::{
    super::{
        fixture::{Failure, contains, failure, need},
        query_graph::{Graph, Uses, function, loan, result, value},
        query_inputs::one,
    },
    ast::{Exit, Input},
    query_body::same_value,
    query_inputs::{CallerInput, Inputs},
    query_ssa::{condition_uses, full_graph, ty},
};
use crate::ssa::model::*;
use lang_frontend::{parser::Expression, source::Span};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn signature(
    f: &Function,
    expected: &[EntityType],
    returns: &[SsaTypeId],
) -> Result<(), Failure> {
    let entry = f
        .entry_block()
        .and_then(|b| f.block(b))
        .ok_or_else(|| failure("temporary exits SSA callable actual entry", f.id))?;
    need(
        f.receiver.is_none()
            && f.borrow_return.is_none()
            && f.carrier_return.is_none()
            && f.return_types == returns
            && entry.origin == f.origin
            && entry.parameters.len() == expected.len()
            && entry
                .parameters
                .iter()
                .zip(expected)
                .enumerate()
                .all(|(index, (&p, t))| {
                    f.entity(p).is_some_and(|e| {
                        e.ty == *t
                            && e.origin == f.origin
                            && e.definition
                                == Definition::BlockParameter {
                                    block: entry.id,
                                    index,
                                }
                    })
                }),
        "temporary exits SSA actual callable signature and parameter definitions",
        f,
    )
}
fn int(module: &Module, t: SsaTypeId) -> bool {
    module.type_kind(t)
        == Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        })
}

pub(super) fn zero_return(
    g: &Graph<'_>,
    f: &Input,
    block: BlockId,
    scope: Span,
) -> Result<(), Failure> {
    let term = g.term(block)?;
    let TerminatorKind::Return { values } = &term.kind else {
        return Err(failure("temporary exits SSA exhausted actual Return", term));
    };
    let returned = one(
        values.iter().copied(),
        "temporary exits SSA exhausted one Int result",
    )?;
    let zero = one(
        g.ops(block)?.into_iter().filter(|i| {
            i.operation == Operation::Constant(ScalarConstant::Integer(0))
                && f.sources.slice(i.origin.span()) == Ok("0")
        }),
        "temporary exits SSA actual source zero literal",
    )?;
    need(
        contains(scope, term.origin.span())
            && contains(term.origin.span(), zero.origin.span())
            && g.position(zero)? >= 2,
        "temporary exits SSA exhausted zero evaluated after full source cleanup",
        (zero, term),
    )?;
    let mut uses = Uses::default();
    let mut protected = BTreeSet::new();
    same_value(
        g,
        returned,
        value(zero)?,
        zero.origin.span(),
        &mut uses,
        &mut protected,
        &mut BTreeSet::new(),
    )?;
    uses.check(g, Some((block, returned)))
}

fn read(module: &Module, f: &Input, input: &Inputs) -> Result<EntityType, Failure> {
    let read = function(module, input.read)?;
    let g = Graph::new(read)?;
    full_graph(&g)?;
    let block = one(
        g.blocks.iter().copied(),
        "temporary exits SSA read single actual block",
    )?;
    let parameters = &g.block(block)?.parameters;
    need(
        parameters.len() == 2 && parameters[0] != parameters[1],
        "temporary exits SSA two distinct read Borrow parameters",
        read,
    )?;
    let shared = ty(&g, parameters[0])?;
    need(
        matches!(shared,EntityType::Loan{kind:LoanKind::Shared,target} if module.type_kind(target)==Some(&SsaTypeKind::StringOwner)),
        "temporary exits SSA read Shared String type",
        shared,
    )?;
    signature(read, &[shared, shared], &[])?;
    let mut calls = f
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            matches!(n.payload(), Expression::Call { .. })
                .then_some(n.span())
                .filter(|s| contains(input.read, *s))
        })
        .collect::<Vec<_>>();
    calls.sort_by_key(|s| s.start());
    need(
        calls.len() == 2 && g.instructions.len() == 2,
        "temporary exits SSA actual two source read consumers",
        &calls,
    )?;
    let mut uses = Uses::default();
    for (index, &parameter) in parameters.iter().enumerate() {
        let EntityId::Loan(l) = parameter else {
            return Err(failure("temporary exits SSA Borrow parameter", parameter));
        };
        let i = g.ops(block)?[index];
        need(
            i.operation == Operation::PrintString { value: l }
                && i.origin == Origin::Source(calls[index])
                && i.results.is_empty(),
            "temporary exits SSA each read consumes its own actual parameter",
            i,
        )?;
        uses.protect(parameter);
        uses.permit(parameter, i);
    }
    need(
        matches!(&g.term(block)?.kind,TerminatorKind::Return{values} if values.is_empty())
            && g.term(block)?.origin == Origin::Source(input.read),
        "temporary exits SSA read actual Unit return",
        g.term(block)?,
    )?;
    uses.check(&g, None)?;
    Ok(shared)
}

fn operand(module: &Module, f: &Input, input: &Inputs, shared: EntityType) -> Result<(), Failure> {
    let Some(anchor) = &input.operand else {
        return Ok(());
    };
    let operand = function(module, anchor.function)?;
    let t = *one(
        operand.return_types.iter(),
        "temporary exits SSA operand sole result type",
    )?;
    need(int(module, t), "temporary exits SSA actual operand Int", t)?;
    signature(operand, &[shared, shared], &[t])?;
    let g = Graph::new(operand)?;
    full_graph(&g)?;
    let block = one(
        g.blocks.iter().copied(),
        "temporary exits SSA actual operand block",
    )?;
    let read = function(module, input.read)?;
    let call = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::DirectCall { .. })),
        "temporary exits SSA actual operand's sole read Call",
    )?;
    let mut roots = BTreeMap::new();
    let mut live = BTreeSet::new();
    let mut uses = Uses::default();
    let mut consumed = false;
    for (index, &p) in g.block(block)?.parameters.iter().enumerate() {
        let EntityId::Loan(l) = p else {
            return Err(failure("temporary exits SSA operand Borrow parameter", p));
        };
        roots.insert(l, (index, None));
        live.insert(l);
        uses.protect(p);
    }
    for i in g.ops(block)? {
        match &i.operation {
            Operation::SharedReborrow { source } => {
                let (index, _) = *roots.get(source).ok_or_else(|| {
                    failure("temporary exits SSA operand reborrow actual parent", i)
                })?;
                let l = loan(i)?;
                need(
                    !consumed
                        && live.contains(source)
                        && ty(&g, EntityId::Loan(l))? == shared
                        && i.origin == Origin::Source(anchor.arguments[index])
                        && roots.insert(l, (index, Some(*source))).is_none()
                        && live.insert(l),
                    "temporary exits SSA operand actual derived argument loan",
                    i,
                )?;
                uses.permit(EntityId::Loan(*source), i);
                uses.protect(EntityId::Loan(l));
            }
            Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } => {
                need(
                    !consumed
                        && i.id == call.id
                        && *callee == read.id
                        && receiver.is_none()
                        && arguments.len() == 2
                        && i.results.is_empty()
                        && i.origin == Origin::Source(anchor.call),
                    "temporary exits SSA operand actual read target/origin",
                    i,
                )?;
                for (index, &a) in arguments.iter().enumerate() {
                    let EntityId::Loan(l) = a else {
                        return Err(failure(
                            "temporary exits SSA operand Borrow actual argument",
                            a,
                        ));
                    };
                    need(
                        live.contains(&l) && roots.get(&l).is_some_and(|(root, _)| *root == index),
                        "temporary exits SSA operand actual K/V slots",
                        i,
                    )?;
                    uses.permit(a, i);
                }
                consumed = true;
            }
            Operation::BorrowEnd { loan } if roots.contains_key(loan) => {
                need(
                    consumed
                        && roots.get(loan).is_some_and(|(_, parent)| parent.is_some())
                        && live.remove(loan)
                        && !roots.iter().any(|(child, (_, parent))| {
                            *parent == Some(*loan) && live.contains(child)
                        })
                        && i.origin == Origin::Source(anchor.call)
                        && i.results.is_empty(),
                    "temporary exits SSA operand derived loan ending after actual call",
                    i,
                )?;
                uses.permit(EntityId::Loan(*loan), i);
            }
            _ => {}
        }
    }
    need(
        consumed
            && live.len() == 2
            && live
                .iter()
                .all(|l| roots.get(l).is_some_and(|(_, parent)| parent.is_none())),
        "temporary exits SSA operand all derived calls complete and Borrow params not ended",
        (&live, &roots),
    )?;
    let seven = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Constant(ScalarConstant::Integer(7))),
        "temporary exits SSA operand actual seven birth",
    )?;
    let TerminatorKind::Return { values } = &g.term(block)?.kind else {
        return Err(failure(
            "temporary exits SSA operand actual Return",
            g.term(block)?,
        ));
    };
    let returned = one(
        values.iter().copied(),
        "temporary exits SSA operand sole returned value",
    )?;
    need(
        f.sources.slice(seven.origin.span()) == Ok("7")
            && ty(&g, result(seven)?)? == EntityType::Value(t)
            && g.position(call)? < g.position(seven)?
            && contains(anchor.function, g.term(block)?.origin.span())
            && contains(g.term(block)?.origin.span(), seven.origin.span()),
        "temporary exits SSA operand source seven after read before Return",
        seven,
    )?;
    same_value(
        &g,
        returned,
        value(seven)?,
        seven.origin.span(),
        &mut uses,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    )?;
    uses.check(&g, Some((block, returned)))
}

#[derive(Debug)]
struct CallerShape {
    pre: BlockId,
    after: BlockId,
    edges: Vec<(BlockId, BlockId)>,
    // Current actual scalar parameter and its actual scanner Call birth.
    carried: Vec<(ValueId, ValueId)>,
}

fn caller_edge(
    g: &Graph<'_>,
    edge: &Edge,
    carried: &[(ValueId, ValueId)],
    uses: &mut Uses,
) -> Result<Vec<(ValueId, ValueId)>, Failure> {
    let expected = carried
        .iter()
        .map(|(current, _)| EntityId::Value(*current))
        .collect::<BTreeSet<_>>();
    need(
        expected.len() == carried.len()
            && edge.arguments.len() == expected.len()
            && edge.arguments.iter().copied().collect::<BTreeSet<_>>() == expected,
        "temporary exits SSA caller exact complete current scalar edge",
        (edge, carried),
    )?;
    let target = g.block(edge.target)?;
    let mut output = Vec::new();
    for &(current, birth) in carried {
        let EntityId::Value(next) = g.transported(edge, EntityId::Value(current))? else {
            return Err(failure(
                "temporary exits SSA caller actual scalar parameter",
                edge,
            ));
        };
        need(
            g.function
                .entity(EntityId::Value(next))
                .is_some_and(|e| e.origin == target.origin),
            "temporary exits SSA caller scalar parameter actual block origin",
            next,
        )?;
        same_value(
            g,
            next,
            birth,
            g.definition(EntityId::Value(birth))?.origin.span(),
            uses,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
        )?;
        output.push((next, birth));
    }
    Ok(output)
}

struct Caller<'a, 'ir> {
    graph: &'a Graph<'ir>,
    module: &'ir Module,
    f: &'a Input,
    unit: bool,
}
impl Caller<'_, '_> {
    fn check(
        &self,
        input: &CallerInput,
        expected: i128,
        prior: &[(ValueId, ValueId)],
        uses: &mut Uses,
    ) -> Result<CallerShape, Failure> {
        let (g, module, f, unit) = (self.graph, self.module, self.f, self.unit);
        let scan = function(module, input.function)?;
        let call = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation,Operation::DirectCall{callee,..} if callee==scan.id),
            ),
            "temporary exits SSA actual unique caller scanner call",
        )?;
        need(
            call.origin == Origin::Source(input.call)
                && matches!(&call.operation,Operation::DirectCall{receiver:None,arguments,..} if arguments.is_empty())
                && matches!(ty(g,result(call)?)?,EntityType::Value(t) if int(module,t)&&scan.return_types.as_slice()==[t]),
            "temporary exits SSA real zero-argument scanner caller result",
            call,
        )?;
        let (origin, condition_span, then_span, else_span) = one(
            f.parsed.ast().expressions().iter().filter_map(|(_, n)| {
                let Expression::If {
                    condition,
                    then_branch,
                    else_branch: Some(otherwise),
                    ..
                } = n.payload()
                else {
                    return None;
                };
                let condition = f.parsed.ast().expressions().get(*condition).ok()?;
                if !contains(condition.span(), input.returned)
                    || !contains(condition.span(), input.expected)
                {
                    return None;
                }
                Some((
                    n.span(),
                    condition.span(),
                    f.parsed.ast().statements().get(*then_branch).ok()?.span(),
                    f.parsed.ast().statements().get(*otherwise).ok()?.span(),
                ))
            }),
            "temporary exits SSA caller exact source 7/0 If",
        )?;
        let term = g.term(call.block)?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &term.kind
        else {
            return Err(failure(
                "temporary exits SSA caller actual conditional",
                term,
            ));
        };
        let cmp = g.definition(EntityId::Value(*condition))?;
        let Operation::Compare {
            operator: ComparisonOperator::Equal,
            left,
            right,
        } = cmp.operation
        else {
            return Err(failure("temporary exits SSA caller actual equality", cmp));
        };
        let literal = g.definition(EntityId::Value(right))?;
        need(
            term.origin == Origin::Source(origin)
                && cmp.origin == Origin::Source(condition_span)
                && cmp.block == call.block
                && literal.block == call.block
                && literal.origin == Origin::Source(input.expected)
                && literal.operation == Operation::Constant(ScalarConstant::Integer(expected))
                && f.sources
                    .slice(input.expected)
                    .is_ok_and(|s| s == expected.to_string())
                && g.position(call)? < g.position(cmp)?
                && g.position(literal)? < g.position(cmp)?,
            "temporary exits SSA actual caller source equality to expected literal",
            (call, cmp, literal, term),
        )?;
        let mut protected = BTreeSet::new();
        same_value(
            g,
            left,
            value(call)?,
            input.returned,
            uses,
            &mut protected,
            &mut BTreeSet::new(),
        )?;
        uses.permit(EntityId::Value(left), cmp);
        uses.protect(EntityId::Value(right));
        uses.permit(EntityId::Value(right), cmp);
        condition_uses(g, call.block, *condition)?;
        let (a, b) = (when_true.target, when_false.target);
        need(
            a != b
                && a != call.block
                && b != call.block
                && g.block(a)?.origin == Origin::Source(then_span)
                && g.block(b)?.origin == Origin::Source(else_span),
            "temporary exits SSA caller actual two source If arms",
            term,
        )?;
        g.incoming_from(a, &[call.block])?;
        g.incoming_from(b, &[call.block])?;
        let mut current = prior.to_vec();
        if unit {
            current.push((value(call)?, value(call)?));
        } else {
            need(
                prior.is_empty(),
                "temporary exits SSA single caller carries no implicit local columns",
                prior,
            )?;
        }
        let left_roles = caller_edge(g, when_true, &current, uses)?;
        let right_roles = caller_edge(g, when_false, &current, uses)?;
        let ea = g.branch(a)?;
        let eb = g.branch(b)?;
        need(
            ea.target == eb.target
                && ![a, b, call.block].contains(&ea.target)
                && g.block(ea.target)?.origin == Origin::Source(origin)
                && g.term(a)?.origin == Origin::Source(origin)
                && g.term(b)?.origin == Origin::Source(origin),
            "temporary exits SSA caller actual common result join",
            (ea, eb),
        )?;
        g.incoming_from(ea.target, &[a, b])?;
        let joined = caller_edge(g, ea, &left_roles, uses)?;
        let other_joined = caller_edge(g, eb, &right_roles, uses)?;
        need(
            joined == other_joined,
            "temporary exits SSA caller every incoming preserves each original scanner result",
            (&joined, &other_joined),
        )?;
        Ok(CallerShape {
            pre: call.block,
            after: ea.target,
            carried: joined,
            edges: vec![
                (call.block, a),
                (call.block, b),
                (a, ea.target),
                (b, eb.target),
            ],
        })
    }
}

pub(super) fn check(module: &Module, f: &Input, input: &Inputs) -> Result<(), Failure> {
    let shared = read(module, f, input)?;
    operand(module, f, input, shared)?;
    if f.exit != Exit::Return {
        return need(
            input.operand.is_none() && input.callers.is_empty(),
            "temporary exits SSA no synthetic Return caller",
            input,
        );
    }
    let [a, b] = input.callers.as_slice() else {
        return Err(failure(
            "temporary exits SSA exactly two actual callers",
            input,
        ));
    };
    need(
        a.function == input.sites[0].function
            && b.function == input.sites[1].function
            && a.call != b.call,
        "temporary exits SSA each caller targets its own actual scanner",
        input,
    )?;
    let entry = function(module, input.entry)?;
    signature(entry, &[], &[])?;
    let g = Graph::new(entry)?;
    full_graph(&g)?;
    let mut uses = Uses::default();
    let unit = input.unit.is_some();
    let caller = Caller {
        graph: &g,
        module,
        f,
        unit,
    };
    let first = caller.check(a, 7, &[], &mut uses)?;
    let second = caller.check(b, 0, &first.carried, &mut uses)?;
    need(
        Some(first.pre) == entry.entry_block() && first.after == second.pre,
        "temporary exits SSA sequential actual caller checks",
        (&first, &second),
    )?;
    g.incoming_from(first.pre, &[])?;
    let mut edges = first.edges;
    edges.extend(second.edges);
    let actual = g
        .edges
        .iter()
        .map(|(from, e)| (*from, e.target))
        .collect::<Vec<_>>();
    need(
        g.blocks.len() == 7
            && actual.len() == edges.len()
            && actual.iter().copied().collect::<BTreeSet<_>>() == edges.iter().copied().collect()
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                .count()
                == 2
            && matches!(&g.term(second.after)?.kind,TerminatorKind::Return{values} if values.is_empty())
            && g.term(second.after)?.origin == Origin::Source(input.entry),
        "temporary exits SSA entire caller graph/calls/return closed",
        entry,
    )?;
    uses.check(&g, None)
}
