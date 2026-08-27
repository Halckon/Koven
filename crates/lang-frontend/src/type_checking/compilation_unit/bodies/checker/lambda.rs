//! SPEC-0197 compilation-unit lambda expected contract 与 callable boundary。

use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::codes,
    name_resolution::SourceUnitId,
    source::Span,
    type_checking::{DeferredReason, UnitTypeId, UnitTypeKind},
};

use super::{BodyChecker, CompilationUnitTypeError, ExpressionCheck};

impl BodyChecker<'_> {
    pub(super) fn is_lambda_syntax(&self, source: SourceUnitId, expression: ExpressionId) -> bool {
        let Ok(node) = self.file(source).ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            crate::parser::Expression::Lambda { .. } => true,
            crate::parser::Expression::Group { expression } => {
                self.is_lambda_syntax(source, *expression)
            }
            _ => false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_lambda(
        &mut self,
        source: SourceUnitId,
        span: Span,
        move_span: Option<Span>,
        parameter_spans: &[Span],
        arrow_span: Option<Span>,
        body: StatementId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let expected_function = expected.and_then(|ty| {
            let UnitTypeKind::Function {
                move_only,
                parameters,
                return_type,
            } = self.signatures.types().get(ty)?.clone()
            else {
                return None;
            };
            Some((ty, move_only, parameters, return_type))
        });
        if let Some((function, move_only, parameters, return_type)) = expected_function {
            let structure_matches =
                move_only == move_span.is_some() && parameters.len() == parameter_spans.len();
            if !structure_matches {
                self.emit_maybe_label(
                    codes::TYPE_MISMATCH,
                    "lambda structure does not match the expected function type",
                    arrow_span.or(move_span).unwrap_or(span),
                    expected_span,
                    "expected function type introduced here",
                )?;
                let error = self.error_type();
                for &parameter_span in parameter_spans {
                    self.set_span_symbol(source, parameter_span, error);
                }
                self.check_lambda_body(source, body, return_type, expected_span, None)?;
                return Ok(ExpressionCheck {
                    ty: error,
                    falls_through: true,
                });
            }
            for (&parameter_span, parameter) in parameter_spans.iter().zip(&parameters) {
                self.set_span_symbol(source, parameter_span, parameter.ty());
                self.set_parameter_mode(source, parameter_span, parameter.mode());
            }
            let body = self.check_lambda_body(
                source,
                body,
                return_type,
                expected_span,
                Some(return_type),
            )?;
            return Ok(ExpressionCheck {
                ty: function,
                falls_through: body.falls_through,
            });
        }
        if parameter_spans.is_empty() {
            let return_type = self.deferred_type(DeferredReason::ControlJoin);
            let body = self.check_lambda_body(source, body, return_type, None, None)?;
            let ty = if self.is_error(body.ty) || self.is_deferred(body.ty) {
                body.ty
            } else {
                self.signatures.types_mut().intern(UnitTypeKind::Function {
                    move_only: move_span.is_some(),
                    parameters: Vec::new(),
                    return_type: body.ty,
                })
            };
            return Ok(ExpressionCheck {
                ty,
                falls_through: body.falls_through,
            });
        }
        self.emit(
            codes::CANNOT_INFER_TYPE,
            "lambda parameter types require an expected function type",
            arrow_span.unwrap_or(span),
        )?;
        let error = self.error_type();
        for &parameter_span in parameter_spans {
            self.set_span_symbol(source, parameter_span, error);
        }
        self.check_lambda_body(source, body, error, None, None)?;
        Ok(ExpressionCheck {
            ty: error,
            falls_through: true,
        })
    }

    fn check_lambda_body(
        &mut self,
        source: SourceUnitId,
        body: StatementId,
        return_type: UnitTypeId,
        return_span: Option<Span>,
        expected: Option<UnitTypeId>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let previous_return_span = self.current_return_span;
        self.current_return_span = return_span;
        self.callable_loop_bases.push(self.loop_depth);
        let result = self.check_value_body(source, body, expected, return_span, return_type);
        self.callable_loop_bases.pop();
        self.current_return_span = previous_return_span;
        result
    }
}
