//! Two-root normal-return relations from real pending handles and commit-confirmed owner snapshots.
//! Owner snapshots are trusted producer inputs, not reconstructed from matching source spans.
use super::super::borrow_call_handoff::distinct_auxiliary::Witness;
use super::{ActionHandle, ActionSource, Record};
use crate::{
    name_resolution::SourceUnitId,
    ownership_checking::{
        LoanKind, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction,
        UnitLoanDistinctAuxiliaryRetirement, UnitLoanFact, UnitLoanTarget, UnitOwnerCallable,
        UnitOwnerTransportEvent,
    },
    source::SourceId,
};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Candidate {
    handoff: ActionHandle,
    source: Option<ActionHandle>,
    auxiliaries: Vec<Option<ActionHandle>>,
    source_live: bool,
    witness: Option<Witness>,
    owner_drop: Option<UnitOwnerTransportEvent>,
}
impl ActionSource {
    pub(in super::super) fn register_distinct_auxiliary_retirement(
        &mut self,
        handoff: ActionHandle,
        source: Option<ActionHandle>,
        auxiliaries: Vec<Option<ActionHandle>>,
        source_live: bool,
        witness: Option<Witness>,
        owner_drop: Option<UnitOwnerTransportEvent>,
    ) {
        if self.enabled {
            self.distinct_auxiliary_retirements.push(Candidate {
                handoff,
                source,
                auxiliaries,
                source_live,
                witness,
                owner_drop,
            });
        }
    }

    pub(super) fn validate_distinct_auxiliary_retirements(
        &self,
        source_unit: SourceUnitId,
        file: SourceId,
    ) -> Vec<UnitLoanDistinctAuxiliaryRetirement> {
        #[cfg(test)]
        tests::record(self, source_unit, file);
        self.distinct_auxiliary_retirements
            .iter()
            .filter_map(|candidate| self.validate_distinct_candidate(candidate, source_unit, file))
            .collect()
    }

