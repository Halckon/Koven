//! Shared null-comparison lookup for remaining `If` proof entries.
use super::*;
use lang_frontend::type_checking::UnitNullComparisonDescriptor;

impl UnitExpressionLowerer<'_> {
    pub(super) fn map_borrow_condition(
        &self,
        mut condition: ExpressionId,
        span: Span,
    ) -> Result<Option<UnitNullComparisonDescriptor>, LoweringError> {
        while let Expression::Group { expression } = self
            .parsed
            .ast()
            .expressions()
            .get(condition)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .payload()
        {
            condition = *expression;
        }
        Ok(self
            .typed
            .null_comparison(UnitExpressionId::new(self.source_unit, condition)))
    }
}
