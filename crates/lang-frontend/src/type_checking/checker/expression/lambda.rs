//! Lambda expected-type checking with creation-site flow restoration.
use super::*;

impl Checker<'_> {
    pub(super) fn check_lambda(
        &mut self,
        syntax: LambdaSyntax,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        // Checking a closure body does not execute it in the creation-site flow.
        let outer_facts = self.flow_facts.clone();
        let outer_versions = self.flow_versions.clone();
        let outer_pending = self.iteration_recovery.bindings.clone();
        let result = self.check_lambda_inner(syntax, expected, expected_span);
        self.flow_facts = outer_facts;
        self.flow_versions = outer_versions;
        // Closure creation cannot clear a pending binding in its enclosing flow.
        self.iteration_recovery.bindings.extend(outer_pending);
        result
    }

    fn check_lambda_inner(
        &mut self,
        syntax: LambdaSyntax,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let LambdaSyntax {
            span,
            move_span,
            opener_span,
            parameter_spans,
            arrow_span,
            body,
        } = syntax;
        let implicit_symbol = arrow_span
            .is_none()
            .then(|| self.symbol_at(opener_span))
            .flatten();
        let implicit_used = implicit_symbol.is_some_and(|symbol| {
            self.source_references.iter().any(|(_, target)| {
                matches!(target, ReferenceTarget::Symbol(candidate) if *candidate == symbol)
            })
        });
        let expected_function = expected.and_then(|ty| match self.kind(ty).clone() {
            TypeKind::Function {
                move_only,
                parameters,
                return_type,
            } => Some((ty, move_only, parameters, return_type)),
            _ => None,
        });
        if let Some((function, move_only, parameters, return_type)) = expected_function {
            let implicit_arity_matches = match parameters.len() {
                0 => !implicit_used,
                1 => true,
                _ => false,
            };
            // The expected move constraint restricts captures; ordinary Function accepts either literal.
            let structure_matches = (!move_only || move_span.is_some())
                && if arrow_span.is_none() {
                    implicit_arity_matches
                } else {
                    parameters.len() == parameter_spans.len()
                };
            if !structure_matches {
                self.emit_with_label(
                    self.mismatch_code,
                    "lambda structure does not match the expected function type",
                    arrow_span.or(move_span).unwrap_or(opener_span),
                    expected_span.unwrap_or(span),
                    "expected function type introduced here",
                )?;
                let error = self.error_type();
                if let Some(symbol) = implicit_symbol {
                    self.set_symbol(symbol, error);
                }
                for &parameter_span in &parameter_spans {
                    if let Some(symbol) = self.symbol_at(parameter_span) {
                        self.set_symbol(symbol, error);
                    }
                }
                self.check_value_body(body, None, None)?;
                return Ok(ExprCheck {
                    ty: error,
                    falls_through: true,
                });
            }
            let effective_spans = if arrow_span.is_none() && parameters.len() == 1 {
                vec![opener_span]
            } else {
                parameter_spans.clone()
            };
            for (&parameter_span, parameter) in effective_spans.iter().zip(&parameters) {
                if let Some(symbol) = self.symbol_at(parameter_span) {
                    self.set_symbol(symbol, parameter.ty);
                    self.set_parameter_mode(symbol, parameter.mode);
                }
            }
            self.callables.push(CallableContext {
                return_type,
                annotation_span: expected_span,
                loop_base: self.loop_depth,
            });
            self.check_value_body(body, Some(return_type), expected_span)?;
            self.callables.pop();
            return Ok(ExprCheck {
                ty: function,
                falls_through: true, // Creating the callable does not execute its local return.
            });
        }
        if arrow_span.is_none() && implicit_used {
            self.emit(
                self.cannot_infer_code,
                "implicit it requires a unary expected function type",
                opener_span,
            )?;
            let error = self.error_type();
            if let Some(symbol) = implicit_symbol {
                self.set_symbol(symbol, error);
            }
            self.callables.push(CallableContext {
                return_type: error,
                annotation_span: None,
                loop_base: self.loop_depth,
            });
            self.check_value_body(body, None, None)?;
            self.callables.pop();
            return Ok(ExprCheck {
                ty: error,
                falls_through: true,
            });
        }
        if parameter_spans.is_empty() {
            let return_type = self.deferred(DeferredReason::ControlJoin);
            self.callables.push(CallableContext {
                return_type,
                annotation_span: None,
                loop_base: self.loop_depth,
            });
            let body = self.check_value_body(body, None, None)?;
            self.callables.pop();
            let ty = if self.is_error(body.ty) || self.is_deferred(body.ty) {
                body.ty
            } else {
                self.types.intern(TypeKind::Function {
                    move_only: move_span.is_some(),
                    parameters: Vec::new(),
                    return_type: body.ty,
                })
            };
            return Ok(ExprCheck {
                ty,
                falls_through: true,
            });
        }
        self.emit(
            self.cannot_infer_code,
            "lambda parameter types require an expected function type",
            arrow_span.unwrap_or(span),
        )?;
        let error = self.error_type();
        for parameter_span in parameter_spans {
            if let Some(symbol) = self.symbol_at(parameter_span) {
                self.set_symbol(symbol, error);
            }
        }
        self.callables.push(CallableContext {
            return_type: error,
            annotation_span: None,
            loop_base: self.loop_depth,
        });
        self.check_value_body(body, None, None)?;
        self.callables.pop();
        Ok(ExprCheck {
            ty: error,
            falls_through: true,
        })
    }
}
