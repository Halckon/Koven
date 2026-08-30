//! SPEC-0197 compilation-unit lambda expected contract 与 callable boundary。

use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::codes,
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget},
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
        opener_span: Span,
        parameter_spans: &[Span],
        arrow_span: Option<Span>,
        body: StatementId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let implicit_symbol = arrow_span
            .is_none()
            .then(|| self.symbol_at(source, opener_span, Namespace::Value))
            .flatten();
        let implicit_used = implicit_symbol.is_some_and(|symbol| {
            self.references.values().any(
                |target| matches!(target, UnitReferenceTarget::Symbol(candidate) if *candidate == symbol),
            )
        });
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
            let implicit_arity_matches = match parameters.len() {
                0 => !implicit_used,
                1 => true,
                _ => false,
            };
            let structure_matches = move_only == move_span.is_some()
                && if arrow_span.is_none() {
                    implicit_arity_matches
                } else {
                    parameters.len() == parameter_spans.len()
                };
            if !structure_matches {
                self.emit_maybe_label(
                    codes::TYPE_MISMATCH,
                    "lambda structure does not match the expected function type",
                    arrow_span.or(move_span).unwrap_or(opener_span),
                    expected_span,
                    "expected function type introduced here",
                )?;
                let error = self.error_type();
                if implicit_symbol.is_some() {
                    self.set_span_symbol(source, opener_span, error);
                }
                for &parameter_span in parameter_spans {
                    self.set_span_symbol(source, parameter_span, error);
                }
                self.check_lambda_body(source, body, return_type, expected_span, None)?;
                return Ok(ExpressionCheck {
                    ty: error,
                    falls_through: true,
                });
            }
            let effective_spans = if arrow_span.is_none() && parameters.len() == 1 {
                vec![opener_span]
            } else {
                parameter_spans.to_vec()
            };
            for (&parameter_span, parameter) in effective_spans.iter().zip(&parameters) {
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
        if arrow_span.is_none() && implicit_used {
            self.emit(
                codes::CANNOT_INFER_TYPE,
                "implicit it requires a unary expected function type",
                opener_span,
            )?;
            let error = self.error_type();
            self.set_span_symbol(source, opener_span, error);
            self.check_lambda_body(source, body, error, None, None)?;
            return Ok(ExpressionCheck {
                ty: error,
                falls_through: true,
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
