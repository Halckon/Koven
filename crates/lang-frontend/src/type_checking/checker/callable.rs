use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::{ExternalSymbolId, Namespace, ReferenceTarget, SymbolId, SymbolKind},
    parser::{CallArgument, Expression, ParameterModeMarker},
    source::Span,
    type_checking::{
        CallArgumentDescriptor, CallDescriptor, CallableTarget, ExpressionCategory,
        ExternalTypeBinding,
    },
};

use super::*;

#[derive(Clone)]
struct CallParameter {
    name: Option<String>,
    mode: ParameterMode,
    ty: TypeId,
    span: Option<Span>,
}

#[derive(Clone)]
struct CallCandidate {
    target: CallableTarget,
    declaration_span: Option<Span>,
    generic: bool,
    parameters: Vec<CallParameter>,
    return_type: TypeId,
}

#[derive(Clone, Copy)]
enum MappingError {
    Named(Span),
    Arity(Span),
    Mode {
        primary: Span,
        parameter: Option<Span>,
    },
}

impl Checker<'_> {
    pub(super) fn check_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: Vec<TypeRefId>,
        arguments: Vec<CallArgument>,
        expected: Option<TypeId>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if let Some(result) = self.check_intrinsic_container_call(
            expression,
            call_span,
            callee,
            &type_arguments,
            &arguments,
            expected,
        )? {
            return Ok(result);
        }
        for &type_argument in &type_arguments {
            self.resolve_type_ref(type_argument)?;
        }

        let mut candidates = self.call_candidates(callee)?;
        if !type_arguments.is_empty() || candidates.iter().any(|candidate| candidate.generic) {
            self.check_deferred_arguments(callee, &arguments)?;
            return Ok(ExprCheck {
                ty: self.deferred(DeferredReason::Call),
                falls_through: true,
            });
        }

        if candidates.is_empty() {
            let callee_result = self.check_expression(callee, None, None)?;
            if let TypeKind::Function {
                parameters,
                return_type,
                ..
            } = self.kind(callee_result.ty).clone()
            {
                candidates.push(CallCandidate {
                    target: CallableTarget::FunctionValue,
                    declaration_span: None,
                    generic: false,
                    parameters: parameters
                        .into_iter()
                        .map(|parameter| CallParameter {
                            name: None,
                            mode: parameter.mode,
                            ty: parameter.ty,
                            span: None,
                        })
                        .collect(),
                    return_type,
                });
            } else if self.aggregate_projection_for(callee).is_none()
                && let Some(result) = self.check_structural_component_call(
                    expression,
                    callee,
                    &type_arguments,
                    &arguments,
                )?
            {
                return Ok(result);
            } else if self.is_error(callee_result.ty) || self.is_deferred(callee_result.ty) {
                for argument in &arguments {
                    self.check_expression(argument.value, None, None)?;
                }
                return Ok(ExprCheck {
                    ty: callee_result.ty,
                    falls_through: true,
                });
            } else {
                for argument in &arguments {
                    self.check_expression(argument.value, None, None)?;
                }
                self.emit(
                    self.non_callable_target_code,
                    "call target does not have a callable type",
                    self.ast().expressions().get(callee)?.span(),
                )?;
                return Ok(ExprCheck {
                    ty: self.error_type(),
                    falls_through: true,
                });
            }
        }

        let mut mapped = Vec::new();
        let mut first_error = None;
        for candidate in candidates {
            match self.map_arguments(&candidate, &arguments, call_span)? {
                Ok(mapping) => mapped.push((candidate, mapping)),
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        if mapped.is_empty() {
            if let Some(error) = first_error {
                self.emit_mapping_error(error)?;
            }
            for argument in &arguments {
                self.check_expression(argument.value, None, None)?;
            }
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        if mapped.len() == 1 {
            let (candidate, mapping) = mapped.pop().expect("one mapped candidate");
            return self.finish_unique_call(expression, callee, &arguments, candidate, mapping);
        }

        let mut argument_types = Vec::with_capacity(arguments.len());
        let mut poisoned = false;
        for argument in &arguments {
            let result = self.check_expression(argument.value, None, None)?;
            poisoned |= self.is_error(result.ty) || self.is_deferred(result.ty);
            argument_types.push(result.ty);
        }
        if poisoned {
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        mapped.retain(|(candidate, mapping)| {
            mapping
                .iter()
                .enumerate()
                .all(|(argument_index, &parameter_index)| {
                    self.assignable(
                        argument_types[argument_index],
                        candidate.parameters[parameter_index].ty,
                    )
                })
        });
        match mapped.len() {
            0 => {
                self.emit(
                    self.no_matching_overload_code,
                    "no overload matches the call arguments",
                    self.ast().expressions().get(callee)?.span(),
                )?;
                Ok(ExprCheck {
                    ty: self.error_type(),
                    falls_through: true,
                })
            }
            1 => {
                let (candidate, mapping) = mapped.pop().expect("one matching candidate");
                self.record_call(expression, callee, &arguments, candidate, mapping)
            }
            _ => {
                let primary = self.ast().expressions().get(callee)?.span();
                let mut diagnostic = Diagnostic::new(
                    self.sources,
                    Severity::Error,
                    self.ambiguous_call_code,
                    "call remains ambiguous after argument type checking",
                    primary,
                )?;
                for (candidate, _) in mapped.iter().take(2) {
                    if let Some(span) = candidate.declaration_span {
                        diagnostic.add_label(
                            self.sources,
                            span,
                            "matching callable declared here",
                        )?;
                    }
                }
                self.diagnostics.push(diagnostic);
                Ok(ExprCheck {
                    ty: self.error_type(),
                    falls_through: true,
                })
            }
        }
    }

    fn check_deferred_arguments(
        &mut self,
        callee: ExpressionId,
        arguments: &[CallArgument],
    ) -> Result<(), TypeCheckingError> {
        self.check_expression(callee, None, None)?;
        for argument in arguments {
            self.check_expression(argument.value, None, None)?;
        }
        Ok(())
    }

    fn call_candidates(
        &mut self,
        callee: ExpressionId,
    ) -> Result<Vec<CallCandidate>, TypeCheckingError> {
        let node = self.ast().expressions().get(callee)?;
        match node.payload().clone() {
            Expression::Name => self.name_call_candidates(node.span()),
            Expression::Member {
                receiver,
                name_span,
                safe,
                ..
            } if !safe => self.member_call_candidates(receiver, name_span),
            _ => Ok(Vec::new()),
        }
    }

    fn name_call_candidates(
        &mut self,
        span: Span,
    ) -> Result<Vec<CallCandidate>, TypeCheckingError> {
        let target = self.reference(span, Namespace::Value).cloned();
        match target {
            Some(ReferenceTarget::Symbol(symbol)) => Ok(self
                .source_candidate(symbol, BTreeMap::new())?
                .into_iter()
                .collect()),
            Some(ReferenceTarget::OverloadSet(symbols)) => symbols
                .into_iter()
                .filter_map(|symbol| self.source_candidate(symbol, BTreeMap::new()).transpose())
                .collect(),
            Some(ReferenceTarget::External(external)) => {
                Ok(self.external_candidate(external)?.into_iter().collect())
            }
            Some(ReferenceTarget::ExternalOverloadSet(externals)) => externals
                .into_iter()
                .filter_map(|external| self.external_candidate(external).transpose())
                .collect(),
            _ => Ok(Vec::new()),
        }
    }

    fn member_call_candidates(
        &mut self,
        receiver: ExpressionId,
        name_span: Span,
    ) -> Result<Vec<CallCandidate>, TypeCheckingError> {
        let receiver_type = self.check_expression(receiver, None, None)?.ty;
        let TypeKind::Nominal { nominal, arguments } = self.kind(receiver_type).clone() else {
            return Ok(Vec::new());
        };
        let owner = self
            .nominals
            .iter()
            .find(|descriptor| descriptor.id() == nominal)
            .cloned()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        let substitutions = owner
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments)
            .collect::<BTreeMap<_, _>>();
        let mut instances = vec![receiver_type];
        for interface in owner.interfaces() {
            instances.push(self.substitute_type(*interface, &substitutions)?);
        }
        let name = self.sources.slice(name_span)?.to_owned();
        let descriptors = self.typed_callables.clone();
        let mut candidates = Vec::new();
        let mut seen_shapes = BTreeSet::new();
        for instance in instances {
            let TypeKind::Nominal {
                nominal: owner_id,
                arguments,
            } = self.kind(instance).clone()
            else {
                continue;
            };
            let owner_descriptor = self
                .nominals
                .iter()
                .find(|descriptor| descriptor.id() == owner_id)
                .cloned()
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            let owner_substitutions = owner_descriptor
                .type_parameters()
                .iter()
                .copied()
                .zip(arguments)
                .collect::<BTreeMap<_, _>>();
            for descriptor in descriptors
                .iter()
                .filter(|descriptor| descriptor.owner() == Some(owner_id))
            {
                if self
                    .sources
                    .slice(self.symbol_spans[descriptor.symbol().index()])?
                    != name
                {
                    continue;
                }
                if let Some(candidate) =
                    self.source_candidate(descriptor.symbol(), owner_substitutions.clone())?
                {
                    let shape = (
                        candidate.generic,
                        candidate
                            .parameters
                            .iter()
                            .map(|parameter| parameter.ty)
                            .collect::<Vec<_>>(),
                    );
                    if seen_shapes.insert(shape) {
                        candidates.push(candidate);
                    }
                }
            }
        }
        Ok(candidates)
    }

    fn source_candidate(
        &mut self,
        symbol: SymbolId,
        substitutions: BTreeMap<SymbolId, TypeId>,
    ) -> Result<Option<CallCandidate>, TypeCheckingError> {
        let Some(descriptor) = self
            .typed_callables
            .iter()
            .find(|descriptor| descriptor.symbol() == symbol)
            .cloned()
        else {
            return Ok(None);
        };
        let mut parameters = Vec::with_capacity(descriptor.parameters().len());
        for (index, parameter) in descriptor.parameters().iter().enumerate() {
            let symbol = descriptor.parameter_symbols()[index];
            parameters.push(CallParameter {
                name: symbol
                    .map(|symbol| self.sources.slice(self.symbol_spans[symbol.index()]))
                    .transpose()?
                    .map(str::to_owned),
                mode: parameter.mode,
                ty: self.substitute_type(parameter.ty, &substitutions)?,
                span: symbol.map(|symbol| self.symbol_spans[symbol.index()]),
            });
        }
        Ok(Some(CallCandidate {
            target: CallableTarget::Source(symbol),
            declaration_span: Some(self.symbol_spans[symbol.index()]),
            generic: !descriptor.type_parameters().is_empty(),
            parameters,
            return_type: self.substitute_type(descriptor.return_type(), &substitutions)?,
        }))
    }

    fn external_candidate(
        &mut self,
        external: ExternalSymbolId,
    ) -> Result<Option<CallCandidate>, TypeCheckingError> {
        let Some(ExternalTypeBinding::Function(signature)) =
            self.environment.binding(external).cloned()
        else {
            return Ok(None);
        };
        let parameters = signature
            .parameters
            .iter()
            .map(|parameter| CallParameter {
                name: None,
                mode: parameter.mode,
                ty: self.normalize_environment_type(&parameter.ty),
                span: None,
            })
            .collect();
        Ok(Some(CallCandidate {
            target: CallableTarget::External(external),
            declaration_span: None,
            generic: false,
            parameters,
            return_type: self.normalize_environment_type(&signature.return_type),
        }))
    }

    fn map_arguments(
        &self,
        candidate: &CallCandidate,
        arguments: &[CallArgument],
        call_span: Span,
    ) -> Result<Result<Vec<usize>, MappingError>, TypeCheckingError> {
        let mut mapping = Vec::with_capacity(arguments.len());
        let mut used = BTreeSet::new();
        let mut next_position = 0;
        let mut saw_named = false;
        for argument in arguments {
            let parameter_index = if let Some(prefix) = argument.named_prefix {
                saw_named = true;
                let name = self.sources.slice(prefix.name_span)?;
                let Some(index) = candidate
                    .parameters
                    .iter()
                    .position(|parameter| parameter.name.as_deref() == Some(name))
                else {
                    return Ok(Err(MappingError::Named(prefix.name_span)));
                };
                if !used.insert(index) {
                    return Ok(Err(MappingError::Named(prefix.name_span)));
                }
                index
            } else {
                if saw_named {
                    return Ok(Err(MappingError::Named(argument.span)));
                }
                while used.contains(&next_position) {
                    next_position += 1;
                }
                if next_position >= candidate.parameters.len() {
                    return Ok(Err(MappingError::Arity(argument.span)));
                }
                let index = next_position;
                used.insert(index);
                next_position += 1;
                index
            };
            let parameter = &candidate.parameters[parameter_index];
            let mode_matches = match (argument.mode_marker, parameter.mode) {
                (None, ParameterMode::Value | ParameterMode::Borrow) => true,
                (Some(ParameterModeMarker::Borrow(_)), ParameterMode::Borrow) => true,
                (Some(ParameterModeMarker::Inout(_)), ParameterMode::Inout) => {
                    self.expression_types[argument.value.index()].map_or_else(
                        || self.is_syntactic_place(argument.value),
                        |_| {
                            self.expression_categories[argument.value.index()]
                                == ExpressionCategory::Place
                                && self
                                    .is_mutable_element_place(argument.value)
                                    .unwrap_or(true)
                        },
                    )
                }
                _ => false,
            };
            if !mode_matches {
                return Ok(Err(MappingError::Mode {
                    primary: argument
                        .mode_marker
                        .map(parameter_mode_span)
                        .unwrap_or(argument.span),
                    parameter: parameter.span,
                }));
            }
            mapping.push(parameter_index);
        }
        if used.len() != candidate.parameters.len() {
            return Ok(Err(MappingError::Arity(call_span)));
        }
        Ok(Ok(mapping))
    }

    fn finish_unique_call(
        &mut self,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        candidate: CallCandidate,
        mapping: Vec<usize>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let mut valid = true;
        let mut deferred = false;
        for (argument_index, argument) in arguments.iter().enumerate() {
            let parameter = &candidate.parameters[mapping[argument_index]];
            let result =
                self.check_expression(argument.value, Some(parameter.ty), parameter.span)?;
            valid &= !self.is_error(result.ty);
            deferred |= self.is_deferred(result.ty);
            if !deferred
                && matches!(argument.mode_marker, Some(ParameterModeMarker::Inout(_)))
                && (self.expression_categories[argument.value.index()] != ExpressionCategory::Place
                    || self.is_mutable_element_place(argument.value) == Some(false))
            {
                self.emit_mapping_error(MappingError::Mode {
                    primary: argument
                        .mode_marker
                        .map(parameter_mode_span)
                        .unwrap_or(argument.span),
                    parameter: parameter.span,
                })?;
                valid = false;
            }
        }
        if deferred {
            return Ok(ExprCheck {
                ty: self.deferred(DeferredReason::Call),
                falls_through: true,
            });
        }
        if !valid {
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        self.record_call(expression, callee, arguments, candidate, mapping)
    }

    fn record_call(
        &mut self,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        candidate: CallCandidate,
        mapping: Vec<usize>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let function = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: candidate
                .parameters
                .iter()
                .map(|parameter| FunctionParameterType {
                    mode: parameter.mode,
                    ty: parameter.ty,
                })
                .collect(),
            return_type: candidate.return_type,
        });
        self.set_expression(callee, function);
        self.set_expression_category(callee, ExpressionCategory::Temporary);
        let descriptors = arguments
            .iter()
            .enumerate()
            .map(|(argument_index, argument)| {
                CallArgumentDescriptor::new(
                    argument_index,
                    mapping[argument_index],
                    self.expression_categories[argument.value.index()],
                    candidate.parameters[mapping[argument_index]].mode,
                )
            })
            .collect();
        self.calls.push(CallDescriptor::new(
            expression,
            candidate.target,
            candidate.return_type,
            descriptors,
        ));
        Ok(ExprCheck {
            ty: candidate.return_type,
            falls_through: true,
        })
    }

    fn emit_mapping_error(&mut self, error: MappingError) -> Result<(), TypeCheckingError> {
        match error {
            MappingError::Named(primary) => self.emit(
                self.invalid_named_argument_code,
                "named argument does not map uniquely to a callable parameter",
                primary,
            ),
            MappingError::Arity(primary) => self.emit(
                self.call_argument_arity_code,
                "call must fill every parameter exactly once",
                primary,
            ),
            MappingError::Mode { primary, parameter } => {
                if let Some(parameter) = parameter {
                    self.emit_with_label(
                        self.call_argument_mode_code,
                        "argument marker does not match the parameter contract",
                        primary,
                        parameter,
                        "parameter contract declared here",
                    )
                } else {
                    self.emit(
                        self.call_argument_mode_code,
                        "argument marker does not match the parameter contract",
                        primary,
                    )
                }
            }
        }
    }

    pub(super) fn classify_expression_category(
        &self,
        expression: ExpressionId,
        ty: TypeId,
    ) -> ExpressionCategory {
        let Ok(node) = self.ast().expressions().get(expression) else {
            return ExpressionCategory::Temporary;
        };
        match node.payload() {
            Expression::This => ExpressionCategory::Place,
            Expression::Group { expression } => self.expression_categories[expression.index()],
            Expression::Name => {
                let Some(ReferenceTarget::Symbol(symbol)) =
                    self.reference(node.span(), Namespace::Value)
                else {
                    return ExpressionCategory::Temporary;
                };
                if matches!(
                    self.symbol_kinds[symbol.index()],
                    SymbolKind::Variable
                        | SymbolKind::Field
                        | SymbolKind::ValueParameter
                        | SymbolKind::LambdaParameter
                        | SymbolKind::ForBinding
                        | SymbolKind::DestructuringBinding
                ) {
                    ExpressionCategory::Place
                } else {
                    ExpressionCategory::Temporary
                }
            }
            Expression::Member { .. }
                if self
                    .is_read_only_container_size(expression)
                    .unwrap_or(false) =>
            {
                ExpressionCategory::Temporary
            }
            Expression::Member { .. } | Expression::Index { .. }
                if self.types.get(ty).is_some()
                    && !matches!(self.kind(ty), TypeKind::Error | TypeKind::Deferred(_)) =>
            {
                ExpressionCategory::Place
            }
            _ => ExpressionCategory::Temporary,
        }
    }

    fn is_syntactic_place(&self, expression: ExpressionId) -> bool {
        let Ok(node) = self.ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::This | Expression::Member { .. } | Expression::Index { .. } => true,
            Expression::Name => matches!(
                self.reference(node.span(), Namespace::Value),
                Some(ReferenceTarget::Symbol(symbol))
                    if matches!(
                        self.symbol_kinds[symbol.index()],
                        SymbolKind::Variable
                            | SymbolKind::Field
                            | SymbolKind::ValueParameter
                            | SymbolKind::LambdaParameter
                            | SymbolKind::ForBinding
                            | SymbolKind::DestructuringBinding
                    )
            ),
            Expression::Group { expression } => self.is_syntactic_place(*expression),
            _ => false,
        }
    }
}

const fn parameter_mode_span(marker: ParameterModeMarker) -> Span {
    match marker {
        ParameterModeMarker::Own(span)
        | ParameterModeMarker::Borrow(span)
        | ParameterModeMarker::Inout(span) => span,
    }
}
