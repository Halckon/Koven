//! Ordered operand checking retains suffix diagnostics without publishing unreachable exits.
use super::{
    BodyChecker, CompilationUnitTypeError, ExpressionCheck, ExpressionId, SourceUnitId, Span,
    UnitTypeId,
};

pub(super) struct CallOperandFlow {
    ordinary: bool,
    pending: bool,
    track_pending: bool,
}

impl CallOperandFlow {
    pub(super) fn new(checker: &BodyChecker<'_>) -> Self {
        Self {
            ordinary: true,
            pending: true,
            track_pending: checker.tracks_pending_call_flow(),
        }
    }

    pub(super) fn falls_through(&self) -> bool {
        self.ordinary
    }

    pub(super) fn check(
        &mut self,
        checker: &mut BodyChecker<'_>,
        source: SourceUnitId,
        expression: ExpressionId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let result = checker.check_loop_paths(self.ordinary, self.pending, |checker| {
            checker.check_expression(source, expression, expected, expected_span, return_type)
        })?;
        self.ordinary &= result.falls_through;
        if self.track_pending && self.pending {
            self.pending = checker.has_pending_expression_exit(source, expression, result)?;
        }
        Ok(result)
    }
}
