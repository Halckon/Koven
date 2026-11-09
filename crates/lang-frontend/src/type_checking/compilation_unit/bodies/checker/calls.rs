//! SPEC-0197 compilation-unit source callable mapping、选择与 typed descriptor。
mod candidate;
use candidate::CallCandidate;

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{DeclarationId, Namespace, SourceUnitId, UnitReferenceTarget},
    parser::{CallArgument, Expression, ParameterModeMarker},
    source::Span,
    type_checking::{
        BuiltinType, DeferredReason, ExternalTypeBinding, ParameterMode, TypeCheckingError,
        UnitCallArgumentDescriptor, UnitCallDescriptor, UnitCallReceiverDescriptor,
        UnitCallReceiverOrigin, UnitCallTarget, UnitCallableInstanceKey, UnitCallableSignature,
        UnitExpressionId, UnitFunctionParameterType, UnitTypeId, UnitTypeKind,
        argument_mapping::{MappedParameter, MappingError, map_arguments, parameter_mode_span},
    },
};

use super::lambda::PartialLambdaContext;
use super::pending_constraints::PendingConstraints;
use super::while_flow::PendingControlSite;
use super::{BodyChecker, CompilationUnitTypeError, ExpressionCheck, ExpressionExpected};

mod generic;
mod member;
mod overload;
mod range_extension;
mod text_extension;
mod text_primitive;

#[derive(Clone, Copy)]
struct ArgumentCheckStart {
    diagnostics: usize,
    deferred_facts: usize,
}

