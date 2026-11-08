//! SPEC-0197 compilation-unit lambda expected contract 与 callable boundary。

use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Severity, codes},
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget, UnitSymbolId},
    parser::Expression,
    source::Span,
    type_checking::{DeferredReason, ParameterMode, UnitExpressionId, UnitTypeId, UnitTypeKind},
};

use super::call_writeback::PendingCallableReturns;
use super::{BodyChecker, CompilationUnitTypeError, ExpressionCheck, ExpressionExpected};

pub(super) enum PartialLambdaContext {
    Unresolved,
    WrongHeader(Span),
    NonCallable,
    Poisoned,
}

impl BodyChecker<'_> {
    pub(super) fn emit_lambda_structure_mismatch(
        &mut self,
        primary: Span,
        expected_span: Option<Span>,
    ) -> Result<(), CompilationUnitTypeError> {
        self.emit_maybe_label(
            codes::TYPE_MISMATCH,
            "lambda structure does not match the expected function type",
            primary,
            expected_span,
            "expected function type introduced here",
        )
    }

    pub(super) fn partial_lambda_context(
        &self,
        source: SourceUnitId,
        mut expression: ExpressionId,
        expected: ExpressionExpected<'_>,
    ) -> PartialLambdaContext {
        let mut inner = expected;
        while let Some(UnitTypeKind::Nullable(ty)) = inner.kind(self) {
            inner = inner.child(*ty);
        }
        let (move_only, parameters) = match inner.kind(self) {
            Some(UnitTypeKind::Function {
                move_only,
                parameters,
                ..
            }) if !matches!(expected.kind(self), Some(UnitTypeKind::Nullable(_))) => {
                (move_only, parameters)
            }
            Some(UnitTypeKind::Deferred(_) | UnitTypeKind::Function { .. }) | None => {
                return PartialLambdaContext::Unresolved;
            }
            Some(UnitTypeKind::Error) => return PartialLambdaContext::Poisoned,
            Some(_) => return PartialLambdaContext::NonCallable,
        };
        loop {
            let Ok(node) = self.file(source).ast().expressions().get(expression) else {
                return PartialLambdaContext::Unresolved;
            };
            match node.payload() {
                Expression::Group { expression: child } => expression = *child,
                Expression::Lambda {
                    move_span,
                    opener_span,
                    parameters: actual,
                    arrow_span,
                    ..
                } => {
                    let implicit = if arrow_span.is_none() {
                        self.symbol_at(source, *opener_span, Namespace::Value)
                    } else {
                        None
                    };
                    return if lambda_structure_matches(
                        move_span.is_some(),
                        arrow_span.is_some(),
                        actual.len(),
                        self.implicit_lambda_parameter_used(implicit),
                        *move_only,
                        parameters.len(),
                    ) {
                        PartialLambdaContext::Unresolved
                    } else {
                        PartialLambdaContext::WrongHeader(
                            arrow_span.or(*move_span).unwrap_or(*opener_span),
                        )
                    };
                }
                _ => return PartialLambdaContext::Unresolved,
            }
        }
    }

    fn implicit_lambda_parameter_used(&self, symbol: Option<UnitSymbolId>) -> bool {
        symbol.is_some_and(|symbol| {
            self.references.values().any(
                |target| matches!(target, UnitReferenceTarget::Symbol(candidate) if *candidate == symbol),
            )
        })
    }

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
        expression: UnitExpressionId,
        span: Span,
        move_span: Option<Span>,
        opener_span: Span,
        parameter_spans: &[Span],
        arrow_span: Option<Span>,
        body: StatementId,
        expected: Option<ExpressionExpected<'_>>,
        expected_span: Option<Span>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let source = expression.source_unit();
        let writeback_origin = move_span.is_none().then_some(expression);
        let implicit_symbol = arrow_span
            .is_none()
            .then(|| self.symbol_at(source, opener_span, Namespace::Value))
            .flatten();
        let implicit_used = self.implicit_lambda_parameter_used(implicit_symbol);
        let expected_function = expected.and_then(|expectation| {
            let UnitTypeKind::Function {
                move_only,
                parameters,
                return_type,
            } = expectation.kind(self)?
            else {
                return None;
            };
            Some((expectation, *move_only, parameters.len(), *return_type))
        });
        if let Some((expectation, move_only, arity, return_type)) = expected_function {
            let structure_matches = lambda_structure_matches(
                move_span.is_some(),
                arrow_span.is_some(),
                parameter_spans.len(),
                implicit_used,
                move_only,
                arity,
            );
            if !structure_matches {
                self.emit_lambda_structure_mismatch(
                    arrow_span.or(move_span).unwrap_or(opener_span),
                    expected_span,
                )?;
                let error = self.error_type();
                if implicit_symbol.is_some() {
                    self.set_span_symbol(source, opener_span, error);
                }
                for &parameter_span in parameter_spans {
                    self.set_span_symbol(source, parameter_span, error);
                }
                // No parameter gets a contextual type when the shape is invalid.
                let return_type = expectation.child(return_type).resolve(self)?;
                self.check_lambda_body(source, body, return_type, expected_span, None, &[], None)?;
                return Ok(ExpressionCheck {
                    ty: error,
                    falls_through: true,
                });
            }
            let function = expectation.resolve(self)?;
            let Some(UnitTypeKind::Function {
                parameters,
                return_type,
                ..
            }) = self.signatures.types().get(function).cloned()
            else {
                return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
            };
            let effective_spans = if arrow_span.is_none() && arity == 1 {
                vec![opener_span]
            } else {
                parameter_spans.to_vec()
            };
            let mut inout_parameters = Vec::new();
            for (&parameter_span, parameter) in effective_spans.iter().zip(&parameters) {
                self.set_span_symbol(source, parameter_span, parameter.ty());
                self.set_parameter_mode(source, parameter_span, parameter.mode());
                if parameter.mode() == ParameterMode::Inout
                    && let Some(symbol) = self.symbol_at(source, parameter_span, Namespace::Value)
                {
                    inout_parameters.push((symbol, parameter.ty()));
                }
            }
            self.check_lambda_body(
                source,
                body,
                return_type,
                expected_span,
                Some(return_type),
                &inout_parameters,
                writeback_origin,
            )?;
            return Ok(ExpressionCheck {
                ty: function,
                // A lambda's local return cannot terminate the enclosing expression path.
                falls_through: true,
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
            self.check_lambda_body(source, body, error, None, None, &[], None)?;
            return Ok(ExpressionCheck {
                ty: error,
                falls_through: true,
            });
        }
        if parameter_spans.is_empty() {
            let return_type = self.deferred_type(DeferredReason::ControlJoin);
            let body = self.check_lambda_body(
                source,
                body,
                return_type,
                None,
                None,
                &[],
                writeback_origin,
            )?;
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
                falls_through: true,
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
        self.check_lambda_body(source, body, error, None, None, &[], None)?;
        Ok(ExpressionCheck {
            ty: error,
            falls_through: true,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn check_lambda_body(
        &mut self,
        source: SourceUnitId,
        body: StatementId,
        return_type: UnitTypeId,
        return_span: Option<Span>,
        expected: Option<UnitTypeId>,
        inout_parameters: &[(UnitSymbolId, UnitTypeId)],
        writeback_origin: Option<UnitExpressionId>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let previous_return_span = self.current_return_span;
        // Creating a lambda does not execute its body. Preserve inbound facts for checking,
        // then restore dynamic flow on both success and error without discarding typed parts.
        let previous_flow = self.flow_facts.clone();
        let previous_versions = self.flow_versions.clone();
        let previous_callable_returns = self.pending_callable_returns.take();
        let previous_body_reachability = std::mem::replace(&mut self.body_path_reachable, true);
        if writeback_origin.is_some() {
            self.pending_callable_returns = Some(PendingCallableReturns::for_closure());
        }
        self.track_inout_parameters(inout_parameters.iter().copied());
        self.current_return_span = return_span;
        self.callable_loop_bases.push(self.loop_depth);
        let body_diagnostics = self.diagnostics.len();
        let result = self.check_value_body(source, body, expected, return_span, return_type);
        if let (Some(origin), Ok(checked)) = (writeback_origin, &result)
            && !self.is_error(checked.ty)
        {
            if self.has_pending_statement_exit(source, body, *checked) {
                self.record_pending_callable_return();
            }
            if let Some(returns) = self.pending_callable_returns.take() {
                let trusted_exit = !self.diagnostics[body_diagnostics..]
                    .iter()
                    .any(|diagnostic| diagnostic.severity() == Severity::Error);
                self.finish_closure_effects(origin, returns, &previous_versions, trusted_exit);
            }
        }
        self.callable_loop_bases.pop();
        self.current_return_span = previous_return_span;
        self.flow_facts = previous_flow;
        self.flow_versions = previous_versions;
        self.pending_callable_returns = previous_callable_returns;
        self.body_path_reachable = previous_body_reachability;
        result
    }
}

fn lambda_structure_matches(
    has_move: bool,
    has_arrow: bool,
    parameter_count: usize,
    implicit_used: bool,
    move_only: bool,
    arity: usize,
) -> bool {
    // The expected move constraint restricts captures; ordinary Function accepts either literal.
    (!move_only || has_move)
        && if has_arrow {
            arity == parameter_count
        } else {
            arity == 1 || (arity == 0 && !implicit_used)
        }
}
