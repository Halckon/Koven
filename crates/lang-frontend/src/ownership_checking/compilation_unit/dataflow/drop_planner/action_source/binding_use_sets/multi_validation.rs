//! Validate the unfiltered original interval against independently captured return causes.
use super::{Candidate, UseRole};
use crate::ownership_checking::{
    LoanKind, UnitBorrowMultiBindingRegion, UnitDropPoint, UnitDropTarget,
    UnitIterationCleanupAction, UnitLoanAction, UnitLoanActionId, UnitLoanBindingUseSetRetirement,
    UnitMultiBindingSourceState, UnitOwnerCallable, UnitOwnerTransportEvent,
};
use std::sync::Arc;

impl Candidate {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn complete_region(
        &self,
        identity: &Arc<()>,
        actions: &[UnitLoanAction],
        retirement: &UnitLoanBindingUseSetRetirement,
    ) -> Option<UnitBorrowMultiBindingRegion> {
        let e = &self.expected;
        let m = &self.multi;
        let source_owner = e.source_owner.owner.as_ref()?;
        let auxiliary_owner = e.auxiliary_owner.owner.as_ref()?;
        let auxiliary_drop = m.drop.as_ref()?;
        let exit = m.exit?;
        let interval = actions.get(self.start..exit)?;
        let a_count = e.auxiliary_uses.len();
        let b_count = e.consumers.len();
        let set = retirement.use_set();
        let at = |id: &UnitLoanActionId, index: usize| {
            Arc::ptr_eq(&id.identity, identity) && id.index == index
        };
        if !self.valid
            || self.source_invalid
            || !e.valid()
            || m.invalid
            || !m.claimed
            || m.removed.as_ref() != Some(&e.auxiliary_owner)
            || e.auxiliary_statements.len() != a_count
            || e.ordered_uses.len() != a_count + b_count
            || m.uses.len() != a_count
            || set.uses() != self.returns
            || !Arc::ptr_eq(&set.identity, identity)
            || set.callable() != e.scope.callable
            || set.binding() != e.scope.binding
            || e.source_owner.symbol != e.scope.root
            || e.source_owner.scope_depth != 1
            || e.auxiliary_owner.scope_depth != 1
            || e.source_owner.symbol == e.auxiliary_owner.symbol
            || source_owner == auxiliary_owner
            || source_owner.callable != UnitOwnerCallable::Function(e.scope.callable)
            || auxiliary_owner.callable != source_owner.callable
            || !Arc::ptr_eq(&source_owner.identity, &auxiliary_owner.identity)
            || retirement.source_owner() != source_owner
            || !Arc::ptr_eq(&auxiliary_drop.identity, identity)
            || interval.len() != 7 + b_count + 2 * a_count
            || !at(&set.source_begin(), self.start.checked_add(e.source_index)?)
            || !at(&set.handoff(), self.start.checked_add(2)?)
        {
            return None;
        }
        let (removed, actual_source_drop) = self.source_cleanup.as_ref()?;
        if removed != &e.source_owner
            || !at(&retirement.source_drop(), actual_source_drop.index)
            || !Arc::ptr_eq(&actual_source_drop.identity, identity)
            || !self.end.as_ref().is_some_and(|end| {
                at(&set.binding_end(), end.index) && Arc::ptr_eq(&end.identity, identity)
            })
        {
            return None;
        }
        let [
            UnitLoanAction::Begin(first),
            UnitLoanAction::Begin(second),
            UnitLoanAction::BorrowHandoffCommitted(handoff),
        ] = interval.get(..3)?
        else {
            return None;
        };
        let (source, auxiliary) = if e.source_index == 0 {
            (first, second)
        } else {
            (second, first)
        };
        if source != &e.source
            || auxiliary != &e.auxiliary
            || handoff.binding() != e.scope.binding
            || handoff.source_loan() != source
            || handoff.auxiliary_loan_ends() != std::slice::from_ref(auxiliary)
        {
            return None;
        }
        if !matches!(auxiliary.target(), crate::ownership_checking::UnitLoanTarget::Place(place) if place.is_root() && place.root()==e.auxiliary_owner.symbol)
            || e.auxiliary_uses.iter().enumerate().any(|(i, loan)| {
                loan.call().source_unit() != e.scope.callable.source_unit()
                    || loan.argument().source_unit() != e.scope.callable.source_unit()
                    || loan.call() == source.call()
                    || e.auxiliary_uses[..i].iter().any(|prior| {
                        prior.call() == loan.call() || prior.argument() == loan.argument()
                    })
            })
        {
            return None;
        }
        let mut cursor = self.start.checked_add(3)?;
        let mut next_a = 0;
        let mut next_b = 0;
        let mut expected_events = Vec::new();
        let mut statements = Vec::new();
        for role in &e.ordered_uses {
            match *role {
                UseRole::Binding(index) => {
                    if index != next_b {
                        return None;
                    }
                    let loan = e.consumers.get(index)?;
                    let returned = self.returns.get(index)?;
                    if !matches!(actions.get(cursor),Some(UnitLoanAction::Begin(actual)) if actual==loan)
                        || returned.loan() != loan
                        || !at(&returned.begin(), cursor)
                        || returned.return_cursor() != cursor.checked_add(1)?
                    {
                        return None;
                    }
                    statements.push(*e.consumer_statements.get(index)?);
                    let begin = cursor;
                    cursor = cursor.checked_add(1)?;
                    if index == 0 {
                        let Some(UnitLoanAction::BorrowBindingUseReturned(old)) =
                            actions.get(cursor)
                        else {
                            return None;
                        };
                        if old.binding() != e.scope.binding
                            || old.loan() != loan
                            || old.source_begin() != Some(set.source_begin())
                            || old.handoff() != Some(set.handoff())
                            || old.begin().is_none_or(|id| !at(&id, begin))
                        {
                            return None;
                        }
                        cursor = cursor.checked_add(1)?;
                    }
                    next_b += 1;
                    if next_b == b_count {
                        if !at(&set.binding_end(), cursor)
                            || !matches!(actions.get(cursor),Some(UnitLoanAction::BorrowBindingEnd(end)) if end.binding()==e.scope.binding && end.point()==UnitDropPoint::AfterStatement(e.scope.consumer_statement))
                        {
                            return None;
                        }
                        cursor = cursor.checked_add(1)?;
                        if !at(&retirement.source_drop(), cursor)
                            || !exact_drop(
                                actions,
                                cursor,
                                UnitDropPoint::AfterStatement(e.scope.consumer_statement),
                                &e.source_owner,
                            )
                        {
                            return None;
                        }
                        expected_events.push(UnitOwnerTransportEvent::Drop {
                            point: UnitDropPoint::AfterStatement(e.scope.consumer_statement),
                            symbol: e.scope.root,
                            owner: source_owner.clone(),
                        });
                        cursor = cursor.checked_add(1)?;
                    }
                }
                UseRole::Auxiliary(index) => {
                    if index != next_a {
                        return None;
                    }
                    let loan = e.auxiliary_uses.get(index)?;
                    let actual = m.uses.get(index)?;
                    let end = cursor.checked_add(1)?;
                    if loan.kind() != LoanKind::Shared
                        || loan.target() != auxiliary.target()
                        || actual.loan() != loan
                        || actual.owner() != auxiliary_owner
                        || actual.returned_binding_uses() != next_b
                        || !at(&actual.begin(), cursor)
                        || !at(&actual.end(), end)
                        || !matches!(actions.get(cursor),Some(UnitLoanAction::Begin(l)) if l==loan)
                        || !matches!(actions.get(end),Some(UnitLoanAction::Cleanup{point,action:UnitIterationCleanupAction::EndCallLoan(l)}) if l==loan && *point==UnitDropPoint::CallReturn(loan.call()))
                    {
                        return None;
                    }
                    match actual.source_state() {
                        UnitMultiBindingSourceState::Active {
                            owner,
                            source_begin,
                            handoff,
                        } if next_b < b_count
                            && owner == source_owner
                            && *source_begin == set.source_begin()
                            && *handoff == set.handoff() => {}
                        UnitMultiBindingSourceState::Retired { binding_end, drop }
                            if next_b == b_count
                                && *binding_end == set.binding_end()
                                && *drop == retirement.source_drop() => {}
                        _ => return None,
                    }
                    statements.push(*e.auxiliary_statements.get(index)?);
                    cursor = end.checked_add(1)?;
                    next_a += 1;
                    if next_a == a_count {
                        if auxiliary_drop.index != cursor
                            || !exact_drop(
                                actions,
                                cursor,
                                UnitDropPoint::CallReturn(loan.call()),
                                &e.auxiliary_owner,
                            )
                        {
                            return None;
                        }
                        expected_events.push(UnitOwnerTransportEvent::Drop {
                            point: UnitDropPoint::CallReturn(loan.call()),
                            symbol: e.auxiliary_owner.symbol,
                            owner: auxiliary_owner.clone(),
                        });
                        cursor = cursor.checked_add(1)?;
                    }
                }
            }
        }
        if cursor != exit
            || next_a != a_count
            || next_b != b_count
            || statements.last() != Some(&e.completion)
            || m.owner_events != expected_events
            || statements.iter().enumerate().any(|(i, s)| {
                s.source_unit() != e.scope.callable.source_unit()
                    || *s == e.scope.entry
                    || *s == e.scope.completion
                    || statements[..i].contains(s)
            })
        {
            return None;
        }
        // Check whole-catalog uniqueness, including duplicates outside this interval.
        for loan in std::iter::once(&e.source)
            .chain(std::iter::once(&e.auxiliary))
            .chain(&e.consumers)
            .chain(&e.auxiliary_uses)
        {
            if actions.iter().filter(|a|matches!(a,UnitLoanAction::Begin(l) if l.call()==loan.call() && l.argument()==loan.argument())).count()!=1 {return None;}
        }
        for loan in e.consumers.iter().chain(&e.auxiliary_uses) {
            if actions.iter().filter(|a|matches!(a,UnitLoanAction::Begin(l) if l.call()==loan.call() || l.argument()==loan.argument())).count()!=1 { return None; }
        }
        for loan in &e.auxiliary_uses {
            if actions.iter().filter(|action| matches!(action,
                UnitLoanAction::Cleanup { action: UnitIterationCleanupAction::EndCallLoan(actual), .. }
                if actual.call() == loan.call() || actual.argument() == loan.argument())).count() != 1 {
                return None;
            }
        }
        if actions
            .iter()
            .filter(|action| {
                matches!(action,
            UnitLoanAction::BorrowBindingUseReturned(actual) if actual.binding() == e.scope.binding)
            })
            .count()
            != 1
        {
            return None;
        }
        for symbol in [e.source_owner.symbol, e.auxiliary_owner.symbol] {
            if actions.iter().filter(|a|matches!(a,UnitLoanAction::Cleanup{action:UnitIterationCleanupAction::Drop(d),..} if d.target()==UnitDropTarget::Named(symbol))).count()!=1 {return None;}
        }
        Some(UnitBorrowMultiBindingRegion {
            retirement: retirement.clone(),
            uses: m.uses.clone(),
            auxiliary_drop: UnitLoanActionId {
                identity: identity.clone(),
                index: auxiliary_drop.index,
            },
            ids: (self.start..exit)
                .map(|index| UnitLoanActionId {
                    identity: identity.clone(),
                    index,
                })
                .collect(),
            entry: e.scope.entry,
            completion: e.scope.completion,
        })
    }
}
fn exact_drop(
    actions: &[UnitLoanAction],
    index: usize,
    point: UnitDropPoint,
    owner:&crate::ownership_checking::compilation_unit::loan_binding_region_validation::OwnerSnapshot,
) -> bool {
    matches!(actions.get(index),Some(UnitLoanAction::Cleanup{point:actual,action:UnitIterationCleanupAction::Drop(drop)})
        if *actual==point && drop.point()==point && drop.target()==UnitDropTarget::Named(owner.symbol) && drop.value_origin()==owner.origin)
}