    fn validate_distinct_candidate(
        &self,
        c: &Candidate,
        source_unit: SourceUnitId,
        file: SourceId,
    ) -> Option<UnitLoanDistinctAuxiliaryRetirement> {
        let local = |loan: &UnitLoanFact| {
            loan.kind() == LoanKind::Shared
                && loan.call().source_unit() == source_unit
                && loan.argument().source_unit() == source_unit
                && loan.begin_span().source_id() == file
                && loan.end_span().source_id() == file
                && matches!(loan.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root().source_unit() == source_unit)
        };

        let witness = c.witness.as_ref()?;
        // The publication boundary checks the complete observed owner-event suffix,
        // not merely the producer's selected Drop event.
        match (witness.owner_events.as_slice(), c.owner_drop.as_ref()) {
            ([], None) => {}
            ([actual], Some(expected)) if actual == expected => {}
            _ => return None,
        }
        let source = c.source.as_ref()?;
        let [Some(auxiliary)] = c.auxiliaries.as_slice() else {
            return None;
        };
        if !c.source_live
            || witness.source_parameter > 1
            || !Arc::ptr_eq(&c.handoff.identity, &self.identity)
            || !Arc::ptr_eq(&source.identity, &self.identity)
            || !Arc::ptr_eq(&auxiliary.identity, &self.identity)
            || source.same_record(auxiliary)
            || source.index >= c.handoff.index
            || auxiliary.index >= c.handoff.index
            || !self.distinct_handles_unique(c, source, auxiliary)
        {
            return None;
        }
        // Chronology binds the explicit typed parameter role; source-second stays auxiliary-first.
        let (first, second) = if witness.source_parameter == 0 {
            (source.index, auxiliary.index)
        } else {
            (auxiliary.index, source.index)
        };
        if first.checked_add(1) != Some(second) {
            return None;
        }
        let Record::BorrowHandoffCommitted(fact) = self.records.get(c.handoff.index)? else {
            return None;
        };
        let Record::Begin(original_source) = self.records.get(source.index)? else {
            return None;
        };
        let Record::Begin(original_auxiliary) = self.records.get(auxiliary.index)? else {
            return None;
        };
        let [retired] = fact.auxiliary_loan_ends() else {
            return None;
        };
        let (UnitLoanTarget::Place(source_place), UnitLoanTarget::Place(auxiliary_place)) =
            (original_source.target(), original_auxiliary.target())
        else {
            return None;
        };
        let source_owner = witness.source_owner.owner.as_ref()?;
        let auxiliary_owner = witness.auxiliary_owner.owner.as_ref()?;
        let point = UnitDropPoint::CallReturn(original_source.call());
        let after = second.checked_add(1)?;
        let window = self.records.get(after..c.handoff.index)?;
        let auxiliary_drop = match (window, c.owner_drop.as_ref()) {
            ([], None) => None,
            (
                [
                    Record::Cleanup((
                        super::PlannerDropPoint::CallReturn(call),
                        UnitIterationCleanupAction::Drop(drop),
                    )),
                ],
                Some(UnitOwnerTransportEvent::Drop {
                    point: event_point,
                    symbol,
                    owner,
                }),
            ) if *call == original_source.call().expression()
                && drop.point() == point
                && drop.target() == UnitDropTarget::Named(witness.auxiliary_owner.symbol)
                && drop.value_origin() == witness.auxiliary_owner.origin
                && *event_point == point
                && *symbol == witness.auxiliary_owner.symbol
                && owner == auxiliary_owner =>
            {
                Some(after)
            }
            _ => return None,
        };
        if witness.call != original_source.call()
                || fact.source_loan() != original_source || retired != original_auxiliary
                || !local(original_source) || !local(original_auxiliary)
                || fact.binding().source_unit() != source_unit
                || original_source.call() != original_auxiliary.call()
                || original_source.argument() == original_auxiliary.argument()
                || original_source.target() == original_auxiliary.target()
                || fact.point() != UnitDropPoint::CallReturn(original_source.call())
                || source_owner == auxiliary_owner
                || !Arc::ptr_eq(&source_owner.identity, &auxiliary_owner.identity)
                || source_owner.callable != auxiliary_owner.callable
                || !matches!(source_owner.callable, UnitOwnerCallable::Function(callable) if callable.source_unit() == source_unit)
                || witness.source_owner.symbol != source_place.root()
                || witness.auxiliary_owner.symbol != auxiliary_place.root()
                || witness.source_owner.symbol == witness.auxiliary_owner.symbol
                || [&witness.source_owner, &witness.auxiliary_owner].iter().any(|owner|
                    owner.origin.source_id() != file || owner.declaration.source_id() != file || owner.scope_depth != 1)
                || self.records.iter().filter(|r| matches!(r, Record::Begin(b) if b == original_source)).count() != 1
                || self.records.iter().filter(|r| matches!(r, Record::Begin(b) if b == original_auxiliary)).count() != 1
                || self.records.iter().filter(|r| matches!(r, Record::BorrowHandoffCommitted(other)
                    if other.source_loan().call() == original_source.call() || other.binding() == fact.binding())).count() != 1
                || self.records[..c.handoff.index].iter().any(|r|
                    ended(r, original_source) || ended(r, original_auxiliary))
            { return None; }
        Some(UnitLoanDistinctAuxiliaryRetirement {
            identity: self.identity.clone(),
            source_begin: source.index,
            auxiliary_begin: auxiliary.index,
            handoff: c.handoff.index,
            source_owner: source_owner.clone(),
            auxiliary_owner: auxiliary_owner.clone(),
            source_parameter: witness.source_parameter,
            call: original_source.call(),
            auxiliary_drop,
        })
    }

    fn distinct_handles_unique(
        &self,
        c: &Candidate,
        source: &ActionHandle,
        auxiliary: &ActionHandle,
    ) -> bool {
        let candidates = &self.distinct_auxiliary_retirements;
        candidates
            .iter()
            .filter_map(|c| c.source.as_ref())
            .filter(|h| h.same_record(source))
            .count()
            == 1
            && candidates
                .iter()
                .flat_map(|c| &c.auxiliaries)
                .flatten()
                .filter(|h| h.same_record(auxiliary))
                .count()
                == 1
            && !candidates
                .iter()
                .flat_map(|c| &c.auxiliaries)
                .flatten()
                .any(|h| h.same_record(source))
            && !candidates
                .iter()
                .filter_map(|c| c.source.as_ref())
                .any(|h| h.same_record(auxiliary))
            && candidates
                .iter()
                .filter(|other| other.handoff.same_record(&c.handoff))
                .count()
                == 1
    }
}
fn ended(record: &Record, loan: &UnitLoanFact) -> bool {
    match record {
        Record::Cleanup((_, UnitIterationCleanupAction::EndCallLoan(actual))) => actual == loan,
        Record::BorrowHandoffCommitted(fact) => fact.auxiliary_loan_ends().contains(loan),
        Record::BorrowBindingSourceRetired(retirement) => {
            retirement.transaction.source_loan() == loan
        }
        Record::BorrowConsumerRetired { transaction, .. } => {
            transaction.ended_loans().contains(&loan)
        }
        _ => false,
    }
}
#[cfg(test)]
mod tests;
