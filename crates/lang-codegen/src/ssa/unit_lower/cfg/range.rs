//! Explicit CFG slots for the loans retained by a descriptor binding.
use super::*;
use crate::ssa::carry::ResultSourceLoan;

pub(super) fn keys(sources: &BTreeMap<UnitSymbolId, Vec<LoanId>>) -> Vec<(UnitSymbolId, usize)> {
    sources
        .iter()
        .flat_map(|(symbol, loans)| (0..loans.len()).map(|index| (*symbol, index)))
        .collect()
}
pub(super) fn rebind(
    sources: &BTreeMap<UnitSymbolId, Vec<LoanId>>,
    parameters: impl Iterator<Item = EntityId>,
    span: Span,
) -> Result<BTreeMap<UnitSymbolId, Vec<LoanId>>, LoweringError> {
    let mut rebound: BTreeMap<_, Vec<_>> =
        sources.keys().map(|symbol| (*symbol, Vec::new())).collect();
    let mut parameters = parameters;
    for (symbol, _) in keys(sources) {
        let Some(EntityId::Loan(loan)) = parameters.next() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        rebound.entry(symbol).or_default().push(loan);
    }
    if parameters.next().is_some() {
        return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
    }
    Ok(rebound)
}
impl UnitExpressionLowerer<'_> {
    pub(super) fn carry_range_sources(
        &self,
        carried: &mut Vec<CarriedAccess>,
        span: Span,
    ) -> Result<(), LoweringError> {
        carry::collect_aliases::<ResultSourceLoan, _, _>(
            self.function,
            carry::indexed_aliases(&self.result_source_loans),
            carried,
            lowering_error(LoweringErrorKind::MissingFact, span),
        )
    }
    pub(super) fn rebind_range_sources(
        &mut self,
        carried: &[CarriedAccess],
        parameters: &[EntityId],
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut rebound: BTreeMap<_, Vec<_>> = self
            .result_source_loans
            .keys()
            .map(|symbol| (*symbol, Vec::new()))
            .collect();
        for (slot, parameter) in carried.iter().zip(parameters) {
            if !slot.range_sources.is_empty() {
                let EntityId::Loan(loan) = parameter else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                carry::apply_indexed_alias_column(&slot.range_sources, *loan, &mut rebound);
            }
        }
        self.result_source_loans = rebound;
        Ok(())
    }
}
