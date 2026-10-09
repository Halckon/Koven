//! Explicit CFG slots for the loans retained by a descriptor binding.
use super::*;

pub(super) fn keys(sources: &BTreeMap<UnitSymbolId, Vec<LoanId>>) -> Vec<(UnitSymbolId, usize)> {
    sources
        .iter()
        .flat_map(|(symbol, loans)| (0..loans.len()).map(|index| (*symbol, index)))
        .collect()
}
pub(super) fn arguments(
    sources: &BTreeMap<UnitSymbolId, Vec<LoanId>>,
) -> impl Iterator<Item = EntityId> + '_ {
    sources.values().flatten().copied().map(EntityId::Loan)
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
        for (symbol, index) in keys(&self.result_source_loans) {
            let source = EntityId::Loan(self.result_source_loans[&symbol][index]);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                slot.range_sources.push((symbol, index));
            } else {
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
                    .ty;
                carried.push(CarriedAccess {
                    range_sources: vec![(symbol, index)],
                    captures: Vec::new(),
                    symbol: None,
                    receiver: None,
                    source,
                    ty,
                    pending: Vec::new(),
                });
            }
        }
        Ok(())
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
            for (symbol, index) in &slot.range_sources {
                let EntityId::Loan(loan) = parameter else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                let loans = rebound.entry(*symbol).or_default();
                if loans.len() <= *index {
                    loans.resize(*index + 1, *loan);
                }
                loans[*index] = *loan;
            }
        }
        self.result_source_loans = rebound;
        Ok(())
    }
}
