//! The cleanup projections and acquisition observation share one append-only source.
use super::PlannerDropPoint;
use crate::ownership_checking::{UnitIterationCleanupAction, UnitLoanFact};

pub(super) struct ActionSource {
    records: Vec<Record>,
    cleanups: usize,
    pub(super) enabled: bool,
}

enum Record {
    Begin(UnitLoanFact),
    Cleanup((PlannerDropPoint, UnitIterationCleanupAction)),
}

impl Default for ActionSource {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            cleanups: 0,
            enabled: true,
        }
    }
}

impl ActionSource {
    pub(super) fn push(&mut self, cleanup: (PlannerDropPoint, UnitIterationCleanupAction)) {
        self.records.push(Record::Cleanup(cleanup));
        self.cleanups += 1;
    }

    pub(super) fn begin(&mut self, loan: UnitLoanFact) {
        if self.enabled {
            self.records.push(Record::Begin(loan));
        }
    }

    pub(super) fn len(&self) -> usize {
        self.cleanups
    }

    pub(super) fn iter(
        &self,
    ) -> impl Iterator<Item = &(PlannerDropPoint, UnitIterationCleanupAction)> {
        self.records.iter().filter_map(|record| match record {
            Record::Cleanup(cleanup) => Some(cleanup),
            Record::Begin(_) => None,
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
        vec![crate::ownership_checking::UnitLoanActionObservation {
            identity: std::sync::Arc::new(()),
            source,
            actions: self
                .records
                .into_iter()
                .map(|record| match record {
                    Record::Begin(loan) => UnitLoanAction::Begin(loan),
                    Record::Cleanup((point, action)) => UnitLoanAction::Cleanup {
                        point: point.into_unit(source),
                        action,
                    },
                })
                .collect(),
        }]
    }
}
