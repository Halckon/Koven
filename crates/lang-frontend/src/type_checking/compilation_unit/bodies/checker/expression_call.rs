//! Keep call dispatch temporaries off recursively nested expression frames.
use super::{BodyChecker, ExpressionCheck};
use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::SourceUnitId,
    parser::CallArgument,
    source::Span,
    type_checking::{CompilationUnitTypeError, UnitTypeId},
};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_call_expression(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let invalidated = arguments
            .iter()
            .filter(|argument| {
                matches!(
                    argument.mode_marker,
                    Some(crate::parser::ParameterModeMarker::Inout(_))
                )
            })
            .filter_map(|argument| self.stable_flow_key(source, argument.value))
            .collect::<Vec<_>>();
        let result = if let Some(result) = self.check_intrinsic_ownership_primitive_call(
            source,
            expression,
            span,
            callee,
            type_arguments,
            arguments,
            return_type,
        )? {
            result
        } else if let Some(result) = self.check_intrinsic_container_call(
            source,
            expression,
            span,
            callee,
            type_arguments,
            arguments,
            expected,
            expected_span,
            return_type,
        )? {
            result
        } else if let Some(result) = self.check_source_construction_call(
            source,
            expression,
            span,
            callee,
            type_arguments,
            arguments,
            expected,
            expected_span,
            return_type,
        )? {
            result
        } else {
            self.check_call(
                source,
                expression,
                callee,
                type_arguments,
                arguments,
                return_type,
            )?
        };
        for key in invalidated {
            self.invalidate_flow_key(key);
        }
        Ok(result)
    }
}
