//! Observe the actual existing Return producer without creating another terminator.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn owner_return(
        &mut self,
        expression: ExpressionId,
        result: Option<Slot>,
        unit_return: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.finish_return(result, unit_return, span)?;
        #[cfg(test)]
        super::super::record_owner_return(self.unit(expression), result.map(|s| s.0), None);
        #[cfg(not(test))]
        let _ = expression;
        Ok(())
    }
}
