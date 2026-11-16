//! Independent cross-statement parent/child verification; existing producers own every effect.
mod admission;
mod emitted;
use super::body::{BlockRef, ROp, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{FunctionId, LoanKind},
};
pub(super) use emitted::Emitted;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{BorrowSourceLoan, UnitDropPoint, UnitDropTarget, UnitLoanActionId},
    source::Span,
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub(super) struct Tag {
    pub(super) plan: usize,
    pub(super) role: u8,
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    entries: Vec<Plan>,
}
#[derive(Clone, Debug)]
struct Plan {
    ids: Vec<UnitLoanActionId>,
    callable: UnitItemId,
    caller: FunctionId,
    parent: UnitSymbolId,
    child: UnitSymbolId,
    child_initializer: UnitExpressionId,
    root: UnitSymbolId,
    source_call: UnitExpressionId,
    source_argument: UnitExpressionId,
    consumer_call: UnitExpressionId,
    consumer_argument: UnitExpressionId,
    entry_statement: UnitStatementId,
    completion_statement: UnitStatementId,
    end_point: UnitDropPoint,
    source_callee: FunctionId,
    consumer_callee: FunctionId,
    drop_index: usize,
    owner: Option<Slot>,
    place: Option<Slot>,
    source: Option<Slot>,
    result: Option<Slot>,
    child_result: Option<Slot>,
    stage: u8,
    activated: bool,
    source_authorized: bool,
    committed: bool,
    child_activated: bool,
    child_committed: bool,
    child_retired: bool,
    consumer_authorized: bool,
    retired: bool,
    drop_authorized: bool,
    completed: bool,
    block: Option<BlockRef>,
}
impl Plans {
    pub(super) fn has_entry(&self, statement: UnitStatementId) -> bool {
        self.entries.iter().any(|p| p.entry_statement == statement)
    }
    pub(super) fn has_completion(&self, statement: UnitStatementId) -> bool {
        self.entries
            .iter()
            .any(|p| p.completion_statement == statement)
    }
    pub(super) fn has_consumer(&self, call: UnitExpressionId) -> bool {
        self.entries.iter().any(|p| p.consumer_call == call)
    }
    pub(super) fn enter(
        &mut self,
        statement: UnitStatementId,
        bindings: &BTreeMap<UnitSymbolId, Slot>,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.entry_statement != statement {
                continue;
            }
            if p.owner.is_some() || p.stage != 0 || bindings.len() != 1 {
                return Err(invalid(span));
            }
            p.owner = Some(*bindings.get(&p.root).ok_or_else(|| invalid(span))?);
            p.block = Some(block);
        }
        Ok(())
    }
    pub(super) fn activate(
        &mut self,
        binding: UnitSymbolId,
        initializer: UnitExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.parent != binding {
                continue;
            }
            if p.owner.is_none()
                || p.activated
                || p.stage != 0
                || initializer != p.source_call
                || source.call() != p.source_call
                || source.argument() != p.source_argument
            {
                return Err(invalid(span));
            }
            p.activated = true;
        }
        Ok(())
    }
    pub(super) fn source_operand(
        &mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.source_call != call {
                continue;
            }
            if !p.activated || p.source_authorized || p.stage != 0 || argument != p.source_argument
            {
                return Err(invalid(span));
            }
            p.source_authorized = true;
        }
        Ok(())
    }
    pub(super) fn commit(
        &mut self,
        binding: UnitSymbolId,
        borrowed: &BTreeMap<UnitSymbolId, Slot>,
        sources: &BTreeMap<UnitSymbolId, Vec<Slot>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.parent != binding {
                continue;
            }
            if p.stage != 3
                || p.committed
                || borrowed.get(&binding).copied() != p.result
                || sources.get(&binding).map(Vec::as_slice) != Some(p.source.as_slice())
            {
                return Err(invalid(span));
            }
            p.committed = true;
        }
        Ok(())
    }
    pub(super) fn consumer_argument(
        &mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        binding: UnitSymbolId,
        loan: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.consumer_call != call {
                continue;
            }
            if p.stage != 4
                || !p.child_committed
                || p.consumer_authorized
                || binding != p.child
                || argument != p.consumer_argument
                || Some(loan) != p.child_result
            {
                return Err(invalid(span));
            }
            p.consumer_authorized = true;
        }
        Ok(())
    }
    pub(super) fn child_activate(
        &mut self,
        binding: UnitSymbolId,
        initializer: UnitExpressionId,
        parent: UnitSymbolId,
        parent_slot: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.child != binding {
                continue;
            }
            if p.stage != 3
                || !p.committed
                || p.child_activated
                || initializer != p.child_initializer
                || parent != p.parent
                || Some(parent_slot) != p.result
            {
                return Err(invalid(span));
            }
            p.child_activated = true;
        }
        Ok(())
    }
    pub(super) fn has_child(&self, binding: UnitSymbolId) -> bool {
        self.entries.iter().any(|p| p.child == binding)
    }
    pub(super) fn child_commit(
        &mut self,
        binding: UnitSymbolId,
        borrowed: &BTreeMap<UnitSymbolId, Slot>,
        sources: &BTreeMap<UnitSymbolId, Vec<Slot>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.child != binding {
                continue;
            }
            if p.stage != 4
                || !p.child_activated
                || p.child_committed
                || borrowed.get(&p.child).copied() != p.child_result
                || borrowed.get(&p.parent).copied() != p.result
                || sources.get(&p.child).map(Vec::as_slice) != Some(&[])
                || sources.get(&p.parent).map(Vec::as_slice) != Some(p.source.as_slice())
            {
                return Err(invalid(span));
            }
            p.child_committed = true;
        }
        Ok(())
    }
    pub(super) fn retire(
        &mut self,
        binding: UnitSymbolId,
        point: UnitDropPoint,
        result: Option<Slot>,
        sources: Option<&[Slot]>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if binding == p.child {
                if p.stage != 5
                    || !p.child_committed
                    || p.child_retired
                    || point != p.end_point
                    || result != p.child_result
                    || sources != Some(&[])
                {
                    return Err(invalid(span));
                }
                p.child_retired = true;
            } else if binding == p.parent {
                if p.stage != 6
                    || !p.child_retired
                    || p.retired
                    || point != p.end_point
                    || result != p.result
                    || sources != Some(p.source.as_slice())
                {
                    return Err(invalid(span));
                }
                p.retired = true;
            }
        }
        Ok(())
    }
    pub(super) fn drop_fact(
        &mut self,
        index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.drop_index != index {
                continue;
            }
            if p.stage != 8
                || !p.retired
                || p.drop_authorized
                || p.owner != Some(slot)
                || target != UnitDropTarget::Named(p.root)
            {
                return Err(invalid(span));
            }
            p.drop_authorized = true;
        }
        Ok(())
    }
    pub(super) fn complete(
        &mut self,
        statement: UnitStatementId,
        bindings: &BTreeMap<UnitSymbolId, Slot>,
        dead: &BTreeSet<UnitSymbolId>,
        consumed: &BTreeSet<usize>,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if p.completion_statement != statement {
                continue;
            }
            if p.completed
                || p.stage != 9
                || p.block != Some(block)
                || bindings.get(&p.root).copied() != p.owner
                || !dead.contains(&p.root)
                || !consumed.contains(&p.drop_index)
            {
                return Err(invalid(span));
            }
            p.completed = true;
        }
        Ok(())
    }
    /// Stamp only the nine real operations during the exact entered statement interval.
    pub(super) fn statement(
        &mut self,
        op: &ROp,
        results: &[Slot],
        block: BlockRef,
        span: Span,
    ) -> Result<Option<Tag>, LoweringError> {
        let mut tag = None;
        for (index, p) in self.entries.iter_mut().enumerate() {
            if p.owner.is_none() || p.completed {
                continue;
            }
            if tag.is_some() || p.block != Some(block) || !p.activated || !p.source_authorized {
                return Err(invalid(span));
            }
            let single = || match results {
                [s] => Ok(*s),
                _ => Err(invalid(span)),
            };
            match (p.stage, op) {
                (0, ROp::RootPlace { owner }) if Some(*owner) == p.owner => {
                    p.place = Some(single()?)
                }
                (
                    1,
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                ) if Some(*place) == p.place => p.source = Some(single()?),
                (
                    2,
                    ROp::BorrowCall {
                        callee,
                        arguments,
                        source,
                    },
                ) if *callee == p.source_callee
                    && Some(*source) == p.source
                    && arguments.as_slice() == p.source.as_slice() =>
                {
                    p.result = Some(single()?);
                    if p.result == p.source {
                        return Err(invalid(span));
                    }
                }
                (3, ROp::SharedReborrow { source })
                    if p.child_activated && Some(*source) == p.result =>
                {
                    p.child_result = Some(single()?);
                    if p.child_result == p.result || p.child_result == p.source {
                        return Err(invalid(span));
                    }
                }
                (
                    4,
                    ROp::DirectCall {
                        callee,
                        receiver: None,
                        arguments,
                    },
                ) if p.child_committed
                    && p.consumer_authorized
                    && *callee == p.consumer_callee
                    && arguments.as_slice() == p.child_result.as_slice()
                    && results.is_empty() => {}
                (5, ROp::BorrowEnd { loan })
                    if p.child_retired && Some(*loan) == p.child_result && results.is_empty() => {}
                (6, ROp::BorrowEnd { loan })
                    if p.retired && Some(*loan) == p.result && results.is_empty() => {}
                (7, ROp::BorrowEnd { loan })
                    if p.retired && Some(*loan) == p.source && results.is_empty() => {}
                (8, ROp::Drop { owner })
                    if p.drop_authorized && Some(*owner) == p.owner && results.is_empty() => {}
                _ => return Err(invalid(span)),
            }
            tag = Some(Tag {
                plan: index,
                role: p.stage,
            });
            p.stage += 1;
        }
        Ok(tag)
    }
    pub(super) fn finish(
        &self,
        consumed: &BTreeSet<usize>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self
            .entries
            .iter()
            .any(|p| p.stage != 9 || !p.completed || !consumed.contains(&p.drop_index))
        {
            return Err(invalid(span));
        }
        Ok(())
    }
}
fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}
#[cfg(test)]
thread_local! { static RECEIPTS: std::cell::RefCell<Option<Vec<emitted::Receipt>>> = const { std::cell::RefCell::new(None) }; }
#[cfg(all(test, feature = "loan-observation-test-support"))]
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<emitted::Receipt>) {
    RECEIPTS.with(|r| {
        assert!(r.borrow().is_none());
        *r.borrow_mut() = Some(Vec::new());
    });
    let result = lower();
    (result, RECEIPTS.with(|r| r.borrow_mut().take().unwrap()))
}
