//! SPEC-0197 compilation-unit source callable mapping、选择与 typed descriptor。

use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{DeclarationId, Namespace, SourceUnitId, UnitReferenceTarget},
    parser::CallArgument,
    type_checking::{
        BuiltinType, TypeCheckingError, UnitCallArgumentDescriptor, UnitCallDescriptor,
        UnitCallTarget, UnitCallableInstanceKey, UnitCallableSignature, UnitExpressionId,
        UnitTypeId,
        argument_mapping::{MappedParameter, MappingError, map_arguments},
    },
};

use super::{BodyChecker, CompilationUnitTypeError, ExpressionCheck};

impl BodyChecker<'_> {
    pub(super) fn check_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let callee_span = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?
            .span();
        let call_span = self
            .file(source)
            .ast()
            .expressions()
            .get(expression)
            .map_err(TypeCheckingError::from)?
            .span();
        let target = self
            .reference(source, callee_span, Namespace::Value)
            .cloned();
        let declaration_ids = match target {
            Some(UnitReferenceTarget::Declaration(declaration)) => vec![declaration],
            Some(UnitReferenceTarget::OverloadSet(declarations)) => declarations,
            _ => return Err(CompilationUnitTypeError::UnsupportedBody(callee_span)),
        };
        let mut mapped = Vec::new();
        let mut first_mapping_error = None;
        for declaration in declaration_ids {
            let Some(callable) = self
                .signatures
                .declaration(declaration)
                .and_then(|signature| signature.callable())
                .cloned()
            else {
                continue;
            };
            if !callable.type_parameters().is_empty() {
                return Err(CompilationUnitTypeError::UnsupportedBody(call_span));
            }
            let parameters = callable
                .parameters()
                .iter()
                .map(|parameter| MappedParameter {
                    name: parameter.name().map(str::to_owned),
                    mode: parameter.mode(),
                    ty: parameter.ty(),
                    span: Some(parameter.span()),
                })
                .collect::<Vec<_>>();
            match map_arguments(
                self.sources,
                &parameters,
                arguments,
                call_span,
                |argument| Ok(self.is_syntactic_place(source, argument)),
            )? {
                Ok(mapping) => mapped.push((declaration, callable, mapping)),
                Err(error) => {
                    first_mapping_error.get_or_insert(error);
                }
            }
        }
        if mapped.is_empty() {
            if let Some(error) = first_mapping_error {
                self.emit_mapping_error(error)?;
            } else {
                self.emit(
                    codes::NON_CALLABLE_TARGET,
                    "call target does not have a callable type",
                    callee_span,
                )?;
            }
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        if mapped.len() == 1 {
            let (declaration, callable, mapping) = mapped
                .pop()
                .expect("one mapped compilation-unit callable candidate");
            return self.finish_candidate(
                source,
                expression,
                callee,
                arguments,
                return_type,
                declaration,
                &callable,
                &mapping,
                None,
                true,
            );
        }
        let lambda_arguments = arguments
            .iter()
            .map(|argument| self.is_lambda_syntax(source, argument.value))
            .collect::<Vec<_>>();
        if lambda_arguments.iter().any(|is_lambda| *is_lambda) {
            return self.finish_overload_lambda_call(
                source,
                expression,
                callee,
                arguments,
                return_type,
                mapped,
                &lambda_arguments,
            );
        }
        let mut argument_types = Vec::with_capacity(arguments.len());
        for argument in arguments {
            argument_types.push(
                self.check_expression(source, argument.value, None, None, return_type)?
                    .ty,
            );
        }
        let viable = mapped
            .iter()
            .enumerate()
            .filter_map(|(candidate, (_, callable, mapping))| {
                arguments
                    .iter()
                    .enumerate()
                    .all(|(argument_index, _)| {
                        self.assignable(
                            argument_types[argument_index],
                            callable.parameters()[mapping[argument_index]].ty(),
                        )
                    })
                    .then_some(candidate)
            })
            .collect::<Vec<_>>();
        if viable.len() != 1 {
            return self.overload_failure(callee_span, viable.is_empty());
        }
        let (declaration, callable, mapping) = mapped.swap_remove(viable[0]);
        self.record_call(
            source,
            expression,
            arguments,
            declaration,
            &callable,
            &mapping,
        );
        Ok(ExpressionCheck {
            ty: callable.return_type(),
            falls_through: !self.is_builtin(callable.return_type(), BuiltinType::Nothing),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_candidate(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        return_type: UnitTypeId,
        declaration: DeclarationId,
        callable: &UnitCallableSignature,
        mapping: &[usize],
        prechecked_argument_types: Option<&[Option<UnitTypeId>]>,
        diagnose_no_match: bool,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let initial_diagnostics = self.diagnostics.len();
        let mut argument_types = Vec::with_capacity(arguments.len());
        for (argument_index, argument) in arguments.iter().enumerate() {
            if let Some(ty) = prechecked_argument_types
                .and_then(|types| types.get(argument_index))
                .copied()
                .flatten()
            {
                argument_types.push(ty);
                continue;
            }
            let parameter = &callable.parameters()[mapping[argument_index]];
            let (expected, expected_span) = if self.is_lambda_syntax(source, argument.value) {
                (Some(parameter.ty()), Some(parameter.span()))
            } else {
                (None, None)
            };
            argument_types.push(
                self.check_expression(
                    source,
                    argument.value,
                    expected,
                    expected_span,
                    return_type,
                )?
                .ty,
            );
        }
        let viable = argument_types.iter().enumerate().all(|(index, &ty)| {
            !self.is_error(ty)
                && !self.is_deferred(ty)
                && self.assignable(ty, callable.parameters()[mapping[index]].ty())
        });
        if !viable {
            if diagnose_no_match && self.diagnostics.len() == initial_diagnostics {
                let callee_span = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(callee)
                    .map_err(TypeCheckingError::from)?
                    .span();
                return self.overload_failure(callee_span, true);
            }
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        self.record_call(
            source,
            expression,
            arguments,
            declaration,
            callable,
            mapping,
        );
        Ok(ExpressionCheck {
            ty: callable.return_type(),
            falls_through: !self.is_builtin(callable.return_type(), BuiltinType::Nothing),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_overload_lambda_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        return_type: UnitTypeId,
        mut candidates: Vec<(DeclarationId, UnitCallableSignature, Vec<usize>)>,
        lambda_arguments: &[bool],
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let mut argument_types = vec![None; arguments.len()];
        let mut poisoned = false;
        for (index, argument) in arguments.iter().enumerate() {
            if lambda_arguments[index] {
                continue;
            }
            let result = self.check_expression(source, argument.value, None, None, return_type)?;
            poisoned |= self.is_error(result.ty) || self.is_deferred(result.ty);
            argument_types[index] = Some(result.ty);
        }
        if poisoned {
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        candidates.retain(|(_, callable, mapping)| {
            argument_types.iter().enumerate().all(|(index, actual)| {
                actual.is_none_or(|actual| {
                    self.assignable(actual, callable.parameters()[mapping[index]].ty())
                })
            })
        });
        let callee_span = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?
            .span();
        match candidates.len() {
            0 => return self.overload_failure(callee_span, true),
            1 => {
                let (declaration, callable, mapping) = candidates
                    .pop()
                    .expect("one candidate remains after non-lambda filtering");
                return self.finish_candidate(
                    source,
                    expression,
                    callee,
                    arguments,
                    return_type,
                    declaration,
                    &callable,
                    &mapping,
                    Some(&argument_types),
                    true,
                );
            }
            _ => {}
        }

        let baseline = self.trial_state();
        let baseline_diagnostics = self.diagnostics.len();
        let mut successes = Vec::new();
        for (declaration, callable, mapping) in candidates {
            self.restore_trial_state(baseline.clone());
            let declaration_span = callable.name_span();
            let result = self.finish_candidate(
                source,
                expression,
                callee,
                arguments,
                return_type,
                declaration,
                &callable,
                &mapping,
                Some(&argument_types),
                false,
            );
            let result = match result {
                Ok(result) => result,
                Err(error) => {
                    self.restore_trial_state(baseline);
                    return Err(error);
                }
            };
            if self.diagnostics.len() == baseline_diagnostics
                && !self.is_error(result.ty)
                && !self.is_deferred(result.ty)
            {
                successes.push((self.trial_state(), result, declaration_span));
            }
        }
        self.restore_trial_state(baseline);
        match successes.len() {
            0 => self.overload_failure(callee_span, true),
            1 => {
                let (state, result, _) = successes
                    .pop()
                    .expect("one successful compilation-unit candidate trial");
                self.restore_trial_state(state);
                Ok(result)
            }
            _ => {
                let declarations = successes
                    .iter()
                    .map(|(_, _, span)| *span)
                    .take(2)
                    .collect::<Vec<_>>();
                self.ambiguous_overload(callee_span, &declarations)
            }
        }
    }

    fn record_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        arguments: &[CallArgument],
        declaration: DeclarationId,
        callable: &UnitCallableSignature,
        mapping: &[usize],
    ) {
        let descriptors = mapping
            .iter()
            .enumerate()
            .map(|(argument_index, &parameter_index)| {
                let parameter = &callable.parameters()[parameter_index];
                UnitCallArgumentDescriptor {
                    argument_index,
                    parameter_index,
                    category: self.expression_category(source, arguments[argument_index].value),
                    mode: parameter.mode(),
                    parameter_type: parameter.ty(),
                    cross_thread: false,
                }
            })
            .collect();
        self.parts.calls.push(UnitCallDescriptor {
            expression: UnitExpressionId::new(source, expression),
            instance: UnitCallableInstanceKey {
                target: UnitCallTarget::Declaration(declaration),
                type_arguments: Vec::new(),
            },
            return_type: callable.return_type(),
            arguments: descriptors,
            aborts: false,
            prints_line: false,
        });
    }

    fn overload_failure(
        &mut self,
        callee_span: crate::source::Span,
        no_match: bool,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        self.emit(
            if no_match {
                codes::NO_MATCHING_OVERLOAD
            } else {
                codes::AMBIGUOUS_CALL
            },
            if no_match {
                "no overload matches the call arguments"
            } else {
                "call is ambiguous between multiple overloads"
            },
            callee_span,
        )?;
        Ok(ExpressionCheck {
            ty: self.error_type(),
            falls_through: true,
        })
    }

    fn ambiguous_overload(
        &mut self,
        callee_span: crate::source::Span,
        declarations: &[crate::source::Span],
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(codes::AMBIGUOUS_CALL)?;
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            "call remains ambiguous after argument type checking",
            callee_span,
        )?;
        for &span in declarations {
            diagnostic.add_label(self.sources, span, "matching callable declared here")?;
        }
        self.diagnostics.push(diagnostic);
        Ok(ExpressionCheck {
            ty: self.error_type(),
            falls_through: true,
        })
    }

    fn emit_mapping_error(&mut self, error: MappingError) -> Result<(), CompilationUnitTypeError> {
        match error {
            MappingError::Named(primary) => self.emit(
                codes::INVALID_NAMED_ARGUMENT,
                "named argument does not map uniquely to a callable parameter",
                primary,
            ),
            MappingError::Arity(primary) => self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "call must fill every parameter exactly once",
                primary,
            ),
            MappingError::Mode { primary, parameter } => self.emit_maybe_label(
                codes::CALL_ARGUMENT_MODE,
                "argument marker does not match the parameter contract",
                primary,
                parameter,
                "parameter contract declared here",
            ),
        }
    }
}
