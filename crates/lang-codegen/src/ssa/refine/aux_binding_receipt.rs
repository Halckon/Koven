//! Independent auxiliary-bearing binding verification; existing producers own every effect.
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
    binding: UnitSymbolId,
    root: UnitSymbolId,
    source_call: UnitExpressionId,
    source_argument: UnitExpressionId,
    auxiliary_argument: UnitExpressionId,
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
    auxiliary_place: Option<Slot>,
    auxiliary: Option<Slot>,
    result: Option<Slot>,
    stage: u8,
    activated: bool,
    source_authorized: bool,
    auxiliary_authorized: bool,
    committed: bool,
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
            if p.binding != binding {
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
            if !p.activated {
                return Err(invalid(span));
            }
            match (p.stage, argument) {
                (0, arg) if arg == p.source_argument && !p.source_authorized => {
                    p.source_authorized = true
                }
                (2, arg)
                    if arg == p.auxiliary_argument
                        && p.source_authorized
                        && !p.auxiliary_authorized =>
                {
                    p.auxiliary_authorized = true
                }
                _ => return Err(invalid(span)),
            }
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
            if p.binding != binding {
                continue;
            }
            if p.stage != 6
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
            if p.stage != 6
                || !p.committed
                || p.consumer_authorized
                || binding != p.binding
                || argument != p.consumer_argument
                || Some(loan) != p.result
            {
                return Err(invalid(span));
            }
            p.consumer_authorized = true;
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
            if p.binding != binding {
                continue;
            }
            if p.stage != 7
                || p.retired
                || point != p.end_point
                || result != p.result
                || sources != Some(p.source.as_slice())
            {
                return Err(invalid(span));
            }
            p.retired = true;
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
            if p.stage != 9
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
                || p.stage != 10
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
    /// Stamp only the ten real operations during the exact entered statement interval.
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
                (2, ROp::RootPlace { owner })
                    if p.auxiliary_authorized && Some(*owner) == p.owner =>
                {
                    let slot = single()?;
                    if Some(slot) == p.place {
                        return Err(invalid(span));
                    }
                    p.auxiliary_place = Some(slot);
                }
                (
                    3,
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                ) if Some(*place) == p.auxiliary_place => {
                    let slot = single()?;
                    if Some(slot) == p.source {
                        return Err(invalid(span));
                    }
                    p.auxiliary = Some(slot);
                }
                (
                    4,
                    ROp::BorrowCall {
                        callee,
                        arguments,
                        source,
                    },
                ) if *callee == p.source_callee
                    && Some(*source) == p.source
                    && arguments.as_slice()
                        == [
                            p.source.ok_or_else(|| invalid(span))?,
                            p.auxiliary.ok_or_else(|| invalid(span))?,
                        ] =>
                {
                    p.result = Some(single()?);
                    if p.result == p.source || p.result == p.auxiliary {
                        return Err(invalid(span));
                    }
                }
                (5, ROp::BorrowEnd { loan })
                    if Some(*loan) == p.auxiliary && results.is_empty() => {}
                (
                    6,
                    ROp::DirectCall {
                        callee,
                        receiver: None,
                        arguments,
                    },
                ) if p.committed
                    && p.consumer_authorized
                    && *callee == p.consumer_callee
                    && arguments.as_slice() == p.result.as_slice()
                    && results.is_empty() => {}
                (7, ROp::BorrowEnd { loan })
                    if p.retired && Some(*loan) == p.result && results.is_empty() => {}
                (8, ROp::BorrowEnd { loan })
                    if p.retired && Some(*loan) == p.source && results.is_empty() => {}
                (9, ROp::Drop { owner })
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
            .any(|p| p.stage != 10 || !p.completed || !consumed.contains(&p.drop_index))
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
#[derive(Default)]
struct Observations {
    pending: Vec<emitted::Receipt>,
    published: Vec<emitted::Receipt>,
}
#[cfg(test)]
thread_local! { static RECEIPTS: std::cell::RefCell<Option<Observations>> = const {std::cell::RefCell::new(None)}; }
/// Observation publication follows the original unit verifier; no extra verifier invocation.
#[cfg(test)]
pub(in crate::ssa) struct UnitPublication {
    start: usize,
    committed: bool,
}
#[cfg(test)]
impl UnitPublication {
    pub(in crate::ssa) fn begin() -> Self {
        Self {
            start: RECEIPTS.with(|v| v.borrow().as_ref().map_or(0, |v| v.pending.len())),
            committed: false,
        }
    }
    pub(in crate::ssa) fn commit(mut self) {
        RECEIPTS.with(|v| {
            if let Some(v) = v.borrow_mut().as_mut() {
                v.published.extend(v.pending.drain(self.start..));
            }
        });
        self.committed = true;
    }
}
#[cfg(test)]
impl Drop for UnitPublication {
    fn drop(&mut self) {
        if !self.committed {
            RECEIPTS.with(|v| {
                if let Some(v) = v.borrow_mut().as_mut() {
                    v.pending.truncate(self.start);
                }
            });
        }
    }
}
#[cfg(test)]
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<emitted::Receipt>) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            RECEIPTS.with(|v| {
                v.borrow_mut().take();
            });
        }
    }
    RECEIPTS.with(|v| {
        assert!(v.borrow().is_none());
        *v.borrow_mut() = Some(Observations::default());
    });
    let _reset = Reset;
    let result = lower();
    let receipts = RECEIPTS.with(|v| v.borrow_mut().take().unwrap().published);
    (result, receipts)
}
