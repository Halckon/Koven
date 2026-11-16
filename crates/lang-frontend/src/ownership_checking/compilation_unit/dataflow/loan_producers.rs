//! Actual append provenance, not a new loan or a callable lifecycle certificate.
use super::{Checker, UnitLoanFact};
impl Checker<'_> {
    pub(super) fn push_loan(&mut self, loan: UnitLoanFact) {
        #[cfg(feature = "d10-catalog-experiment")]
        self.loan_producers.append();
        self.loans.push(loan);
    }
}
#[cfg(feature = "d10-catalog-experiment")]
use super::super::d10_catalog_experiment::LoanProducer;
#[cfg(feature = "d10-catalog-experiment")]
#[derive(Default)]
pub(super) struct Collection {
    // The shared loan Vec spans sources; this collection covers only the current source.
    base: usize,
    valid: bool,
    current: Option<LoanProducer>,
    producers: Vec<Option<LoanProducer>>,
}
#[cfg(feature = "d10-catalog-experiment")]
impl Collection {
    pub(super) fn new(base: usize) -> Self {
        Self {
            base,
            valid: true,
            ..Self::default()
        }
    }
    pub(super) fn enter_item(
        &mut self,
        source: crate::name_resolution::SourceUnitId,
        item: crate::ast::ItemId,
        deinit: bool,
    ) -> Option<LoanProducer> {
        let item = crate::type_checking::UnitItemId::new(source, item);
        self.replace(if deinit {
            LoanProducer::Deinit(item)
        } else {
            LoanProducer::Function(item)
        })
    }
    pub(super) fn enter_lambda(
        &mut self,
        lambda: crate::type_checking::UnitExpressionId,
    ) -> Option<LoanProducer> {
        self.replace(LoanProducer::Lambda(lambda))
    }
    fn replace(&mut self, producer: LoanProducer) -> Option<LoanProducer> {
        self.current.replace(producer)
    }
    pub(super) fn restore(&mut self, previous: Option<LoanProducer>) {
        self.current = previous;
    }
    fn append(&mut self) {
        self.producers.push(self.current);
    }
    pub(super) fn rollback(&mut self, global_len: usize) {
        // All checkpoints belong to this checker, and never precede the source base.
        if let Some(local_len) = global_len.checked_sub(self.base) {
            self.producers.truncate(local_len);
            debug_assert_eq!(self.producers.len(), local_len);
        } else {
            // Impossible for a local checkpoint. Preserve an explicit mismatch, never publish.
            self.valid = false;
        }
    }
    pub(super) fn finish(self) -> Option<Vec<Option<LoanProducer>>> {
        self.valid.then_some(self.producers)
    }
}
