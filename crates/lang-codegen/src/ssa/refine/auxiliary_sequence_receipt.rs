//! Independent bounded two-root auxiliary-sequence binding verification; existing producers own every effect.
mod admission;
const MAX_USES: usize = 8;
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
    pub(super) role: usize,
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    entries: Vec<Plan>,
}
#[derive(Clone, Debug)]
struct Use {
    call: UnitExpressionId,
    argument: UnitExpressionId,
    callee: FunctionId,
    place: Option<Slot>,
    loan: Option<Slot>,
    authorized: bool,
}
#[derive(Clone, Debug)]
struct Plan {
    ids: Vec<UnitLoanActionId>,
    uses: Vec<Use>,
    callable: UnitItemId,
    caller: FunctionId,
    binding: UnitSymbolId,
    root: UnitSymbolId,
    auxiliary_root: UnitSymbolId,
    source_call: UnitExpressionId,
    source_argument: UnitExpressionId,
    source_index: usize,
    auxiliary_argument: UnitExpressionId,
    consumer_call: UnitExpressionId,
    consumer_argument: UnitExpressionId,
    entry_statement: UnitStatementId,
    completion_statement: UnitStatementId,
    end_point: UnitDropPoint,
    source_callee: FunctionId,
    consumer_callee: FunctionId,
    drop_index: usize,
    auxiliary_drop_index: usize,
    owner: Option<Slot>,
    auxiliary_owner: Option<Slot>,
    place: Option<Slot>,
    source: Option<Slot>,
    auxiliary_place: Option<Slot>,
    auxiliary: Option<Slot>,
    result: Option<Slot>,
    stage: usize,
    activated: bool,
    source_authorized: bool,
    auxiliary_authorized: bool,
    committed: bool,
    consumer_authorized: bool,
    retired: bool,
    drop_authorized: bool,
    auxiliary_drop_authorized: bool,
    completed: bool,
    block: Option<BlockRef>,
}
impl Plan {
    fn valid_shape(&self) -> bool {
        (2..=MAX_USES).contains(&self.uses.len())
    }
    fn auxiliary_drop_role(&self) -> usize {
        10 + 4 * self.uses.len()
    }
    fn role_count(&self) -> usize {
        11 + 4 * self.uses.len()
    }

