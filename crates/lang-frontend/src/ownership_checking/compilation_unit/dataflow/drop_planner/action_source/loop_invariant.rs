//! Independent linear boundary ticket. It is never cloned into a branch state.
use super::{ActionHandle, ActionSource, BindingEnd, Record};
use crate::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitBindingUseReturn, UnitDropPoint, UnitLoanActionId, UnitLoanFact,
        UnitLoanLoopInvariantLineage,
    },
    type_checking::{UnitExpressionId, UnitStatementId},
};
use std::sync::Arc;

pub(in super::super) struct Plan {
    pub(in super::super) binding: UnitSymbolId,
    pub(in super::super) statement: UnitStatementId,
    pub(in super::super) condition: UnitExpressionId,
    pub(in super::super) body: UnitStatementId,
    pub(in super::super) completion: UnitStatementId,
    pub(in super::super) consumers: Vec<(UnitExpressionId, UnitExpressionId)>,
}
/// A single producer owns this ticket through entry, body return and normal exit.
/// No Clone implementation: a copied ValueState cannot duplicate boundary authority.
pub(in super::super) struct Token {
    pub(in super::super) plan: Plan,
    pub(in super::super) source: UnitLoanFact,
    pub(in super::super) begin: ActionHandle,
    pub(in super::super) handoff: ActionHandle,
    identity: Arc<()>,
    cursors: [usize; 4],
    stage: usize,
}
pub(super) struct Returned {
    loan: UnitLoanFact,
    begin: ActionHandle,
    cursor: usize,
}
pub(super) struct Candidate {
    token: Token,
    end: Option<ActionHandle>,
}
impl ActionSource {
    pub(in super::super) fn capture_loop_return(
        &mut self,
        loan: &UnitLoanFact,
        begin: Option<&ActionHandle>,
    ) {
        if self.enabled
            && let Some(begin) = begin
        {
            self.loop_returns.push(Returned {
                loan: loan.clone(),
                begin: begin.clone(),
                cursor: self.records.len(),
            });
        }
    }

