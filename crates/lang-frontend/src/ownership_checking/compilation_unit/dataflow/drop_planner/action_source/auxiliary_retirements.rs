//! Validate carried pending-token identities, never reconstruct acquisitions from facts.
use super::{ActionHandle, ActionSource, Record};
use crate::{
    name_resolution::SourceUnitId,
    ownership_checking::{LoanKind, UnitLoanAuxiliaryRetirement, UnitLoanFact, UnitLoanTarget},
    source::SourceId,
};
use std::sync::Arc;

pub(super) struct Candidate {
    handoff: ActionHandle,
    source: Option<ActionHandle>,
    auxiliaries: Vec<Option<ActionHandle>>,
    witnessed: bool,
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn register_auxiliary_retirement(
        &mut self,
        handoff: ActionHandle,
        source: Option<ActionHandle>,
        auxiliaries: Vec<Option<ActionHandle>>,
        witnessed: bool,
    ) {
        if self.enabled {
            self.auxiliary_retirements.push(Candidate {
                handoff,
                source,
                auxiliaries,
                witnessed,
            });
        }
    }

    pub(super) fn validate_auxiliary_retirements(
        &self,
        source_unit: SourceUnitId,
        file: SourceId,
    ) -> Vec<UnitLoanAuxiliaryRetirement> {
        let local = |loan: &UnitLoanFact| {
            loan.kind() == LoanKind::Shared
                && loan.call().source_unit() == source_unit
                && loan.argument().source_unit() == source_unit
                && loan.begin_span().source_id() == file
                && loan.end_span().source_id() == file
                && matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root().source_unit() == source_unit)
        };
        self.auxiliary_retirements
            .iter()
            .filter_map(|candidate| {
                if !candidate.witnessed || !Arc::ptr_eq(&candidate.handoff.identity, &self.identity)
                {
                    return None;
                }
                let source = candidate.source.as_ref()?;
                let [Some(auxiliary)] = candidate.auxiliaries.as_slice() else {
                    return None;
                };
                let handoff = candidate.handoff.index;
                if !Arc::ptr_eq(&source.identity, &self.identity)
                    || !Arc::ptr_eq(&auxiliary.identity, &self.identity)
                    || source.index >= handoff
                    || auxiliary.index >= handoff
                    || source.same_record(auxiliary)
                    || self
                        .auxiliary_retirements
                        .iter()
                        .filter_map(|other| other.source.as_ref())
                        .filter(|other| other.same_record(source))
                        .count()
                        != 1
                    || self
                        .auxiliary_retirements
                        .iter()
                        .flat_map(|other| &other.auxiliaries)
                        .flatten()
                        .any(|other| other.same_record(source))
                    || self
                        .auxiliary_retirements
                        .iter()
                        .filter_map(|other| other.source.as_ref())
                        .any(|other| other.same_record(auxiliary))
                    || self
                        .auxiliary_retirements
                        .iter()
                        .filter(|other| other.handoff.same_record(&candidate.handoff))
                        .count()
                        != 1
                    || self
                        .auxiliary_retirements
                        .iter()
                        .flat_map(|other| &other.auxiliaries)
                        .flatten()
                        .filter(|other| other.same_record(auxiliary))
                        .count()
                        != 1
                {
                    return None;
                }
                let Record::BorrowHandoffCommitted(fact) = self.records.get(handoff)? else {
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
                if fact.source_loan() != original_source
                    || retired != original_auxiliary
                    || !local(original_source)
                    || !local(original_auxiliary)
                    || fact.binding().source_unit() != source_unit
                    || original_source.call() != original_auxiliary.call()
                    || original_source.argument() == original_auxiliary.argument()
                    || original_source.target() != original_auxiliary.target()
                {
                    return None;
                }
                Some(UnitLoanAuxiliaryRetirement {
                    identity: self.identity.clone(),
                    source_begin: source.index,
                    auxiliary_begin: auxiliary.index,
                    handoff,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
