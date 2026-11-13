//! Match the completed terminal false assignment to actual emitted operations.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn multi_root_guard_reset(
        &self,
        body: BlockRef,
        guard: Slot,
        reset: UnitExpressionId,
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let descriptor = self.inputs.typed.assignment(reset).ok_or_else(fail)?;
        let statements = &self.body.blocks[body.0].statements;
        let mut writes = statements
            .iter()
            .enumerate()
            .filter(|(_, s)| s.results.contains(&guard));
        let (index, write) = writes.next().ok_or_else(fail)?;
        if writes.next().is_some()
            || write.results != [guard]
            || write.span != self.span(reset.expression())
            || index + 1 != statements.len()
        {
            return Err(fail());
        }
        let ROp::Alias { source } = write.op else {
            return Err(fail());
        };
        let constants: Vec<_> = statements[..index]
            .iter()
            .filter(|s| {
                s.results == [source]
                    && s.span == self.span(descriptor.value().expression())
                    && matches!(
                        s.op,
                        ROp::Constant(crate::ssa::model::ScalarConstant::Boolean(false))
                    )
            })
            .collect();
        if constants.len() != 1 || self.multi_root_definitions(source) != 1 {
            return Err(fail());
        }
        Ok(())
    }
}
