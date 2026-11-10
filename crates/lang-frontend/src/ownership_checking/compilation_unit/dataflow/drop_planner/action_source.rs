//! The cleanup projections and acquisition observation share one append-only source.
use super::PlannerDropPoint;
use crate::ownership_checking::{UnitIterationCleanupAction, UnitLoanFact};

pub(super) struct ActionSource {
    records: Vec<Record>,
    cleanups: usize,
    regions: Vec<crate::ownership_checking::compilation_unit::loan_region_validation::Candidate>,
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
            regions: Vec::new(),
            enabled: true,
        }
    }
}

impl ActionSource {
    pub(super) fn enter_region(
        &mut self,
        call: crate::type_checking::UnitExpressionId,
        callable: Option<crate::type_checking::UnitItemId>,
        expected: Option<
            Vec<crate::ownership_checking::compilation_unit::loan_region_validation::Expected>,
        >,
    ) -> usize {
        let index = self.regions.len();
        self.regions.push(
            crate::ownership_checking::compilation_unit::loan_region_validation::Candidate {
                call,
                callable,
                expected,
                start: self.records.len(),
                end: None,
            },
        );
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
        let identity = std::sync::Arc::new(());
        let actions: Vec<_> = self
            .records
            .into_iter()
            .map(|record| match record {
                Record::Begin(loan) => UnitLoanAction::Begin(loan),
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
