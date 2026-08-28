//! SPEC-0197 compilation-unit source callable mapping、选择与 typed descriptor。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{
        DeclarationId, ExternalSymbolId, Namespace, SourceUnitId, UnitReferenceTarget,
    },
    parser::{CallArgument, ParameterModeMarker},
    type_checking::{
        BuiltinType, DeferredReason, EnvironmentFunctionEffect, ExternalTypeBinding,
        TypeCheckingError, UnitCallArgumentDescriptor, UnitCallDescriptor, UnitCallTarget,
        UnitCallableInstanceKey, UnitCallableSignature, UnitExpressionId,
        UnitFunctionParameterType, UnitTypeId, UnitTypeKind,
        argument_mapping::{MappedParameter, MappingError, map_arguments, parameter_mode_span},
    },
};

use super::{BodyChecker, CompilationUnitTypeError, ExpressionCheck};

mod generic;
mod member;

#[derive(Clone)]
struct CallCandidate {
    target: UnitCallTarget,
    declaration_span: Option<crate::source::Span>,
    type_parameters: Vec<crate::name_resolution::UnitSymbolId>,
    move_only: bool,
    parameters: Vec<MappedParameter<UnitTypeId>>,
    return_type: UnitTypeId,
    instance_arguments: Vec<UnitTypeId>,
    owner_substitutions: BTreeMap<crate::name_resolution::UnitSymbolId, UnitTypeId>,
    cross_thread_parameters: BTreeSet<usize>,
    aborts: bool,
    prints_line: bool,
}

impl CallCandidate {
    fn from_signature(declaration: DeclarationId, callable: &UnitCallableSignature) -> Self {
        Self::from_source(UnitCallTarget::Declaration(declaration), callable)
    }

    fn from_source(target: UnitCallTarget, callable: &UnitCallableSignature) -> Self {
        Self {
            target,
            declaration_span: Some(callable.name_span()),
            type_parameters: callable.type_parameters().to_vec(),
            move_only: false,
            parameters: callable
                .parameters()
                .iter()
                .map(|parameter| MappedParameter {
                    name: parameter.name().map(str::to_owned),
                    mode: parameter.mode(),
                    ty: parameter.ty(),
                    span: Some(parameter.span()),
                })
                .collect(),
            return_type: callable.return_type(),
            instance_arguments: Vec::new(),
            owner_substitutions: BTreeMap::new(),
            cross_thread_parameters: BTreeSet::new(),
            aborts: false,
            prints_line: false,
        }
    }
}

