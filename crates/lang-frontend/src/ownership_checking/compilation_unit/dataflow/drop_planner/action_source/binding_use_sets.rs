//! Independent pending-return collection. Does not publish old single-use retirement authority.
use super::{ActionHandle, ActionSource, Record};
use crate::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitBindingUseReturn, UnitBorrowBindingTransportFact, UnitDropPoint, UnitLoanAction,
        UnitLoanActionId, UnitLoanBindingUseSet, UnitLoanFact,
        compilation_unit::loan_binding_region_validation::Scope,
    },
    type_checking::UnitStatementId,
};
use std::sync::Arc;

#[derive(Clone)]
pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct Expected {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) source_owner:
        crate::ownership_checking::compilation_unit::loan_binding_region_validation::OwnerSnapshot,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) scope: Scope,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) source:
        UnitLoanFact,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) auxiliary:
        UnitLoanFact,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) source_index: usize,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) consumers:
        Vec<UnitLoanFact>,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) consumer_statements:
        Vec<UnitStatementId>,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) auxiliary_uses:
        Vec<UnitLoanFact>,
}
impl Expected {
    fn valid(&self) -> bool {
        let n = self.consumers.len();
        (2..=8).contains(&n)
            && (1..=8).contains(&self.auxiliary_uses.len())
            && self.consumer_statements.len() == n
            && self.source_index <= 1
            && self
                .consumers
                .last()
                .is_some_and(|l| l.call() == self.scope.consumer_call)
            && self.consumer_statements.last() == Some(&self.scope.consumer_statement)
            && self.consumers.iter().enumerate().all(|(i, l)| {
                l.target() == self.source.target()
                    && l.call() != self.source.call()
                    && self.consumers[..i]
                        .iter()
                        .all(|p| p.call() != l.call() && p.argument() != l.argument())
                    && self.auxiliary_uses.iter().all(|a| a.call() != l.call())
            })
    }
}
#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct Token {
    identity: Arc<()>,
    candidate: usize,
    next: usize,
}
#[derive(Clone)]
pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct Candidate {
    expected: Expected,
    source_invalid: bool,
    source_cleanup: Option<(
        crate::ownership_checking::compilation_unit::loan_binding_region_validation::OwnerSnapshot,
        ActionHandle,
    )>,
    start: usize,
    source: Option<ActionHandle>,
    handoff: Option<ActionHandle>,
    returns: Vec<UnitBindingUseReturn>,
    end: Option<ActionHandle>,
    valid: bool,
}
impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn enter_binding_use_set(
        &mut self,
        expected: Expected,
    ) {
        if !self.enabled || !expected.valid() {
            return;
        }
        let mut duplicate = false;
        for c in &mut self.binding_use_sets {
            if c.expected.scope.binding == expected.scope.binding {
                c.valid = false;
                duplicate = true;
            }
        }
        if duplicate {
            return;
        }
        self.binding_use_sets.push(Candidate {
            expected,
            source_invalid: false,
            source_cleanup: None,
            start: self.records.len(),
            source: None,
            handoff: None,
            returns: Vec::new(),
            end: None,
            valid: true,
        });
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn activate_binding_use_set(
        &mut self,
        binding: UnitSymbolId,
        source: &UnitLoanFact,
        begin: Option<&ActionHandle>,
        handoff: Option<&ActionHandle>,
    ) -> Option<Token> {
        let index = self
            .binding_use_sets
            .iter()
            .position(|c| c.expected.scope.binding == binding)?;
        let c = &mut self.binding_use_sets[index];
        let Some((begin, handoff)) = begin.zip(handoff) else {
            c.valid = false;
            return None;
        };
        if !c.valid
            || c.source.is_some()
            || c.handoff.is_some()
            || source != &c.expected.source
            || !Arc::ptr_eq(&begin.identity, &self.identity)
            || !Arc::ptr_eq(&handoff.identity, &self.identity)
            || Some(begin.index) != c.start.checked_add(c.expected.source_index)
            || Some(handoff.index) != c.start.checked_add(2)
            || !matches!(self.records.get(begin.index),Some(Record::Begin(l)) if l==source)
            || !matches!(self.records.get(handoff.index),Some(Record::BorrowHandoffCommitted(h)) if h.binding()==binding && h.source_loan()==source && h.auxiliary_loan_ends()==std::slice::from_ref(&c.expected.auxiliary))
        {
            c.valid = false;
            return None;
        }
        c.source = Some(begin.clone());
        c.handoff = Some(handoff.clone());
        Some(Token {
            identity: self.identity.clone(),
            candidate: index,
            next: 0,
        })
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn return_binding_use_set(
        &mut self,
        token: &mut Token,
        loan: &UnitLoanFact,
        begin: Option<&ActionHandle>,
    ) {
        let Some(c) = self.binding_use_sets.get_mut(token.candidate) else {
            return;
        };
        if c.expected.auxiliary_uses.contains(loan) {
            return;
        }
        if !Arc::ptr_eq(&token.identity, &self.identity)
            || !c.valid
            || c.end.is_some()
            || token.next != c.returns.len()
            || c.expected.consumers.get(token.next) != Some(loan)
        {
            c.valid = false;
            return;
        }
        let Some(begin) = begin else {
            c.valid = false;
            return;
        };
        if !Arc::ptr_eq(&begin.identity, &self.identity)
            || !c.handoff.as_ref().is_some_and(|h| h.precedes(begin))
            || begin.index.checked_add(1) != Some(self.records.len())
            || !matches!(self.records.get(begin.index),Some(Record::Begin(actual)) if actual==loan)
            || c.returns
                .last()
                .is_some_and(|prior| prior.cursor > begin.index)
        {
            c.valid = false;
            return;
        }
        c.returns.push(UnitBindingUseReturn {
            loan: loan.clone(),
            begin: UnitLoanActionId {
                identity: begin.identity.clone(),
                index: begin.index,
            },
            cursor: self.records.len(),
        });
        token.next += 1;
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn close_binding_use_set(
        &mut self,
        token: Token,
        transaction: &UnitBorrowBindingTransportFact,
        end: &ActionHandle,
    ) -> Option<super::BindingDropCause> {
        let c = self.binding_use_sets.get_mut(token.candidate)?;
        if !Arc::ptr_eq(&token.identity, &self.identity)
            || !Arc::ptr_eq(&end.identity, &self.identity)
            || !c.valid
            || c.end.is_some()
            || token.next != c.expected.consumers.len()
            || c.returns.len() != token.next
            || transaction.binding() != c.expected.scope.binding
            || transaction.source_loan() != &c.expected.source
            || transaction.point()
                != UnitDropPoint::AfterStatement(c.expected.scope.consumer_statement)
            || !matches!(self.records.get(end.index),Some(Record::BorrowBindingEnd(e)) if e.binding==transaction.binding() && e.point==transaction.point())
            || c.returns.last().is_none_or(|r| r.cursor > end.index)
        {
            c.valid = false;
            return None;
        }
        c.end = Some(end.clone());
        Some(super::BindingDropCause::UseSet(UseSetDropCause {
            identity: self.identity.clone(),
            candidate: token.candidate,
            end: end.clone(),
            removed: None,
        }))
    }
}
impl Candidate {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn finish(
        self,
        identity: &Arc<()>,
        actions: &[UnitLoanAction],
    ) -> Option<UnitLoanBindingUseSet> {
        #[cfg(test)]
        tests::record(&self, identity, actions);
        let (source, handoff, end) = (
            self.source.as_ref()?,
            self.handoff.as_ref()?,
            self.end.as_ref()?,
        );
        if !self.valid
            || !self.expected.valid()
            || self.returns.len() != self.expected.consumers.len()
            || [source, handoff, end]
                .iter()
                .any(|h| !Arc::ptr_eq(&h.identity, identity))
            || Some(source.index) != self.start.checked_add(self.expected.source_index)
            || Some(handoff.index) != self.start.checked_add(2)
            || !matches!(actions.get(source.index),Some(UnitLoanAction::Begin(l)) if l==&self.expected.source)
            || !matches!(actions.get(handoff.index),Some(UnitLoanAction::BorrowHandoffCommitted(h)) if h.binding()==self.expected.scope.binding && h.source_loan()==&self.expected.source && h.auxiliary_loan_ends()==std::slice::from_ref(&self.expected.auxiliary))
            || !matches!(actions.get(end.index),Some(UnitLoanAction::BorrowBindingEnd(e)) if e.binding()==self.expected.scope.binding && e.point()==UnitDropPoint::AfterStatement(self.expected.scope.consumer_statement))
        {
            return None;
        }
        if actions.iter().filter(|a|matches!(a, UnitLoanAction::BorrowHandoffCommitted(h) if h.binding()==self.expected.scope.binding)).count()!=1
            || actions.iter().filter(|a|matches!(a, UnitLoanAction::BorrowBindingEnd(e) if e.binding()==self.expected.scope.binding)).count()!=1 {
            return None;
        }
        for (i, (r, l)) in self
            .returns
            .iter()
            .zip(&self.expected.consumers)
            .enumerate()
        {
            if actions.iter().filter(|a|matches!(a,UnitLoanAction::Begin(actual) if actual.call()==l.call() || actual.argument()==l.argument())).count()!=1
                || &r.loan != l
                || !Arc::ptr_eq(&r.begin.identity, identity)
                || r.begin.index.checked_add(1) != Some(r.cursor)
                || r.begin.index <= handoff.index
                || r.cursor > end.index
                || i > 0 && self.returns[i - 1].cursor > r.begin.index
                || !matches!(actions.get(r.begin.index),Some(UnitLoanAction::Begin(actual)) if actual==l)
            {
                return None;
            }
        }
        Some(UnitLoanBindingUseSet {
            identity: identity.clone(),
            callable: self.expected.scope.callable,
            binding: self.expected.scope.binding,
            source_begin: UnitLoanActionId {
                identity: identity.clone(),
                index: source.index,
            },
            handoff: UnitLoanActionId {
                identity: identity.clone(),
                index: handoff.index,
            },
            binding_end: UnitLoanActionId {
                identity: identity.clone(),
                index: end.index,
            },
            uses: self.returns,
        })
    }
}

#[cfg(test)]
mod tests;

/// One-shot cause from actual transport consumption, not an old single-use retirement handle.
pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct UseSetDropCause {
    identity: Arc<()>,
    candidate: usize,
    end: ActionHandle,
    removed: Option<
        crate::ownership_checking::compilation_unit::loan_binding_region_validation::OwnerSnapshot,
    >,
}
impl ActionSource {
    pub(super) fn verify_removed_use_set_owner(
        &mut self,
        mut cause: UseSetDropCause,
        value: &super::super::OwnedValue,
    ) -> Option<UseSetDropCause> {
        use crate::ownership_checking::compilation_unit::loan_binding_region_validation::OwnerSnapshot;
        let c = self.binding_use_sets.get_mut(cause.candidate)?;
        let actual = OwnerSnapshot {
            owner: value.owner.clone(),
            symbol: value.symbol,
            origin: value.origin,
            declaration: value.declaration,
            scope_depth: value.scope_depth,
        };
        if !Arc::ptr_eq(&cause.identity, &self.identity)
            || c.source_invalid
            || cause.removed.is_some()
            || c.source_cleanup.is_some()
            || !c
                .end
                .as_ref()
                .is_some_and(|end| end.same_record(&cause.end))
            || actual.owner.is_none()
            || actual != c.expected.source_owner
        {
            c.source_invalid = true;
            return None;
        }
        cause.removed = Some(actual);
        Some(cause)
    }
    pub(super) fn record_use_set_source_drop(
        &mut self,
        cause: UseSetDropCause,
        drop: ActionHandle,
    ) {
        let Some(c) = self.binding_use_sets.get_mut(cause.candidate) else {
            return;
        };
        let Some(owner) = cause.removed else {
            c.source_invalid = true;
            return;
        };
        if !Arc::ptr_eq(&cause.identity, &self.identity)
            || c.source_invalid
            || c.source_cleanup.is_some()
            || owner != c.expected.source_owner
            || !c
                .end
                .as_ref()
                .is_some_and(|end| end.same_record(&cause.end))
            || !cause.end.precedes(&drop)
        {
            c.source_invalid = true;
            return;
        }
        c.source_cleanup = Some((owner, drop));
    }
}
impl Candidate {
    pub(super) fn source_retirement(
        &self,
        identity: &Arc<()>,
        actions: &[UnitLoanAction],
    ) -> Option<(crate::ownership_checking::UnitOwnerId, UnitLoanActionId)> {
        use crate::ownership_checking::{UnitDropTarget, UnitIterationCleanupAction};
        let (owner, drop) = self.source_cleanup.as_ref()?;
        let end = self.end.as_ref()?;
        let expected = &self.expected.source_owner;
        let owner_id = owner.owner.as_ref()?;
        if self.source_invalid || owner!=expected || expected.symbol!=self.expected.scope.root
            || expected.scope_depth!=1 || owner_id.callable!=crate::ownership_checking::UnitOwnerCallable::Function(self.expected.scope.callable)
            || !Arc::ptr_eq(&drop.identity,identity) || !end.precedes(drop)
            || !matches!(actions.get(drop.index),Some(UnitLoanAction::Cleanup{point,action:UnitIterationCleanupAction::Drop(d)})
                if *point==UnitDropPoint::AfterStatement(self.expected.scope.consumer_statement)
                && d.point()==*point && d.target()==UnitDropTarget::Named(expected.symbol) && d.value_origin()==expected.origin)
            || actions.iter().filter(|a|matches!(a,UnitLoanAction::Cleanup{action:UnitIterationCleanupAction::Drop(d),..} if d.target()==UnitDropTarget::Named(expected.symbol))).count()!=1 {
            return None;
        }
        Some((
            owner_id.clone(),
            UnitLoanActionId {
                identity: drop.identity.clone(),
                index: drop.index,
            },
        ))
    }
}
