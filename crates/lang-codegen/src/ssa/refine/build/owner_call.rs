//! Existing Unit call emission with observation of the actual emitted action.
use super::*;

impl Builder<'_, '_> {
    pub(super) fn owner_unit_call(
        &mut self,
        call: UnitExpressionId,
        op: ROp,
        span: Span,
    ) -> Result<(), LoweringError> {
        #[cfg(test)]
        let actual = match &op {
            ROp::DirectCall {
                callee, arguments, ..
            } => Some((*callee, arguments.clone())),
            _ => None,
        };
        self.push(op, Vec::new(), span)?;
        #[cfg(test)]
        if let Some((callee, arguments)) = actual {
            super::super::record_owner_call(call, callee, &arguments, None);
        }
        #[cfg(not(test))]
        let _ = call;
        Ok(())
    }
}