    fn ordered_loans(&self) -> [Option<Slot>; 2] {
        if self.source_index == 0 {
            [self.source, self.auxiliary]
        } else {
            [self.auxiliary, self.source]
        }
    }
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
            if !p.valid_shape()
                || p.source_index > 1
                || p.owner.is_some()
                || p.stage != 0
                || bindings.len() != 2
            {
                return Err(invalid(span));
            }
            p.owner = Some(*bindings.get(&p.root).ok_or_else(|| invalid(span))?);
            p.auxiliary_owner = Some(
                *bindings
                    .get(&p.auxiliary_root)
                    .ok_or_else(|| invalid(span))?,
            );
            if p.owner == p.auxiliary_owner {
                return Err(invalid(span));
            }
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
    pub(super) fn sequence_operand(
        &mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        owner: Slot,
        bindings: &BTreeMap<UnitSymbolId, Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            if !p.valid_shape() {
                return Err(invalid(span));
            }
            let Some(index) = p.uses.iter().position(|u| u.call == call) else {
                continue;
            };
            if p.uses.iter().filter(|u| u.call == call).count() != 1
                || p.stage != 10 + 4 * index
                || p.uses[index].authorized
                || argument != p.uses[index].argument
                || Some(owner) != p.auxiliary_owner
                || bindings.get(&p.auxiliary_root).copied() != Some(owner)
                || !p.retired
                || !p.drop_authorized
            {
                return Err(invalid(span));
            }
            p.uses[index].authorized = true;
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
            let source_stage = if p.source_index == 0 { 0 } else { 2 };
            let auxiliary_stage = 2 - source_stage;
            if p.stage == source_stage && argument == p.source_argument && !p.source_authorized {
                if p.stage == 2 && !p.auxiliary_authorized {
                    return Err(invalid(span));
                }
                p.source_authorized = true;
            } else if p.stage == auxiliary_stage
                && argument == p.auxiliary_argument
                && !p.auxiliary_authorized
            {
                if p.stage == 2 && !p.source_authorized {
                    return Err(invalid(span));
                }
                p.auxiliary_authorized = true;
            } else {
                return Err(invalid(span));
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
            if p.auxiliary_drop_index == index {
                if p.stage != p.auxiliary_drop_role()
                    || p.auxiliary_drop_authorized
                    || p.auxiliary_owner != Some(slot)
                    || target != UnitDropTarget::Named(p.auxiliary_root)
                {
                    return Err(invalid(span));
                }
                p.auxiliary_drop_authorized = true;
            } else if p.drop_index == index {
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
            if !p.valid_shape()
                || p.completed
                || p.stage != p.role_count()
                || p.block != Some(block)
                || bindings.get(&p.root).copied() != p.owner
                || !dead.contains(&p.root)
                || !dead.contains(&p.auxiliary_root)
                || bindings.get(&p.auxiliary_root).copied() != p.auxiliary_owner
                || !consumed.contains(&p.drop_index)
                || !consumed.contains(&p.auxiliary_drop_index)
            {
                return Err(invalid(span));
            }
            p.completed = true;
        }
        Ok(())
    }
    /// Stamp the bounded dynamic sequence of real operations during the exact entered statement interval.
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
            let first_authorized = if p.source_index == 0 {
                p.source_authorized
            } else {
                p.auxiliary_authorized
            };
            let both_authorized = p.source_authorized && p.auxiliary_authorized;
            if !p.valid_shape()
                || tag.is_some()
                || p.block != Some(block)
                || !p.activated
                || !first_authorized
                || (p.stage >= 2 && !both_authorized)
            {
                return Err(invalid(span));
            }
            let single = || match results {
                [s] => Ok(*s),
                _ => Err(invalid(span)),
            };
            match (p.stage, op) {
                (stage @ (0 | 2), ROp::RootPlace { owner }) => {
                    let expected_owner = if (stage / 2) == p.source_index {
                        p.owner
                    } else {
                        p.auxiliary_owner
                    };
                    if Some(*owner) != expected_owner {
                        return Err(invalid(span));
                    }
                    let slot = single()?;
                    if Some(slot) == p.place || Some(slot) == p.auxiliary_place {
                        return Err(invalid(span));
                    }
                    if (stage / 2) == p.source_index {
                        p.place = Some(slot);
                    } else {
                        p.auxiliary_place = Some(slot);
                    }
                }
                (
                    stage @ (1 | 3),
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                ) => {
                    let is_source = (stage / 2) == p.source_index;
                    let expected = if is_source {
                        p.place
                    } else {
                        p.auxiliary_place
                    };
                    if Some(*place) != expected {
                        return Err(invalid(span));
                    }
                    let slot = single()?;
                    if Some(slot) == p.source || Some(slot) == p.auxiliary {
                        return Err(invalid(span));
                    }
                    if is_source {
                        p.source = Some(slot);
                    } else {
                        p.auxiliary = Some(slot);
                    }
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
                            p.ordered_loans()[0].ok_or_else(|| invalid(span))?,
                            p.ordered_loans()[1].ok_or_else(|| invalid(span))?,
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
                (stage, ROp::Drop { owner })
                    if stage == p.auxiliary_drop_role()
                        && p.auxiliary_drop_authorized
                        && Some(*owner) == p.auxiliary_owner
                        && results.is_empty() => {}
                (stage, op) if (10..p.auxiliary_drop_role()).contains(&stage) => {
                    let index = (stage - 10) / 4;
                    if !p.uses[index].authorized {
                        return Err(invalid(span));
                    }
                    match ((stage - 10) % 4, op) {
                        (0, ROp::RootPlace { owner }) if Some(*owner) == p.auxiliary_owner => {
                            let slot = single()?;
                            if [p.place, p.auxiliary_place].contains(&Some(slot))
                                || p.uses.iter().any(|u| u.place == Some(slot))
                            {
                                return Err(invalid(span));
                            }
                            p.uses[index].place = Some(slot);
                        }
                        (
                            1,
                            ROp::BorrowBegin {
                                place,
                                kind: LoanKind::Shared,
                            },
                        ) if Some(*place) == p.uses[index].place => {
                            let slot = single()?;
                            if [p.source, p.auxiliary, p.result].contains(&Some(slot))
                                || p.uses.iter().any(|u| u.loan == Some(slot))
                            {
                                return Err(invalid(span));
                            }
                            p.uses[index].loan = Some(slot);
                        }
                        (
                            2,
                            ROp::DirectCall {
                                callee,
                                receiver: None,
                                arguments,
                            },
                        ) if *callee == p.uses[index].callee
                            && arguments.as_slice() == p.uses[index].loan.as_slice()
                            && results.is_empty() => {}
                        (3, ROp::BorrowEnd { loan })
                            if Some(*loan) == p.uses[index].loan && results.is_empty() => {}
                        _ => return Err(invalid(span)),
                    }
                }
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
        if self.entries.iter().any(|p| {
            !p.valid_shape()
                || p.stage != p.role_count()
                || !p.completed
                || !consumed.contains(&p.drop_index)
                || !consumed.contains(&p.auxiliary_drop_index)
        }) {
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