    pub(in super::super) fn begin_loop_invariant(
        &self,
        plan: Plan,
        source: UnitLoanFact,
        begin: ActionHandle,
        handoff: ActionHandle,
    ) -> Option<Token> {
        if !self.enabled
            || plan.consumers.is_empty()
            || !Arc::ptr_eq(&begin.identity, &self.identity)
            || !Arc::ptr_eq(&handoff.identity, &self.identity)
            || begin.index >= handoff.index
            || handoff.index >= self.records.len()
            || !matches!(self.records.get(begin.index), Some(Record::Begin(actual)) if actual == &source)
        {
            return None;
        }
        Some(Token {
            plan,
            source,
            begin,
            handoff,
            identity: self.identity.clone(),
            cursors: [self.records.len(); 4],
            stage: 1,
        })
    }
    pub(in super::super) fn advance_loop_invariant(&self, token: &mut Token, phase: usize) -> bool {
        if !Arc::ptr_eq(&token.identity, &self.identity)
            || token.stage != phase
            || !(1..=3).contains(&phase)
        {
            return false;
        }
        token.cursors[phase] = self.records.len();
        token.stage += 1;
        true
    }
    pub(in super::super) fn complete_loop_invariant(&mut self, token: Token) {
        if token.stage == 4 && Arc::ptr_eq(&token.identity, &self.identity) {
            self.loop_invariants.push(Candidate { token, end: None });
        }
    }
    pub(super) fn close_loop_invariants(&mut self, fact: &BindingEnd, handle: &ActionHandle) {
        for candidate in &mut self.loop_invariants {
            if candidate.token.plan.binding == fact.binding() {
                if candidate.end.is_some()
                    || fact.point()
                        != UnitDropPoint::AfterStatement(candidate.token.plan.completion)
                {
                    candidate.token.stage = 0;
                } else {
                    candidate.end = Some(handle.clone());
                }
            }
        }
    }
    pub(super) fn validate_loop_invariants(&self) -> Vec<UnitLoanLoopInvariantLineage> {
        self.loop_invariants
            .iter()
            .filter_map(|c| self.validate_loop_invariant(c))
            .collect()
    }
    fn validate_loop_invariant(&self, c: &Candidate) -> Option<UnitLoanLoopInvariantLineage> {
        let t = &c.token;
        let end = c.end.as_ref()?;
        if t.stage != 4
            || !Arc::ptr_eq(&t.identity, &self.identity)
            || !Arc::ptr_eq(&end.identity, &self.identity)
            || !Arc::ptr_eq(&t.begin.identity, &self.identity)
            || !Arc::ptr_eq(&t.handoff.identity, &self.identity)
            || t.cursors[0] > t.cursors[1]
            || t.cursors[1] >= t.cursors[2]
            || t.cursors[2] > t.cursors[3]
            || t.cursors[3] > end.index
            || t.handoff.index >= t.cursors[0]
            || !matches!(self.records.get(t.begin.index),Some(Record::Begin(loan)) if loan==&t.source)
            || !matches!(self.records.get(t.handoff.index),Some(Record::BorrowHandoffCommitted(h)) if h.binding()==t.plan.binding && h.source_loan()==&t.source)
            || !matches!(self.records.get(end.index),Some(Record::BorrowBindingEnd(e)) if e.binding()==t.plan.binding && e.point()==UnitDropPoint::AfterStatement(t.plan.completion))
        {
            return None;
        }
        if self
            .records
            .iter()
            .filter(
                |r| matches!(r,Record::BorrowHandoffCommitted(h) if h.binding()==t.plan.binding),
            )
            .count()
            != 1
        {
            return None;
        }
        let interval = self.records.get(t.cursors[1]..t.cursors[2])?;
        // Every recorded body action is accounted for; normal-return evidence
        // comes only from removal of the actual pending call token.
        let mut uses = Vec::new();
        let mut actual_returns = Vec::new();
        let mut legacy_returns = 0;
        for (offset, record) in interval.iter().enumerate() {
            match record {
                Record::Begin(loan) => {
                    let index = t.cursors[1] + offset;
                    if self.records.iter().filter(|record|matches!(record,Record::Begin(other) if other.call()==loan.call() && other.argument()==loan.argument())).count()!=1 {return None;}
                    let mut returns = self.loop_returns.iter().filter(|r| r.begin.index == index);
                    let returned = returns.next()?;
                    if returns.next().is_some()
                        || returned.loan != *loan
                        || !Arc::ptr_eq(&returned.begin.identity, &self.identity)
                        || returned.cursor <= index
                        || returned.cursor > t.cursors[2]
                        || uses.last().is_some_and(|(_, cursor)| *cursor > index)
                    {
                        return None;
                    }
                    uses.push(((loan.call(), loan.argument()), returned.cursor));
                    actual_returns.push(returned);
                }
                Record::BorrowBindingUseReturned(returned) => {
                    legacy_returns += 1;
                    if legacy_returns > 1 {
                        return None;
                    }
                    let (pair, cursor) = uses.last()?;
                    if returned.binding!=t.plan.binding || (returned.loan.call(),returned.loan.argument())!=*pair
                            || *cursor>t.cursors[1]+offset
                            || !returned.source_begin.as_ref().is_some_and(|h|h.same_record(&t.begin))
                            || !returned.handoff.as_ref().is_some_and(|h|h.same_record(&t.handoff))
                            || !returned.begin.as_ref().is_some_and(|h|Arc::ptr_eq(&h.identity,&self.identity) && matches!(self.records.get(h.index),Some(Record::Begin(l)) if l==&returned.loan)) {return None;}
                }
                _ => return None,
            }
        }
        if uses.iter().map(|(pair, _)| *pair).collect::<Vec<_>>() != t.plan.consumers
            || self
                .records
                .iter()
                .filter(|r| matches!(r,Record::BorrowBindingEnd(e) if e.binding()==t.plan.binding))
                .count()
                != 1
        {
            return None;
        }
        let body_returns = self
            .loop_returns
            .iter()
            .filter(|r| r.begin.index >= t.cursors[1] && r.begin.index < t.cursors[2])
            .collect::<Vec<_>>();
        if body_returns.len() != actual_returns.len()
            || body_returns
                .iter()
                .zip(&actual_returns)
                .any(|(a, b)| !std::ptr::eq(*a, *b))
        {
            return None;
        }
        let id = |h: &ActionHandle| UnitLoanActionId {
            identity: self.identity.clone(),
            index: h.index,
        };
        Some(UnitLoanLoopInvariantLineage {
            identity: self.identity.clone(),
            binding: t.plan.binding,
            statement: t.plan.statement,
            condition: t.plan.condition,
            body: t.plan.body,
            source_begin: id(&t.begin),
            handoff: id(&t.handoff),
            binding_end: id(end),
            uses: actual_returns
                .iter()
                .map(|r| UnitBindingUseReturn {
                    loan: r.loan.clone(),
                    begin: id(&r.begin),
                    cursor: r.cursor,
                })
                .collect(),
            cursors: t.cursors,
        })
    }
}

#[cfg(test)]
pub(super) mod tests;
