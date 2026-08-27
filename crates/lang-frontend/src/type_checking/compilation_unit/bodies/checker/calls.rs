//! SPEC-0197 compilation-unit source callable mapping、选择与 typed descriptor。

use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget},
    parser::CallArgument,
    type_checking::{
        BuiltinType, TypeCheckingError, UnitCallArgumentDescriptor, UnitCallDescriptor,
        UnitCallTarget, UnitCallableInstanceKey, UnitExpressionId, UnitTypeId,
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
        if mapped.len() > 1
            && arguments
                .iter()
                .any(|argument| self.is_lambda_syntax(source, argument.value))
        {
            return Err(CompilationUnitTypeError::UnsupportedBody(call_span));
        }
        let unique_expected = (mapped.len() == 1).then(|| {
            let (_, callable, mapping) = &mapped[0];
            mapping
                .iter()
                .map(|&parameter| {
                    let parameter = &callable.parameters()[parameter];
                    (parameter.ty(), Some(parameter.span()))
                })
                .collect::<Vec<_>>()
        });
        let mut argument_types = Vec::with_capacity(arguments.len());
        for (index, argument) in arguments.iter().enumerate() {
            let (expected, expected_span) = if self.is_lambda_syntax(source, argument.value) {
                unique_expected
                    .as_ref()
                    .and_then(|expected| expected.get(index).copied())
                    .map_or((None, None), |(ty, span)| (Some(ty), span))
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
        let selected = if viable.len() == 1 {
            viable[0]
        } else {
            self.emit(
                if viable.is_empty() {
                    codes::NO_MATCHING_OVERLOAD
                } else {
                    codes::AMBIGUOUS_CALL
                },
                if viable.is_empty() {
                    "no overload matches the call arguments"
                } else {
                    "call is ambiguous between multiple overloads"
                },
                callee_span,
            )?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        };
        let (declaration, callable, mapping) = mapped.swap_remove(selected);
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
        Ok(ExpressionCheck {
            ty: callable.return_type(),
            falls_through: !self.is_builtin(callable.return_type(), BuiltinType::Nothing),
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