impl BodyChecker<'_> {
    pub(super) fn check_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
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
        if let Some(result) = self.check_rc_share_call(
            source,
            expression,
            call_span,
            callee,
            type_arguments,
            arguments,
            return_type,
        )? {
            return Ok(result);
        }
        let callee_payload = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?
            .payload()
            .clone();
        let target = self
            .reference(source, callee_span, Namespace::Value)
            .cloned();
        let explicit_types = type_arguments
            .iter()
            .map(|type_argument| self.resolve_body_type_ref(source, *type_argument))
            .collect::<Result<Vec<_>, _>>()?;
        let mut candidates = match target.clone() {
            Some(UnitReferenceTarget::Declaration(declaration)) => vec![declaration],
            Some(UnitReferenceTarget::OverloadSet(declarations)) => declarations,
            _ => Vec::new(),
        }
        .into_iter()
        .filter_map(|declaration| {
            self.signatures
                .declaration(declaration)
                .and_then(|signature| signature.callable())
                .map(|callable| CallCandidate::from_signature(declaration, callable))
        })
        .collect::<Vec<_>>();
        match target.clone() {
            Some(UnitReferenceTarget::Symbol(symbol)) => {
                if let Some(candidate) = self.symbol_candidate(symbol)? {
                    candidates.push(candidate);
                }
            }
            Some(UnitReferenceTarget::Symbols(symbols)) => {
                for symbol in symbols {
                    if let Some(candidate) = self.symbol_candidate(symbol)? {
                        candidates.push(candidate);
                    }
                }
            }
            _ => {}
        }
        if let crate::parser::Expression::Member {
            receiver,
            name_span,
            safe: false,
            ..
        } = callee_payload
        {
            candidates.extend(self.member_call_candidates(
                source,
                receiver,
                name_span,
                return_type,
            )?);
        }
        match target.clone() {
            Some(UnitReferenceTarget::External(external)) => {
                if let Some(candidate) = self.external_candidate(external) {
                    candidates.push(candidate);
                }
            }
            Some(UnitReferenceTarget::ExternalOverloadSet(externals)) => {
                candidates.extend(
                    externals
                        .into_iter()
                        .filter_map(|external| self.external_candidate(external)),
                );
            }
            _ => {}
        }
        let handles_source_type_arguments = candidates.iter().any(|candidate| {
            matches!(
                candidate.target,
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            )
        });
        if self.target_uses_intrinsic_callable(target.as_ref()) {
            return Err(CompilationUnitTypeError::UnsupportedBody(callee_span));
        }
        if let Some(UnitReferenceTarget::ExternalOverloadSet(externals)) = &target
            && externals
                .iter()
                .any(|external| self.environment.binding(*external).is_none())
        {
            self.check_call_arguments_without_expected(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::UnboundExternalType),
                falls_through: true,
            });
        }
        if !type_arguments.is_empty() && !handles_source_type_arguments {
            if !self.target_is_external_function(target.as_ref()) {
                self.check_expression(source, callee, None, None, return_type)?;
            }
            self.check_call_arguments_without_expected(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::Call),
                falls_through: true,
            });
        }
        if candidates.is_empty() {
            let callee_result = self.check_expression(source, callee, None, None, return_type)?;
            if let Some(UnitTypeKind::Function {
                move_only,
                parameters,
                return_type,
            }) = self.signatures.types().get(callee_result.ty).cloned()
            {
                candidates.push(CallCandidate {
                    target: UnitCallTarget::FunctionValue,
                    declaration_span: None,
                    type_parameters: Vec::new(),
                    move_only,
                    parameters: parameters
                        .into_iter()
                        .map(|parameter| MappedParameter {
                            name: None,
                            mode: parameter.mode(),
                            ty: parameter.ty(),
                            span: None,
                        })
                        .collect(),
                    return_type,
                    instance_arguments: Vec::new(),
                    owner_substitutions: BTreeMap::new(),
                    cross_thread_parameters: BTreeSet::new(),
                    aborts: false,
                    prints_line: false,
                });
            } else if !self
                .parts
                .aggregate_projections
                .iter()
                .any(|projection| projection.expression() == UnitExpressionId::new(source, callee))
                && let Some(result) = self.check_structural_component_call(
                    source,
                    expression,
                    callee,
                    type_arguments,
                    arguments,
                    return_type,
                )?
            {
                return Ok(result);
            } else if self.is_error(callee_result.ty) || self.is_deferred(callee_result.ty) {
                self.check_call_arguments_without_expected(source, arguments, return_type)?;
                return Ok(callee_result);
            } else {
                self.check_call_arguments_without_expected(source, arguments, return_type)?;
                self.emit(
                    codes::NON_CALLABLE_TARGET,
                    "call target does not have a callable type",
                    callee_span,
                )?;
                return Ok(ExpressionCheck {
                    ty: self.error_type(),
                    falls_through: true,
                });
            }
        }
        let initial_candidate_count = candidates.len();
        let mut mapped = Vec::new();
        let mut first_mapping_error = None;
        for candidate in candidates {
            match map_arguments(
                self.sources,
                &candidate.parameters,
                arguments,
                call_span,
                |argument| Ok(self.is_inout_argument_syntax(source, argument)),
            )? {
                Ok(mapping) => mapped.push((candidate, mapping)),
                Err(error) => {
                    first_mapping_error.get_or_insert(error);
                }
            }
        }
        if mapped.is_empty() {
            if let Some(error) = first_mapping_error {
                self.emit_mapping_error(error)?;
            }
            self.check_call_arguments_without_expected(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        let needs_inference = explicit_types.is_empty()
            && mapped
                .iter()
                .any(|(candidate, _)| !candidate.type_parameters.is_empty());
        let inference_types = if needs_inference {
            self.check_generic_inference_arguments(source, arguments, return_type)?
        } else {
            vec![None; arguments.len()]
        };
        let mut instantiated = Vec::with_capacity(mapped.len());
        let mut first_instantiation_error = None;
        for (candidate, mapping) in mapped {
            match self.instantiate_source_candidate(
                source,
                candidate,
                &mapping,
                arguments,
                &inference_types,
                (type_arguments, &explicit_types),
                callee_span,
            )? {
                Ok(candidate) => instantiated.push((candidate, mapping)),
                Err(error) => {
                    first_instantiation_error.get_or_insert(error);
                }
            }
        }
        if instantiated.is_empty() {
            if initial_candidate_count == 1 {
                if let Some(error) = first_instantiation_error {
                    self.emit_unit_instantiation_failure(error)?;
                }
            } else {
                return self.overload_failure(callee_span, true);
            }
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        let mut mapped = instantiated;
        let prechecked = needs_inference.then_some(inference_types.as_slice());
        if mapped.len() == 1 {
            let (candidate, mapping) = mapped
                .pop()
                .expect("one mapped compilation-unit callable candidate");
            return self.finish_candidate(
                source,
                expression,
                callee,
                arguments,
                return_type,
                &candidate,
                &mapping,
                prechecked,
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
                prechecked,
            );
        }
        let argument_types = if needs_inference {
            inference_types
                .iter()
                .copied()
                .collect::<Option<Vec<_>>>()
                .expect("non-lambda call inference prechecks every argument")
        } else {
            arguments
                .iter()
                .map(|argument| {
                    self.check_expression(source, argument.value, None, None, return_type)
                        .map(|result| result.ty)
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        if argument_types.iter().any(|ty| self.is_error(*ty)) {
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        if argument_types.iter().any(|ty| self.is_deferred(*ty)) {
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::Call),
                falls_through: true,
            });
        }
        let viable = mapped
            .iter()
            .enumerate()
            .filter_map(|(candidate_index, (candidate, mapping))| {
                arguments
                    .iter()
                    .enumerate()
                    .all(|(argument_index, _)| {
                        self.assignable(
                            argument_types[argument_index],
                            candidate.parameters[mapping[argument_index]].ty,
                        )
                    })
                    .then_some(candidate_index)
            })
            .collect::<Vec<_>>();
        if viable.len() != 1 {
            return self.overload_failure(callee_span, viable.is_empty());
        }
        let (candidate, mapping) = mapped.swap_remove(viable[0]);
        let mut valid = true;
        for (argument_index, argument) in arguments.iter().enumerate() {
            valid &= self.validate_inout_argument(
                source,
                argument,
                &candidate.parameters[mapping[argument_index]],
            )?;
        }
        if !valid {
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        self.record_call(source, expression, callee, arguments, &candidate, &mapping);
        Ok(ExpressionCheck {
            ty: candidate.return_type,
            falls_through: !self.is_builtin(candidate.return_type, BuiltinType::Nothing),
        })
    }

    fn external_candidate(&mut self, external: ExternalSymbolId) -> Option<CallCandidate> {
        let ExternalTypeBinding::Function(signature) = self.environment.binding(external)?.clone()
        else {
            return None;
        };
        let parameters = signature
            .parameters
            .iter()
            .map(|parameter| MappedParameter {
                name: None,
                mode: parameter.mode,
                ty: self.normalize_environment_type(&parameter.ty),
                span: None,
            })
            .collect();
        Some(CallCandidate {
            target: UnitCallTarget::External(external),
            declaration_span: None,
            type_parameters: Vec::new(),
            move_only: false,
            parameters,
            return_type: self.normalize_environment_type(&signature.return_type),
            instance_arguments: Vec::new(),
            owner_substitutions: BTreeMap::new(),
            cross_thread_parameters: signature
                .effects
                .iter()
                .filter_map(|effect| match effect {
                    EnvironmentFunctionEffect::CrossThreadTransfer { parameter } => {
                        Some(*parameter)
                    }
                    EnvironmentFunctionEffect::Abort | EnvironmentFunctionEffect::PrintLine => None,
                })
                .collect(),
            aborts: signature
                .effects
                .contains(&EnvironmentFunctionEffect::Abort),
            prints_line: signature
                .effects
                .contains(&EnvironmentFunctionEffect::PrintLine),
        })
    }

    fn target_uses_intrinsic_callable(&self, target: Option<&UnitReferenceTarget>) -> bool {
        match target {
            Some(UnitReferenceTarget::External(external)) => matches!(
                self.environment.binding(*external),
                Some(ExternalTypeBinding::IntrinsicCallable(_))
            ),
            Some(UnitReferenceTarget::ExternalOverloadSet(externals)) => {
                externals.iter().any(|external| {
                    matches!(
                        self.environment.binding(*external),
                        Some(ExternalTypeBinding::IntrinsicCallable(_))
                    )
                })
            }
            _ => false,
        }
    }

    fn target_is_external_function(&self, target: Option<&UnitReferenceTarget>) -> bool {
        match target {
            Some(UnitReferenceTarget::External(external)) => matches!(
                self.environment.binding(*external),
                Some(ExternalTypeBinding::Function(_))
            ),
            Some(UnitReferenceTarget::ExternalOverloadSet(_)) => true,
            _ => false,
        }
    }

    pub(super) fn check_call_arguments_without_expected(
        &mut self,
        source: SourceUnitId,
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<(), CompilationUnitTypeError> {
        for argument in arguments {
            self.check_expression(source, argument.value, None, None, return_type)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_candidate(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        return_type: UnitTypeId,
        candidate: &CallCandidate,
        mapping: &[usize],
        prechecked_argument_types: Option<&[Option<UnitTypeId>]>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let mut valid = true;
        let mut deferred = false;
        for (argument_index, argument) in arguments.iter().enumerate() {
            let parameter = &candidate.parameters[mapping[argument_index]];
            if let Some(ty) = prechecked_argument_types
                .and_then(|types| types.get(argument_index))
                .copied()
                .flatten()
            {
                valid &= !self.is_error(ty);
                deferred |= self.is_deferred(ty);
                if !self.is_error(ty) && !self.is_deferred(ty) && !self.assignable(ty, parameter.ty)
                {
                    self.emit_call_argument_mismatch(source, argument, parameter, ty)?;
                    valid = false;
                }
                if !self.is_error(ty) && !self.is_deferred(ty) {
                    valid &= self.validate_inout_argument(source, argument, parameter)?;
                }
                continue;
            }
            let result = self.check_expression(
                source,
                argument.value,
                Some(parameter.ty),
                parameter.span,
                return_type,
            )?;
            valid &= !self.is_error(result.ty);
            deferred |= self.is_deferred(result.ty);
            if !self.is_error(result.ty)
                && !self.is_deferred(result.ty)
                && !self.assignable(result.ty, parameter.ty)
            {
                self.emit_call_argument_mismatch(source, argument, parameter, result.ty)?;
                valid = false;
            }
            if !self.is_error(result.ty) && !self.is_deferred(result.ty) {
                valid &= self.validate_inout_argument(source, argument, parameter)?;
            }
        }
        if deferred {
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::Call),
                falls_through: true,
            });
        }
        if !valid {
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        self.record_call(source, expression, callee, arguments, candidate, mapping);
        Ok(ExpressionCheck {
            ty: candidate.return_type,
            falls_through: !self.is_builtin(candidate.return_type, BuiltinType::Nothing),
        })
    }

    fn emit_call_argument_mismatch(
        &mut self,
        source: SourceUnitId,
        argument: &CallArgument,
        parameter: &MappedParameter<UnitTypeId>,
        actual: UnitTypeId,
    ) -> Result<(), CompilationUnitTypeError> {
        let primary = self
            .file(source)
            .ast()
            .expressions()
            .get(argument.value)
            .map_err(TypeCheckingError::from)?
            .span();
        self.emit_maybe_label(
            codes::TYPE_MISMATCH,
            "expression type does not match the expected type",
            primary,
            parameter.span,
            format!(
                "expected {}, found {}",
                self.type_name(parameter.ty),
                self.type_name(actual)
            ),
        )
    }

    fn validate_inout_argument(
        &mut self,
        source: SourceUnitId,
        argument: &CallArgument,
        parameter: &MappedParameter<UnitTypeId>,
    ) -> Result<bool, CompilationUnitTypeError> {
        let Some(marker @ ParameterModeMarker::Inout(_)) = argument.mode_marker else {
            return Ok(true);
        };
        if parameter.mode != crate::type_checking::ParameterMode::Inout
            || self.is_mutable_inout_place(source, argument.value)?
        {
            return Ok(true);
        }
        self.emit_mapping_error(MappingError::Mode {
            primary: parameter_mode_span(marker),
            parameter: parameter.span,
        })?;
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_overload_lambda_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        return_type: UnitTypeId,
        mut candidates: Vec<(CallCandidate, Vec<usize>)>,
        lambda_arguments: &[bool],
        prechecked_argument_types: Option<&[Option<UnitTypeId>]>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let mut argument_types = vec![None; arguments.len()];
        let mut has_error = false;
        let mut has_deferred = false;
        for (index, argument) in arguments.iter().enumerate() {
            if lambda_arguments[index] {
                continue;
            }
            if let Some(ty) = prechecked_argument_types
                .and_then(|types| types.get(index))
                .copied()
                .flatten()
            {
                has_error |= self.is_error(ty);
                has_deferred |= self.is_deferred(ty);
                argument_types[index] = Some(ty);
                continue;
            }
            let result = self.check_expression(source, argument.value, None, None, return_type)?;
            has_error |= self.is_error(result.ty);
            has_deferred |= self.is_deferred(result.ty);
            argument_types[index] = Some(result.ty);
        }
        if has_error {
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        if has_deferred {
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::Call),
                falls_through: true,
            });
        }
        candidates.retain(|(candidate, mapping)| {
            argument_types.iter().enumerate().all(|(index, actual)| {
                actual.is_none_or(|actual| {
                    self.assignable(actual, candidate.parameters[mapping[index]].ty)
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
                let (candidate, mapping) = candidates
                    .pop()
                    .expect("one candidate remains after non-lambda filtering");
                return self.finish_candidate(
                    source,
                    expression,
                    callee,
                    arguments,
                    return_type,
                    &candidate,
                    &mapping,
                    Some(&argument_types),
                );
            }
            _ => {}
        }

        let baseline = self.trial_state();
        let baseline_diagnostics = self.diagnostics.len();
        let mut successes = Vec::new();
        let mut has_deferred_trial = false;
        for (candidate, mapping) in candidates {
            self.restore_trial_state(baseline.clone());
            let declaration_span = candidate.declaration_span;
            let previous_candidate_local_expected = self.candidate_local_expected;
            self.candidate_local_expected = true;
            let result = self.finish_candidate(
                source,
                expression,
                callee,
                arguments,
                return_type,
                &candidate,
                &mapping,
                Some(&argument_types),
            );
            self.candidate_local_expected = previous_candidate_local_expected;
            let result = match result {
                Ok(result) => result,
                Err(error) => {
                    self.restore_trial_state(baseline);
                    return Err(error);
                }
            };
            let trial_deferred =
                self.is_deferred(result.ty) || self.trial_introduced_deferred(&baseline);
            has_deferred_trial |= trial_deferred;
            if self.diagnostics.len() == baseline_diagnostics
                && !self.is_error(result.ty)
                && !trial_deferred
            {
                successes.push((self.trial_state(), result, declaration_span));
            }
        }
        self.restore_trial_state(baseline);
        if has_deferred_trial {
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::Call),
                falls_through: true,
            });
        }
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
                    .filter_map(|(_, _, span)| *span)
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
        callee: ExpressionId,
        arguments: &[CallArgument],
        candidate: &CallCandidate,
        mapping: &[usize],
    ) {
        let descriptors = mapping
            .iter()
            .enumerate()
            .map(|(argument_index, &parameter_index)| {
                let parameter = &candidate.parameters[parameter_index];
                UnitCallArgumentDescriptor {
                    argument_index,
                    parameter_index,
                    category: self.expression_category(source, arguments[argument_index].value),
                    mode: parameter.mode,
                    parameter_type: parameter.ty,
                    cross_thread: candidate.cross_thread_parameters.contains(&parameter_index),
                }
            })
            .collect();
        let function_type = self.signatures.types_mut().intern(UnitTypeKind::Function {
            move_only: candidate.move_only,
            parameters: candidate
                .parameters
                .iter()
                .map(|parameter| UnitFunctionParameterType::new(parameter.mode, parameter.ty))
                .collect(),
            return_type: candidate.return_type,
        });
        let callee = UnitExpressionId::new(source, callee);
        self.parts.expression_types.insert(callee, function_type);
        self.parts
            .expression_categories
            .insert(callee, crate::type_checking::ExpressionCategory::Temporary);
        self.parts.calls.push(UnitCallDescriptor {
            expression: UnitExpressionId::new(source, expression),
            instance: UnitCallableInstanceKey {
                target: candidate.target,
                type_arguments: candidate.instance_arguments.clone(),
            },
            return_type: candidate.return_type,
            arguments: descriptors,
            aborts: candidate.aborts,
            prints_line: candidate.prints_line,
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

    pub(super) fn emit_mapping_error(
        &mut self,
        error: MappingError,
    ) -> Result<(), CompilationUnitTypeError> {
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
