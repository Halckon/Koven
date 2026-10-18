//! 每 site 的 K/V、可选派生 Borrow 与 read/End/Next 实际 operand 闭合。
use super::{
    super::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, loan, result, value},
        query_inputs::one,
    },
    query_inputs::SiteInput,
    query_ssa::Roles,
};
use crate::ssa::model::*;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Body<'a, 'ir> {
    pub graph: &'a Graph<'ir>,
    pub read: &'ir Function,
    pub module: &'ir Module,
    pub site: &'a SiteInput,
    pub block: BlockId,
    pub roles: Roles,
}

impl Body<'_, '_> {
    pub(super) fn check(
        &self,
        uses: &mut Uses,
        protected: &mut BTreeSet<EntityId>,
    ) -> Result<ValueId, Failure> {
        let (graph, site, roles) = (self.graph, self.site, self.roles);
        let ops = graph.ops(self.block)?;
        let key = one(
            ops.iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. })),
            "temporary SSA unique actual KeyLoan",
        )?;
        let val = one(
            ops.iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
            "temporary SSA unique actual ValueLoan",
        )?;
        let next = one(
            ops.iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
            "temporary SSA unique actual Next",
        )?;
        need(
            key.operation
                == Operation::MapIterationKeyLoan {
                    source: roles.source,
                    cursor: roles.cursor,
                }
                && val.operation
                    == Operation::MapIterationValueLoan {
                        source: roles.source,
                        cursor: roles.cursor,
                    }
                && next.operation
                    == Operation::MapIterationNext {
                        source: roles.source,
                        cursor: roles.cursor,
                    }
                && [key, val, next]
                    .iter()
                    .all(|i| i.origin == Origin::Source(site.span)),
            "temporary SSA K/V/Next use this current source and cursor",
            (key, val, next, roles),
        )?;
        let (k, v) = (loan(key)?, loan(val)?);
        need(
            k != v
                && [k, v].iter().all(|l| {
                    graph.function.entity(EntityId::Loan(*l)).is_some_and(|e| {
                        matches!(e.ty, EntityType::Loan { kind: LoanKind::Shared, target }
                    if self.module.type_kind(target) == Some(&SsaTypeKind::StringOwner))
                    })
                }),
            "temporary SSA distinct K/V Shared String result identities",
            (key, val),
        )?;
        let read_entry = self
            .read
            .entry_block()
            .and_then(|id| self.read.block(id))
            .ok_or_else(|| failure("temporary SSA actual read entry", self.read.id))?;
        need(
            self.read.receiver.is_none()
                && self.read.borrow_return.is_none()
                && self.read.carrier_return.is_none()
                && self.read.return_types.is_empty()
                && read_entry.parameters.len() == 2
                && read_entry.parameters.iter().zip([k, v]).all(|(&p, l)| {
                    self.read.entity(p).map(|d| d.ty)
                        == graph.function.entity(EntityId::Loan(l)).map(|d| d.ty)
                }),
            "temporary SSA actual read Borrow signature matches K/V",
            self.read,
        )?;
        let mut roots = BTreeMap::<LoanId, (usize, Option<LoanId>)>::new();
        let mut live = BTreeSet::new();
        let mut ends = Vec::new();
        let mut calls = 0;
        for i in &ops {
            match &i.operation {
                Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. } => {
                    let l = loan(i)?;
                    let index = if l == k { 0 } else { 1 };
                    need(
                        [k, v].contains(&l)
                            && roots.insert(l, (index, None)).is_none()
                            && live.insert(l),
                        "temporary SSA actual component birth",
                        i,
                    )?;
                    for operand in [EntityId::Loan(roles.source), EntityId::Value(roles.cursor)] {
                        uses.permit(operand, i);
                    }
                    uses.protect(EntityId::Loan(l));
                    protected.insert(EntityId::Loan(l));
                }
                Operation::SharedReborrow { source } => {
                    let (index, _) = *roots
                        .get(source)
                        .ok_or_else(|| failure("temporary SSA derived Borrow actual parent", i))?;
                    let l = loan(i)?;
                    need(
                        live.contains(source)
                            && graph.function.entity(EntityId::Loan(l)).map(|e| e.ty)
                                == graph.function.entity(EntityId::Loan(*source)).map(|e| e.ty)
                            && i.origin == Origin::Source(site.arguments[index])
                            && roots.insert(l, (index, Some(*source))).is_none()
                            && live.insert(l),
                        "temporary SSA live typed component-derived Borrow",
                        i,
                    )?;
                    uses.permit(EntityId::Loan(*source), i);
                    uses.protect(EntityId::Loan(l));
                    protected.insert(EntityId::Loan(l));
                }
                Operation::DirectCall {
                    callee,
                    receiver,
                    arguments,
                } => {
                    need(
                        *callee == self.read.id
                            && receiver.is_none()
                            && arguments.len() == 2
                            && i.origin == Origin::Source(site.read_call)
                            && i.results.is_empty(),
                        "temporary SSA actual read Call target/origin and Unit result",
                        i,
                    )?;
                    for (index, &argument) in arguments.iter().enumerate() {
                        let EntityId::Loan(l) = argument else {
                            return Err(failure("temporary SSA read Borrow operand", argument));
                        };
                        need(
                            live.contains(&l)
                                && roots.get(&l).is_some_and(|(root, _)| *root == index),
                            "temporary SSA actual ordered read K/V lineage",
                            (index, i, &roots),
                        )?;
                        uses.permit(argument, i);
                    }
                    calls += 1;
                }
                Operation::BorrowEnd { loan } => {
                    let (index, parent) = roots.get(loan).copied().ok_or_else(|| {
                        failure("temporary SSA ended actual component lineage", i)
                    })?;
                    need(
                        calls == 1
                            && live.contains(loan)
                            && !roots.iter().any(|(child, (_, parent))| {
                                *parent == Some(*loan) && live.contains(child)
                            })
                            && i.results.is_empty()
                            && i.origin
                                == Origin::Source(if parent.is_none() {
                                    site.span
                                } else {
                                    site.read_call
                                }),
                        "temporary SSA derived End follows read and precedes parent End",
                        (i, index, parent),
                    )?;
                    live.remove(loan);
                    if parent.is_none() {
                        ends.push(*loan);
                    }
                    uses.permit(EntityId::Loan(*loan), i);
                }
                Operation::MapIterationNext { .. } => {
                    need(
                        i.id == next.id
                            && calls == 1
                            && live.is_empty()
                            && ends == [v, k]
                            && graph.function.entity(result(i)?).map(|e| e.ty)
                                == graph
                                    .function
                                    .entity(EntityId::Value(roles.cursor))
                                    .map(|e| e.ty),
                        "temporary SSA all derived/KV end before actual Next",
                        (i, &live, &ends, calls),
                    )?;
                    uses.permit(EntityId::Loan(roles.source), i);
                    uses.permit(EntityId::Value(roles.cursor), i);
                    uses.protect(result(i)?);
                    protected.insert(result(i)?);
                }
                _ => return Err(failure("temporary SSA unexpected body operation", i)),
            }
        }
        need(
            calls == 1
                && live.is_empty()
                && ends == [v, k]
                && ops.first().map(|i| i.id) == Some(key.id)
                && graph.position(key)? < graph.position(val)?
                && ops.last().map(|i| i.id) == Some(next.id),
            "temporary SSA entire body lifetime closure",
            (&ops, &roots, ends),
        )?;
        need(
            graph.edges.iter().all(|(_, e)| {
                e.arguments
                    .iter()
                    .all(|a| !matches!(a, EntityId::Loan(l) if roots.contains_key(l)))
            }),
            "temporary SSA no K/V or derived loan escapes body edges",
            &graph.edges,
        )?;
        Ok(value(next)?)
    }
}
