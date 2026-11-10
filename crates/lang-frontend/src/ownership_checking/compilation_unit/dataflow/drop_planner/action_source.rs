//! The cleanup projections and acquisition observation share one append-only source.
use super::PlannerDropPoint;
use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::{
    BorrowBindingEndFact, UnitBorrowCallHandoffFact, UnitDropPoint, UnitIterationCleanupAction,
    UnitLoanFact,
};
type BindingEnd = BorrowBindingEndFact<UnitSymbolId, UnitDropPoint>;

pub(super) struct ActionSource {
    records: Vec<Record>,
    cleanups: usize,
    regions: Vec<crate::ownership_checking::compilation_unit::loan_region_validation::Candidate>,
    pub(super) enabled: bool,
}

enum Record {
    Begin(UnitLoanFact),
    ReceiverBegin(crate::ownership_checking::UnitReceiverOwnershipFact),
    ReceiverEnd(crate::ownership_checking::UnitReceiverCallReturnEndFact),
    BorrowHandoffCommitted(UnitBorrowCallHandoffFact),
    BorrowBindingEnd(BindingEnd),
    Cleanup((PlannerDropPoint, UnitIterationCleanupAction)),
}

impl Default for ActionSource {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            cleanups: 0,
            regions: Vec::new(),
            enabled: true,
        }
    }
}

impl ActionSource {
    pub(super) fn enter_region(
        &mut self,
        mut candidate: crate::ownership_checking::compilation_unit::loan_region_validation::Candidate,
    ) -> usize {
        let index = self.regions.len();
        // Only actual planner entry/completion owns these cursors, never admission.
        candidate.start = self.records.len();
        candidate.end = None;
        self.regions.push(candidate);
        index
    }
    pub(super) fn complete_region(&mut self, index: usize) {
        self.regions[index].end = Some(self.records.len());
    }

    pub(super) fn push(&mut self, cleanup: (PlannerDropPoint, UnitIterationCleanupAction)) {
        self.records.push(Record::Cleanup(cleanup));
        self.cleanups += 1;
    }

    pub(super) fn begin(&mut self, loan: UnitLoanFact) {
        if self.enabled {
            self.records.push(Record::Begin(loan));
        }
    }

    pub(super) fn begin_receiver(
        &mut self,
        receiver: crate::ownership_checking::UnitReceiverOwnershipFact,
    ) {
        if self.enabled
            && receiver.kind() == crate::ownership_checking::UnitReceiverOwnershipKind::SharedLoan
        {
            self.records.push(Record::ReceiverBegin(receiver));
        }
    }

    pub(super) fn end_receiver(
        &mut self,
        end: crate::ownership_checking::UnitReceiverCallReturnEndFact,
    ) {
        // Kept with observation disabled: the legacy table is a projection of this source.
        self.records.push(Record::ReceiverEnd(end));
    }

    pub(super) fn receiver_return_ends(
        &self,
    ) -> Vec<crate::ownership_checking::UnitReceiverCallReturnEndFact> {
        self.records
            .iter()
            .filter_map(|record| match record {
                Record::ReceiverEnd(end) => Some(end.clone()),
                _ => None,
            })
            .collect()
    }

    pub(super) fn commit_borrow_handoff(&mut self, fact: UnitBorrowCallHandoffFact) {
        // Even observer-off retains the source used by the old public projection.
        self.records.push(Record::BorrowHandoffCommitted(fact));
    }

    pub(super) fn end_borrow_binding(&mut self, fact: BindingEnd) {
        // Each actual retirement has its own record. Only the old projection deduplicates.
        self.records.push(Record::BorrowBindingEnd(fact));
    }

    pub(super) fn borrow_handoffs(&self) -> Vec<UnitBorrowCallHandoffFact> {
        self.records
            .iter()
            .filter_map(|record| match record {
                Record::BorrowHandoffCommitted(fact) => Some(fact.clone()),
                _ => None,
            })
            .collect()
    }

    pub(super) fn borrow_binding_ends(&self) -> Vec<BindingEnd> {
        let mut ends = Vec::new();
        for record in &self.records {
            if let Record::BorrowBindingEnd(fact) = record
                && !ends.contains(fact)
            {
                ends.push(*fact);
            }
        }
        ends
    }

    pub(super) fn len(&self) -> usize {
        self.cleanups
    }

    pub(super) fn iter(
        &self,
    ) -> impl Iterator<Item = &(PlannerDropPoint, UnitIterationCleanupAction)> {
        self.records.iter().filter_map(|record| match record {
            Record::Cleanup(cleanup) => Some(cleanup),
            Record::Begin(_)
            | Record::ReceiverBegin(_)
            | Record::ReceiverEnd(_)
            | Record::BorrowHandoffCommitted(_)
            | Record::BorrowBindingEnd(_) => None,
        })
    }

    pub(super) fn finish(
        self,
        source: crate::name_resolution::SourceUnitId,
    ) -> Vec<crate::ownership_checking::UnitLoanActionObservation> {
        use crate::ownership_checking::UnitLoanAction;
        if !self.enabled {
            return Vec::new();
        }
        let identity = std::sync::Arc::new(());
        let actions: Vec<_> = self
            .records
            .into_iter()
            .map(|record| match record {
                Record::Begin(loan) => UnitLoanAction::Begin(loan),
                Record::ReceiverBegin(receiver) => UnitLoanAction::ReceiverBegin(receiver),
                Record::ReceiverEnd(end) => UnitLoanAction::ReceiverEnd(end),
                Record::BorrowHandoffCommitted(fact) => {
                    UnitLoanAction::BorrowHandoffCommitted(fact)
                }
                Record::BorrowBindingEnd(fact) => UnitLoanAction::BorrowBindingEnd(fact),
                Record::Cleanup((point, action)) => UnitLoanAction::Cleanup {
                    point: point.into_unit(source),
                    action,
                },
            })
            .collect();
        let regions = self
            .regions
            .into_iter()
            .map(|region| region.finish(identity.clone(), &actions))
            .collect();
        vec![crate::ownership_checking::UnitLoanActionObservation {
            identity,
            source,
            actions,
            regions,
        }]
    }
}

#[cfg(test)]
mod tests;
