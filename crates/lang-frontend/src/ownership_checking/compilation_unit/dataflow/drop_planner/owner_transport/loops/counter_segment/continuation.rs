//! Private second bridge from a frozen formal Header contract, never an entry catalog.
use super::*;
use crate::ownership_checking::{
    UnitDropFact, UnitDropPoint, UnitDropTarget, UnitOwnerMultiRootNormalTransportV1,
};

#[derive(Clone, Debug)]
pub(in super::super) struct Continuation {
    normal: UnitOwnerMultiRootNormalTransportV1,
    tail: (UnitSymbolId, UnitSymbolId, [UnitExpressionId; 4]),
    live: Vec<UnitSymbolId>,
    drops: Vec<UnitDropFact>,
    bindings: Vec<(UnitSymbolId, usize)>,
    pending: Option<(UnitExpressionId, usize)>,
}
impl Continuation {
    fn freeze(segment: &Segment, predecessor: UnitStatementId, state: &ValueState) -> Option<Self> {
        if segment.invalid
            || !segment.captured
            || segment.bridge.is_none()
            || segment.statements.last() != Some(&predecessor)
        {
            return None;
        }
        let body = segment.normal.as_ref()?;
        let normal = body.result()?;
        if normal.statement() != predecessor {
            return None;
        }
        let tail = body.counter_tail()?;
        let (live, drops) = body.exit_partition()?;
        let value = Self {
            bindings: normal
                .header_roots()
                .iter()
                .enumerate()
                .filter(|(_, s)| live.contains(s))
                .map(|(i, s)| (*s, i))
                .collect(),
            normal,
            tail,
            live,
            drops,
            pending: None,
        };
        (value.valid_source() && value.matches_state(state)).then_some(value)
    }
    fn valid_source(&self) -> bool {
        let roots = self.normal.header_roots();
        self.normal.is_consistent()
            && self.live.len() >= 2
            && self.tail.0 != self.tail.1
            && !roots.contains(&self.tail.0)
            && !roots.contains(&self.tail.1)
            && self
                .tail
                .2
                .iter()
                .all(|e| e.source_unit() == self.normal.statement().source_unit())
            && self.live.iter().all(|s| roots.contains(s))
            && self.drops.iter().all(|d| {
                matches!(d.target(), UnitDropTarget::Named(s) if roots.contains(&s))
                    && d.point() == UnitDropPoint::LoopExit(self.normal.statement())
            })
            && roots.iter().all(|s| {
                self.live.iter().filter(|v| *v == s).count()
                    + self
                        .drops
                        .iter()
                        .filter(|d| d.target() == UnitDropTarget::Named(*s))
                        .count()
                    == 1
            })
    }
    pub(super) fn matches_state(&self, state: &ValueState) -> bool {
        self.pending.is_none()
            && boundary_values(state).is_some_and(|values| {
                values.len() == self.bindings.len()
                    && values
                        .iter()
                        .all(|v| self.bindings.iter().filter(|(s, _)| *s == v.symbol).count() == 1)
            })
    }
    pub(super) fn move_value(
        &mut self,
        expression: UnitExpressionId,
        value: Option<&OwnedValue>,
    ) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let Some(i) = value.and_then(|v| self.bindings.iter().position(|(s, _)| *s == v.symbol))
        else {
            return false;
        };
        let (_, root) = self.bindings.remove(i);
        self.pending = Some((expression, root));
        true
    }
    pub(super) fn bind(&mut self, symbol: UnitSymbolId, expression: UnitExpressionId) -> bool {
        let Some((actual, root)) = self.pending.take() else {
            return false;
        };
        if actual != expression
            || symbol.source_unit() != self.normal.statement().source_unit()
            || self.bindings.iter().any(|(s, _)| *s == symbol)
        {
            return false;
        }
        self.bindings.push((symbol, root));
        true
    }
    pub(super) fn complete(&self) -> bool {
        self.valid_source()
            && self.pending.is_none()
            && self.bindings.len() == self.live.len()
            && self
                .normal
                .header_roots()
                .iter()
                .enumerate()
                .filter(|(_, s)| self.live.contains(s))
                .all(|(i, _)| {
                    self.bindings
                        .iter()
                        .filter(|(s, index)| *index == i && !self.normal.header_roots().contains(s))
                        .count()
                        == 1
                })
    }
    #[cfg(test)]
    pub(in super::super) fn pairs(&self) -> Vec<(UnitSymbolId, UnitSymbolId)> {
        self.bindings
            .iter()
            .map(|(target, i)| (self.normal.header_roots()[*i], *target))
            .collect()
    }
}
impl ControlTransactions {
    pub(super) fn continuation_source(
        &self,
        id: UnitStatementId,
        state: &ValueState,
    ) -> Option<Continuation> {
        // Exactly three admitted top-level controls; never recursive or arbitrary batching.
        let [first, second] = self.counter_segments.as_slice() else {
            return None;
        };
        if self.root_branches_invalid
            || second.active
            || second.invalid
            || second.captured
            || second.statements.first() != Some(&id)
        {
            return None;
        }
        Continuation::freeze(first, second.predecessor, state)
    }
    #[cfg(test)]
    pub(in super::super) fn segment_continuation(
        &self,
        control: UnitStatementId,
    ) -> Option<Continuation> {
        if self.root_branches_invalid {
            return None;
        }
        let segment = self
            .counter_segments
            .iter()
            .find(|s| s.statements.last() == Some(&control) && s.captured && !s.invalid)?;
        segment.continuation.clone().filter(|c| c.complete())
    }
}

#[cfg(test)]
mod tests;