impl BodyChecker<'_> {
    fn unresolved_call(&mut self) -> ExpressionCheck {
        ExpressionCheck {
            ty: self.deferred_type(DeferredReason::Call),
            falls_through: true,
        }
    }

    pub(super) fn rejected_call_recovery(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> ExpressionCheck {
        // Final admission still needs the Deferred anchor for uncommitted children.
        self.inactive_inout_calls
            .insert(UnitExpressionId::new(source, expression));
        self.unresolved_call()
    }

    fn argument_check_start(&self) -> ArgumentCheckStart {
        ArgumentCheckStart {
            diagnostics: self.diagnostics.len(),
            deferred_facts: self.deferred_facts.len(),
        }
    }

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
        if let Some(result) = self.check_text_primitive_call(
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
        if let Some(result) = self.check_range_construction_call(
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
        if let Some(result) = self.check_integer_operation_call(
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
        if let Some(result) = self.check_string_clone_call(
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
            .or_else(|| match &callee_payload {
                crate::parser::Expression::Member { name_span, .. } => {
                    self.reference(source, *name_span, Namespace::Value)
                }
                _ => None,
            })
            .cloned();
        let argument_start = self.argument_check_start();
        let (explicit_types, explicit_storage_failures) =
            self.resolve_explicit_call_types(source, type_arguments)?;
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
                .filter(|callable| callable.receiver().is_none())
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
        if candidates.is_empty()
            && let crate::parser::Expression::Member {
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
        if candidates.is_empty()
            && let Expression::SuperMember {
                interface,
                name_span,
                ..
            } = callee_payload
        {
            candidates.extend(self.super_call_candidates(source, interface, name_span)?);
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
            self.check_recovery_call_arguments(source, callee, arguments, return_type)?;
            let ty = self.deferred_type(DeferredReason::UnboundExternalType);
            self.record_expression(source, callee, ty);
            return Ok(ExpressionCheck {
                ty,
                falls_through: true,
            });
        }
        if !type_arguments.is_empty() && !handles_source_type_arguments {
            let callee_result = if self.target_is_external_function(target.as_ref()) {
                None
            } else {
                Some(self.check_expression(source, callee, None, None, return_type)?)
            };
            self.check_recovery_call_arguments(source, callee, arguments, return_type)?;
            return Ok(match callee_result {
                Some(result) if self.is_error(result.ty) => result,
                Some(result) if self.is_deferred(result.ty) => self.unresolved_call(),
                _ => self.rejected_call_recovery(source, expression),
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
                    parameter_symbols: Vec::new(),
                    return_type,
                    result_source: crate::type_checking::CallableResultSource::Owned,
                    instance_arguments: Vec::new(),
                    receiver: None,
                    member_owner: None,
                    cross_thread_parameters: BTreeSet::new(),
                    aborts: false,
                    prints_line: false,
                });
            } else if self.is_error(callee_result.ty) || self.is_deferred(callee_result.ty) {
                self.check_recovery_call_arguments(source, callee, arguments, return_type)?;
                return Ok(callee_result);
            } else {
                self.check_recovery_call_arguments(source, callee, arguments, return_type)?;
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
        let callee_exits =
            !self.tracks_pending_call_flow() || self.checked_callee_exits(source, callee)?;
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
            self.check_loop_reachable(callee_exits, |checker| {
                checker.with_explicit_storage_recovery(&explicit_storage_failures, |checker| {
                    checker.check_call_arguments_without_expected(source, arguments, return_type)
                })
            })?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        self.check_loop_reachable(callee_exits, |checker| {
            checker.select_mapped_call(
                overload::CallSite {
                    source,
                    expression,
                    callee,
                    callee_span,
                    arguments,
                    return_type,
                    explicit: (type_arguments, &explicit_types),
                    explicit_storage_failures: &explicit_storage_failures,
                    argument_start,
                },
                initial_candidate_count,
                mapped,
            )
        })
    }

    fn resolve_explicit_call_types(
        &mut self,
        source: SourceUnitId,
        type_arguments: &[TypeRefId],
    ) -> Result<(Vec<UnitTypeId>, BTreeMap<UnitTypeId, Span>), CompilationUnitTypeError> {
        if type_arguments.is_empty() {
            return Ok((Vec::new(), BTreeMap::new()));
        }
        let previous = self.explicit_storage_failures.replace(BTreeMap::new());
        let types = type_arguments
            .iter()
            .map(|type_argument| self.resolve_body_type_ref(source, *type_argument))
            .collect::<Result<Vec<_>, _>>();
        let failures = std::mem::replace(&mut self.explicit_storage_failures, previous);
        Ok((types?, failures.unwrap_or_default()))
    }

    fn with_explicit_storage_recovery<T>(
        &mut self,
        failures: &BTreeMap<UnitTypeId, Span>,
        check: impl FnOnce(&mut Self) -> Result<T, CompilationUnitTypeError>,
    ) -> Result<T, CompilationUnitTypeError> {
        if failures.is_empty() {
            return check(self);
        }
        self.storage_recovery_scopes.push(failures.clone());
        let result = check(self);
        self.storage_recovery_scopes.pop();
        result
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

    fn check_recovery_call_arguments(
        &mut self,
        source: SourceUnitId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<(), CompilationUnitTypeError> {
        let callee_exits =
            !self.tracks_pending_call_flow() || self.checked_callee_exits(source, callee)?;
        self.check_loop_reachable(callee_exits, |checker| {
            checker.check_call_arguments_without_expected(source, arguments, return_type)
        })
    }

    pub(super) fn check_call_arguments_without_expected(
        &mut self,
        source: SourceUnitId,
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<(), CompilationUnitTypeError> {
        let track_arguments = self.tracks_pending_call_flow();
        let mut arguments_exit = true;
        for argument in arguments {
            let result = self.check_loop_reachable(arguments_exit, |checker| {
                checker.check_expression(source, argument.value, None, None, return_type)
            })?;
            if track_arguments && arguments_exit {
                arguments_exit =
                    self.has_pending_expression_exit(source, argument.value, result)?;
            }
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
        mut candidate: CallCandidate,
        mapping: &[usize],
        prechecked_argument_types: Option<&[Option<UnitTypeId>]>,
        candidate_types: Option<super::type_graph::CandidateTypes>,
        argument_start: ArgumentCheckStart,
        pending_instance: bool,
        constraints: Option<PendingConstraints>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let mut valid = true;
        let mut deferred = false;
        let mut uncommitted_context = false;
        let track_arguments = self.tracks_pending_call_flow();
        let mut arguments_exit = !track_arguments || self.checked_callee_exits(source, callee)?;
        let mut observed = constraints.as_ref().map(|_| vec![None; arguments.len()]);
        for (argument_index, argument) in arguments.iter().enumerate() {
            let parameter_index = mapping[argument_index];
            let parameter = &candidate.parameters[parameter_index];
            let parameter_symbol = candidate
                .parameter_symbols
                .get(parameter_index)
                .copied()
                .flatten();
            let lambda_syntax = (pending_instance || parameter_symbol.is_some())
                && self.is_lambda_syntax(source, argument.value);
            let expected_span = parameter_symbol
                .filter(|_| lambda_syntax)
                .map(|symbol| self.unit_symbol_span(symbol))
                .transpose()?
                .or(parameter.span);
            let expected = match &candidate_types {
                Some(types) => ExpressionExpected::Candidate(types, parameter.ty),
                None => ExpressionExpected::Canonical(parameter.ty),
            };
            let unresolved_context = pending_instance
                && expected.probe(self, |checker, ty| {
                    Ok(checker.contains_unresolved_call_type(ty))
                })?;
            let lambda = pending_instance && lambda_syntax;
            if lambda {
                match self.partial_lambda_context(source, argument.value, expected) {
                    PartialLambdaContext::Unresolved if unresolved_context => {
                        if let Some(constraints) = &constraints
                            && let Some(span) = constraints.explicit_lambda_header_mismatch(
                                self,
                                candidate_types.as_ref(),
                                source,
                                parameter_index,
                                argument.value,
                            )?
                        {
                            self.emit_lambda_structure_mismatch(span, expected_span)?;
                            valid = false;
                        }
                        uncommitted_context = true;
                        continue;
                    }
                    PartialLambdaContext::Poisoned => {
                        valid = false;
                        uncommitted_context = true;
                        continue;
                    }
                    PartialLambdaContext::NonCallable => {
                        let span = self
                            .file(source)
                            .ast()
                            .expressions()
                            .get(argument.value)
                            .map_err(TypeCheckingError::from)?
                            .span();
                        self.emit_maybe_label(
                            codes::TYPE_MISMATCH,
                            "lambda argument requires a function parameter type",
                            span,
                            expected_span,
                            "parameter type declared here",
                        )?;
                        valid = false;
                        uncommitted_context = true;
                        continue;
                    }
                    PartialLambdaContext::WrongHeader(_) | PartialLambdaContext::Unresolved => {}
                }
            }
            let actual = match prechecked_argument_types
                .and_then(|types| types.get(argument_index))
                .copied()
                .flatten()
            {
                Some(actual) => actual,
                None => {
                    let context = if unresolved_context && !lambda {
                        let reason =
                            if matches!(expected.kind(self), Some(UnitTypeKind::Deferred(_))) {
                                DeferredReason::ContextualArgument
                            } else {
                                DeferredReason::Call
                            };
                        ExpressionExpected::Canonical(self.deferred_type(reason))
                    } else {
                        expected
                    };
                    self.check_loop_reachable(arguments_exit, |checker| {
                        checker.check_expression_with_expectation(
                            source,
                            argument.value,
                            Some(context),
                            expected_span,
                            return_type,
                        )
                    })?
                    .ty
                }
            };
            if track_arguments && arguments_exit {
                arguments_exit = self.checked_call_operand_exits(source, argument.value)?;
            }
            if let Some(observed) = &mut observed {
                observed[argument_index] = Some(actual);
            }
            let required = self
                .contextual_argument_requirement(source, argument.value)?
                .unwrap_or(actual);
            if self.is_deferred(required) && !self.has_contextual_integer(required) {
                deferred = true;
                if let Some(marker @ ParameterModeMarker::Inout(_)) = argument.mode_marker
                    && self.is_known_read_only_index(source, argument.value)
                {
                    self.emit_mapping_error(MappingError::Mode {
                        primary: parameter_mode_span(marker),
                        parameter: parameter.span,
                    })?;
                    valid = false;
                }
            } else if self.is_error(actual) {
                valid = false;
            } else {
                let unresolved_actual = self.contains_unresolved_call_type(actual);
                deferred |= unresolved_actual;
                // The source facts already exist. Any remaining use of the
                // expected type is local to compatibility or the diagnostic.
                valid &= expected.probe(self, |checker, expected| {
                    let assignable = checker.call_argument_could_match(
                        required,
                        expected,
                        pending_instance || unresolved_actual,
                    );
                    if !assignable {
                        checker.emit_call_argument_mismatch(
                            source,
                            argument,
                            parameter.span,
                            expected,
                            actual,
                        )?;
                    }
                    Ok(checker.validate_inout_argument(source, argument, parameter)? && assignable)
                })?;
            }
        }
        if valid
            && !self.has_argument_errors_since(argument_start)
            && let (Some(constraints), Some(observed)) = (&constraints, &observed)
            && let Some(index) = constraints.first_conflict(
                self,
                candidate_types.as_ref(),
                source,
                arguments,
                mapping,
                observed,
            )?
        {
            let span = self
                .file(source)
                .ast()
                .expressions()
                .get(arguments[index].value)
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit_maybe_label(
                codes::TYPE_MISMATCH,
                "argument conflicts with the pending type constraints",
                span,
                candidate.parameters[mapping[index]].span,
                "parameter type declared here",
            )?;
            valid = false;
        }
        if !valid || self.has_argument_errors_since(argument_start) {
            if uncommitted_context {
                // Final admission needs the unresolved call root to identify
                // the uncommitted lambda. Independent errors are already recorded.
                return Ok(self.rejected_call_recovery(source, expression));
            }
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        let callee_payload = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?
            .payload();
        let callee_id = UnitExpressionId::new(source, callee);
        let pending_receiver = match callee_payload {
            Expression::Member { receiver, .. } => {
                let receiver = UnitExpressionId::new(source, *receiver);
                self.storage_value_is_pending(receiver)
                    || self
                        .parts
                        .expression_types
                        .get(&receiver)
                        .is_some_and(|&ty| {
                            self.replacement_value_pending(receiver, ty, self.deferred_facts.len())
                        })
            }
            _ => false,
        };
        let call_pending = deferred
            || pending_instance
            || pending_receiver
            || self.storage_value_is_pending(callee_id)
            || self.has_unresolved_argument_facts_since(argument_start.deferred_facts);
        if let Expression::SuperMember { name_span, .. } = callee_payload
            && candidate.receiver.is_some_and(|(target, _)| {
                self.current_receiver_mode
                    .is_some_and(|current| !receiver_mode_allows(current, target))
            })
        {
            if call_pending {
                return Ok(self.rejected_call_recovery(source, expression));
            }
            self.emit(
                codes::INVALID_OVERRIDE,
                "current receiver mode cannot satisfy the selected super member contract",
                *name_span,
            )?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        // Selection identifies the recovery target even when publication is
        // gated by a provisional argument type or value.
        let closure_call = candidate.target == UnitCallTarget::FunctionValue;
        let returns = !candidate.aborts
            && !self.is_builtin(candidate.return_type, BuiltinType::Nothing)
            && (!closure_call || self.known_closure_returns(callee_id));
        if track_arguments && !returns {
            self.note_pending_exit(
                PendingControlSite::Expression(UnitExpressionId::new(source, expression)),
                false,
            );
        }
        if arguments_exit && returns {
            if closure_call {
                self.apply_closure_effects(callee_id);
            } else {
                self.apply_pending_call_writebacks(source, candidate.target, arguments, mapping);
            }
        }
        if call_pending {
            return Ok(self.unresolved_call());
        }
        if let Some(types) = candidate_types {
            types.publish(
                self.signatures.types_mut(),
                candidate
                    .parameters
                    .iter_mut()
                    .map(|parameter| &mut parameter.ty)
                    .chain(std::iter::once(&mut candidate.return_type))
                    .chain(candidate.instance_arguments.iter_mut())
                    .chain(candidate.receiver.iter_mut().map(|(_, ty)| ty)),
            )?;
        }
        self.record_call(source, expression, callee, arguments, &candidate, mapping);
        Ok(ExpressionCheck {
            ty: candidate.return_type,
            falls_through: !self.is_builtin(candidate.return_type, BuiltinType::Nothing),
        })
    }

    fn has_argument_errors_since(&self, start: ArgumentCheckStart) -> bool {
        self.diagnostics[start.diagnostics..]
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error)
    }

    fn emit_call_argument_mismatch(
        &mut self,
        source: SourceUnitId,
        argument: &CallArgument,
        parameter_span: Option<Span>,
        expected: UnitTypeId,
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
            parameter_span,
            format!(
                "expected {}, found {}",
                self.type_name(expected),
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

    fn record_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        candidate: &CallCandidate,
        mapping: &[usize],
    ) {
        let receiver = candidate.receiver.and_then(|(mode, ty)| {
            let payload = self
                .file(source)
                .ast()
                .expressions()
                .get(callee)
                .expect("validated callee expression identity")
                .payload();
            let (origin, category) = match payload {
                Expression::Member { receiver, .. } => (
                    UnitCallReceiverOrigin::Expression(UnitExpressionId::new(source, *receiver)),
                    self.expression_category(source, *receiver),
                ),
                _ => (
                    UnitCallReceiverOrigin::ImplicitThis(self.current_owner?),
                    crate::type_checking::ExpressionCategory::Place,
                ),
            };
            Some(UnitCallReceiverDescriptor {
                origin,
                mode,
                category,
                ty,
            })
        });
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
            text_primitive: None,
            expression: UnitExpressionId::new(source, expression),
            instance: UnitCallableInstanceKey {
                target: candidate.target,
                type_arguments: candidate.instance_arguments.clone(),
            },
            return_type: candidate.return_type,
            receiver,
            result_source: candidate.result_source,
            range_construction: None,
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

const fn receiver_mode_allows(current: ParameterMode, target: ParameterMode) -> bool {
    matches!(
        (current, target),
        (ParameterMode::Borrow, ParameterMode::Borrow)
            | (
                ParameterMode::Inout,
                ParameterMode::Borrow | ParameterMode::Inout
            )
            | (
                ParameterMode::Value,
                ParameterMode::Borrow | ParameterMode::Value
            )
    )
}
