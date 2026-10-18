//! Each actual For operation consumes its own current role; no production witness is used.
use super::super::{
    fixture::{Failure, contains, failure, need},
    query_graph::{Graph, loan, result, value},
    query_inputs::one,
};
use super::{
    query_graph::{Role, Roles, Witness, ty},
    query_inputs::{ReadInput, SiteInput},
};
use crate::ssa::model::*;

pub(super) fn provider(i: &Instruction) -> bool {
    matches!(
        i.operation,
        Operation::MapIterationFirst { .. }
            | Operation::MapIterationHasNext { .. }
            | Operation::MapIterationNext { .. }
            | Operation::MapIterationKeyLoan { .. }
            | Operation::MapIterationValueLoan { .. }
    )
}
#[derive(Clone, Copy)]
pub(super) struct Layer {
    pub source: Role,
    pub cursor: Role,
    pub key: Role,
    pub val: Role,
}
pub(super) const OUTER: Layer = Layer {
    source: Role::OuterSource,
    cursor: Role::OuterCursor,
    key: Role::OuterKey,
    val: Role::OuterValue,
};
pub(super) const INNER: Layer = Layer {
    source: Role::InnerSource,
    cursor: Role::InnerCursor,
    key: Role::InnerKey,
    val: Role::InnerValue,
};
pub(super) struct Context<'a, 'ir> {
    pub graph: &'a Graph<'ir>,
    pub module: &'ir Module,
    pub read: &'ir Function,
}
impl<'ir> Context<'_, 'ir> {
    pub fn birth(
        &self,
        block: BlockId,
        site: &SiteInput,
        layer: Layer,
        roles: &mut Roles,
        w: &mut Witness,
    ) -> Result<(), Failure> {
        let g = self.graph;
        need(
            contains(site.span, site.source) && contains(site.span, site.body),
            "nested SSA consumes validated source/body bridge",
            site,
        )?;
        let parent = roles.loan(Role::Caller)?;
        let born = one(
            g.ops(block)?.into_iter().filter(|i| {
                i.origin == Origin::Source(site.span)
                    && matches!(i.operation, Operation::SharedReborrow { .. })
            }),
            "nested SSA unique For lease birth",
        )?;
        need(
            born.operation == Operation::SharedReborrow { source: parent },
            "nested SSA For lease from current caller, not outer lease",
            born,
        )?;
        let source = loan(born)?;
        need(
            ty(g, result(born)?)? == ty(g, EntityId::Loan(parent))?,
            "nested SSA lease preserves Shared map target",
            born,
        )?;
        let first = one(
            g.ops(block)?.into_iter().filter(|i| {
                i.origin == Origin::Source(site.span)
                    && matches!(i.operation, Operation::MapIterationFirst { .. })
            }),
            "nested SSA unique For First",
        )?;
        need(
            first.operation == Operation::MapIterationFirst { source }
                && matches!(ty(g, result(first)?)?, EntityType::Value(t) if self.module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 64, signed: false }))
                && g.ops(block)?[g.position(born)?..]
                    .iter()
                    .map(|i| i.id)
                    .collect::<Vec<_>>()
                    == [born.id, first.id],
            "nested SSA actual reborrow/First suffix and cursor type",
            (born, first),
        )?;
        roles.insert(layer.source, result(born)?);
        roles.insert(layer.cursor, result(first)?);
        w.protect(roles)?;
        w.uses.permit(EntityId::Loan(parent), born);
        w.uses.permit(EntityId::Loan(source), first);
        w.providers.insert(first.id);
        need(
            g.term(block)?.origin == Origin::Source(site.span),
            "nested SSA actual preheader branch origin",
            g.term(block)?,
        )
    }
    pub fn header(
        &self,
        block: BlockId,
        site: &SiteInput,
        layer: Layer,
        roles: &Roles,
        w: &mut Witness,
    ) -> Result<(BlockId, Roles, BlockId, Roles), Failure> {
        let g = self.graph;
        let has = one(
            g.ops(block)?.into_iter(),
            "nested SSA header only actual HasNext",
        )?;
        need(
            has.operation
                == Operation::MapIterationHasNext {
                    source: roles.loan(layer.source)?,
                    cursor: roles.value(layer.cursor)?,
                }
                && has.origin == Origin::Source(site.span)
                && matches!(ty(g, result(has)?)?, EntityType::Value(t) if self.module.type_kind(t) == Some(&SsaTypeKind::Boolean)),
            "nested SSA header current lease/cursor condition",
            has,
        )?;
        w.uses.permit(roles.get(layer.source)?, has);
        w.uses.permit(roles.get(layer.cursor)?, has);
        w.providers.insert(has.id);
        let term = g.term(block)?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &term.kind
        else {
            return Err(failure("nested SSA actual provider conditional", term));
        };
        need(
            *condition == value(has)?
                && term.origin == Origin::Source(site.span)
                && when_true.target != when_false.target
                && when_true.target != block
                && when_false.target != block,
            "nested SSA HasNext Bool drives both real provider arms",
            term,
        )?;
        w.conditions.insert(block, *condition);
        for i in &g.instructions {
            need(
                !i.operation
                    .entities()
                    .contains(&EntityId::Value(*condition)),
                "nested SSA provider Bool has no instruction use",
                i,
            )?;
        }
        for &b in &g.blocks {
            let t = g.term(b)?;
            if t.kind.entities().contains(&EntityId::Value(*condition)) {
                need(
                    b == block
                        && !when_true.arguments.contains(&EntityId::Value(*condition))
                        && !when_false.arguments.contains(&EntityId::Value(*condition)),
                    "nested SSA provider Bool sole terminal use",
                    t,
                )?;
            }
        }
        for b in [block, when_true.target, when_false.target] {
            need(
                g.block(b)?.origin == Origin::Source(site.span),
                "nested SSA actual For role block Origin",
                g.block(b)?,
            )?;
        }
        let yes = w.edge(g, block, when_true, roles)?;
        let no = w.edge(g, block, when_false, roles)?;
        Ok((when_true.target, yes, when_false.target, no))
    }
    pub fn project(
        &self,
        block: BlockId,
        site: &SiteInput,
        layer: Layer,
        roles: &mut Roles,
        w: &mut Witness,
    ) -> Result<(), Failure> {
        let g = self.graph;
        let source = roles.loan(layer.source)?;
        let cursor = roles.value(layer.cursor)?;
        let key = one(
            g.ops(block)?
                .into_iter()
                .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. })),
            "nested SSA unique real key projection",
        )?;
        let val = one(
            g.ops(block)?
                .into_iter()
                .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
            "nested SSA unique real value projection",
        )?;
        need(
            key.operation == Operation::MapIterationKeyLoan { source, cursor }
                && val.operation == Operation::MapIterationValueLoan { source, cursor }
                && key.origin == Origin::Source(site.span)
                && val.origin == Origin::Source(site.span)
                && g.position(key)? == 0
                && g.position(val)? == 1,
            "nested SSA ordered K/V birth from own current lease/cursor",
            (key, val),
        )?;
        for i in [key, val] {
            need(
                matches!(ty(g, result(i)?)?, EntityType::Loan { kind: LoanKind::Shared, target }
                if self.module.type_kind(target) == Some(&SsaTypeKind::StringOwner)),
                "nested SSA actual Shared String component",
                i,
            )?;
            w.uses.permit(EntityId::Loan(source), i);
            w.uses.permit(EntityId::Value(cursor), i);
            w.providers.insert(i.id);
        }
        roles.insert(layer.key, result(key)?);
        roles.insert(layer.val, result(val)?);
        w.protect(roles)
    }
    pub fn read(
        &self,
        block: BlockId,
        input: &ReadInput,
        layer: Layer,
        roles: &Roles,
        w: &mut Witness,
    ) -> Result<&'ir Instruction, Failure> {
        let g = self.graph;
        let i = one(
            g.ops(block)?.into_iter().filter(|i| {
                i.origin == Origin::Source(input.span)
                    && matches!(i.operation, Operation::DirectCall { .. })
            }),
            "nested SSA actual read Call by source origin",
        )?;
        let args = [roles.get(layer.key)?, roles.get(layer.val)?];
        let params = &g.function; // Keep function-scoped entity lookup separate from callee slots.
        let callee_entry = self
            .read
            .entry_block()
            .and_then(|b| self.read.block(b))
            .ok_or_else(|| failure("nested SSA actual read entry", self.read.id))?;
        need(
            matches!(&i.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == self.read.id && arguments.as_slice() == args)
                && i.results.is_empty()
                && callee_entry.parameters.len() == 2
                && callee_entry.parameters.iter().zip(args).all(|(&p, a)| {
                    self.read.entity(p).map(|d| d.ty) == params.entity(a).map(|d| d.ty)
                })
                && input.arguments[0] != input.arguments[1]
                && input.arguments.iter().all(|&s| contains(input.span, s))
                && input.site
                    == if layer.source == Role::OuterSource {
                        0
                    } else {
                        1
                    },
            "nested SSA actual ordered current Borrow operands and callee parameter positions",
            (i, input, &args),
        )?;
        for a in args {
            w.uses.permit(a, i);
        }
        Ok(i)
    }
    pub fn end_source(
        &self,
        block: BlockId,
        site: &SiteInput,
        layer: Layer,
        roles: &Roles,
        w: &mut Witness,
    ) -> Result<&'ir Instruction, Failure> {
        let g = self.graph;
        let source = roles.loan(layer.source)?;
        let i = one(
            g.ops(block)?
                .into_iter()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: source }),
            "nested SSA unique current EndSource",
        )?;
        need(
            i.origin == Origin::Source(site.span) && g.position(i)? == 0,
            "nested SSA exhaustion EndSource actual prefix",
            i,
        )?;
        w.uses.permit(EntityId::Loan(source), i);
        Ok(i)
    }
    pub fn cleanup_next(
        &self,
        block: BlockId,
        site: &SiteInput,
        layer: Layer,
        roles: &Roles,
        last_read: &Instruction,
        w: &mut Witness,
    ) -> Result<Roles, Failure> {
        let g = self.graph;
        let mut suffix = Vec::new();
        for r in [layer.val, layer.key] {
            let loan = roles.loan(r)?;
            let end = one(
                g.ops(block)?
                    .into_iter()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan }),
                "nested SSA unique current component EndBinding",
            )?;
            need(
                end.origin == Origin::Source(site.span)
                    && g.position(last_read)? < g.position(end)?,
                "nested SSA actual component cleanup after last Borrow read",
                end,
            )?;
            w.uses.permit(EntityId::Loan(loan), end);
            suffix.push(end.id);
        }
        let next = one(
            g.ops(block)?
                .into_iter()
                .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
            "nested SSA this tail unique Next",
        )?;
        need(
            next.operation
                == Operation::MapIterationNext {
                    source: roles.loan(layer.source)?,
                    cursor: roles.value(layer.cursor)?,
                }
                && next.origin == Origin::Source(site.span)
                && ty(g, result(next)?)? == ty(g, roles.get(layer.cursor)?)?,
            "nested SSA Next actual current lease/cursor and result type",
            next,
        )?;
        suffix.push(next.id);
        let first = g
            .function
            .instruction(suffix[0])
            .ok_or_else(|| failure("nested SSA actual cleanup suffix head", &suffix))?;
        need(
            g.ops(block)?[g.position(first)?..]
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>()
                == suffix
                && g.term(block)?.origin == Origin::Source(site.span),
            "nested SSA complete V/K/Next suffix matches FE normal cleanup",
            (&suffix, g.ops(block)?),
        )?;
        w.uses.permit(roles.get(layer.source)?, next);
        w.uses.permit(roles.get(layer.cursor)?, next);
        w.providers.insert(next.id);
        let mut after = roles.clone();
        after.0.remove(&layer.key);
        after.0.remove(&layer.val);
        after.insert(layer.cursor, result(next)?);
        w.protect(&after)?;
        Ok(after)
    }
}
