//! SPEC-0197 compilation-unit source callable 的泛型实例化与不变结构推导。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::TypeRefId,
    diagnostic::codes,
    name_resolution::{SourceUnitId, UnitSymbolId},
    parser::CallArgument,
    source::Span,
    type_checking::{
        Capability, CompilationUnitTypeError, Copyability, DeferredReason, UnitTypeId,
        UnitTypeKind, UnitTypeParameterBound,
    },
};

use super::super::BodyChecker;
use super::super::pending_constraints::PendingConstraints;
use super::super::type_graph::substitute;
use super::{super::copyability::UnitTransferability, CallCandidate};

#[derive(Clone, Copy)]
pub(super) enum UnitInstantiationFailure {
    Poisoned,
    Arity {
        primary: Span,
        parameter: Option<Span>,
    },
    Inference {
        primary: Span,
        parameter: UnitSymbolId,
    },
    Bound {
        kind: UnitBoundFailureKind,
        primary: Span,
        parameter: UnitSymbolId,
    },
    Storage {
        primary: Span,
        declaration: Option<Span>,
        message: &'static str,
    },
}

pub(super) struct InstantiatedCandidate {
    pub(super) call: CallCandidate,
    pub(super) pending: bool,
    pub(super) constraints: Option<PendingConstraints>,
}

#[derive(Clone, Copy)]
pub(super) enum UnitBoundFailureKind {
    RangeCarrier,
    Interface,
    Copyable,
    Transferable,
    Hashable,
}

impl BodyChecker<'_> {
    pub(super) fn check_generic_inference_arguments(
        &mut self,
        source: SourceUnitId,
        arguments: &[CallArgument],
        expected: Option<UnitTypeId>,
        return_type: UnitTypeId,
    ) -> Result<Vec<Option<UnitTypeId>>, CompilationUnitTypeError> {
        let track_arguments = self.tracks_pending_call_flow();
        let mut arguments_exit = true;
        arguments
            .iter()
            .map(|argument| {
                if self.is_lambda_syntax(source, argument.value) {
                    Ok(None)
                } else {
                    let result = self.check_loop_reachable(arguments_exit, |checker| {
                        checker.check_expression(
                            source,
                            argument.value,
                            expected,
                            None,
                            return_type,
                        )
                    })?;
                    if track_arguments && arguments_exit {
                        arguments_exit =
                            self.has_pending_expression_exit(source, argument.value, result)?;
                    }
                    Ok(Some(result.ty))
                }
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn instantiate_source_candidate(
        &mut self,
        source: SourceUnitId,
        candidate: CallCandidate,
        mapping: &[usize],
        arguments: &[CallArgument],
        inference_types: &[Option<UnitTypeId>],
        explicit: (&[TypeRefId], &[UnitTypeId]),
        callee_span: Span,
    ) -> Result<Result<InstantiatedCandidate, UnitInstantiationFailure>, CompilationUnitTypeError>
    {
        // The receiver and inference inputs are already committed. Only candidate-local
        // interning can occur here, so a failed candidate can discard the type-table tail.
        let checkpoint = self.signatures.types().len();
        let result = self.try_instantiate_source_candidate(
            source,
            candidate,
            mapping,
            arguments,
            inference_types,
            explicit,
            callee_span,
        );
        if !matches!(&result, Ok(Ok(_))) {
            self.signatures.types_mut().discard_after(checkpoint);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn try_instantiate_source_candidate(
        &mut self,
        source: SourceUnitId,
        mut candidate: CallCandidate,
        mapping: &[usize],
        arguments: &[CallArgument],
        inference_types: &[Option<UnitTypeId>],
        explicit: (&[TypeRefId], &[UnitTypeId]),
        callee_span: Span,
    ) -> Result<Result<InstantiatedCandidate, UnitInstantiationFailure>, CompilationUnitTypeError>
    {
        let owner = self.specialize_member_owner(&mut candidate)?;
        let owner_formals = owner
            .values()
            .any(|&ty| self.contains_unresolved_call_type(ty))
            .then(|| {
                candidate
                    .parameters
                    .iter()
                    .map(|parameter| parameter.ty)
                    .collect()
            });
        if !owner.is_empty()
            && let Some(message) = self.substitute_candidate_types(&mut candidate, &owner)?
        {
            return Ok(Err(UnitInstantiationFailure::Storage {
                primary: callee_span,
                declaration: candidate.declaration_span,
                message,
            }));
        }
        let (type_refs, explicit_types) = explicit;
        let parameters = candidate.type_parameters.clone();
        let parameter_set = parameters.iter().copied().collect::<BTreeSet<_>>();
        let mut substitutions = owner;
        let mut origins = BTreeMap::new();
        let mut pending_parameters = BTreeSet::new();
        let mut pending_templates = Vec::new();

        if !explicit_types.is_empty() {
            if explicit_types.len() != parameters.len() {
                let primary = if explicit_types.len() > parameters.len() {
                    self.file(source)
                        .ast()
                        .type_refs()
                        .get(type_refs[parameters.len()])
                        .map_err(crate::type_checking::TypeCheckingError::from)?
                        .span()
                } else {
                    callee_span
                };
                let parameter = parameters
                    .get(explicit_types.len())
                    .map(|parameter| self.unit_symbol_span(*parameter))
                    .transpose()?;
                return Ok(Err(UnitInstantiationFailure::Arity { primary, parameter }));
            }
            for ((&parameter, &actual), &type_ref) in
                parameters.iter().zip(explicit_types).zip(type_refs)
            {
                if self.is_error(actual) {
                    return Ok(Err(UnitInstantiationFailure::Poisoned));
                }
                substitutions.insert(parameter, actual);
                origins.insert(
                    parameter,
                    self.file(source)
                        .ast()
                        .type_refs()
                        .get(type_ref)
                        .map_err(crate::type_checking::TypeCheckingError::from)?
                        .span(),
                );
            }
        }
        if let Some((template, actual)) = self.range_extension_receiver_types(&candidate)
            && let Err(parameter) = self.infer_unit_type_arguments_tracking(
                template,
                actual,
                &parameter_set,
                &mut substitutions,
                &mut origins,
                callee_span,
                &mut Some(&mut pending_templates),
            )
        {
            return Ok(Err(UnitInstantiationFailure::Inference {
                primary: callee_span,
                parameter,
            }));
        }
        if explicit_types.is_empty() && !parameters.is_empty() {
            for (argument_index, &parameter_index) in mapping.iter().enumerate() {
                let Some(actual) = inference_types[argument_index] else {
                    continue;
                };
                if self.is_error(actual) {
                    return Ok(Err(UnitInstantiationFailure::Poisoned));
                }
                let template = candidate.parameters[parameter_index].ty;
                let span = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(arguments[argument_index].value)
                    .map_err(crate::type_checking::TypeCheckingError::from)?
                    .span();
                if let Err(parameter) = self.infer_unit_type_arguments_tracking(
                    template,
                    actual,
                    &parameter_set,
                    &mut substitutions,
                    &mut origins,
                    span,
                    &mut Some(&mut pending_templates),
                ) {
                    return Ok(Err(UnitInstantiationFailure::Inference {
                        primary: span,
                        parameter,
                    }));
                }
            }
            for &parameter in parameters.iter().filter(|p| !substitutions.contains_key(p)) {
                let requested = BTreeSet::from([parameter]);
                if pending_templates.iter().any(|&template| {
                    self.first_contained_unit_parameter(template, &requested, &mut BTreeSet::new())
                        .is_some()
                }) {
                    pending_parameters.insert(parameter);
                } else {
                    return Ok(Err(UnitInstantiationFailure::Inference {
                        primary: callee_span,
                        parameter,
                    }));
                }
            }
        }

        if parameters.is_empty() && !explicit_types.is_empty() {
            let primary = self
                .file(source)
                .ast()
                .type_refs()
                .get(type_refs[0])
                .map_err(crate::type_checking::TypeCheckingError::from)?
                .span();
            return Ok(Err(UnitInstantiationFailure::Arity {
                primary,
                parameter: None,
            }));
        }

        let mut pending = !pending_parameters.is_empty();
        if pending {
            let unresolved = self.deferred_type(DeferredReason::Call);
            substitutions.extend(
                pending_parameters
                    .iter()
                    .map(|&parameter| (parameter, unresolved)),
            );
        }
        for &parameter in &parameters {
            let actual = substitutions[&parameter];
            let unresolved = self.contains_unresolved_call_type(actual);
            pending |= unresolved;
            if self.is_unresolved_call_type(actual) {
                // A known layout restriction such as Any is handled below.
                // An unresolved input cannot prove this parameter's bound.
                pending = true;
                continue;
            }
            let primary = origins.get(&parameter).copied().unwrap_or(callee_span);
            if crate::type_checking::range_type_uses::unit_contains_range(
                self.signatures.types(),
                actual,
            ) {
                return Ok(Err(UnitInstantiationFailure::Bound {
                    kind: UnitBoundFailureKind::RangeCarrier,
                    primary,
                    parameter,
                }));
            }
            let bound = self
                .signatures
                .type_parameter(parameter)
                .map(|descriptor| descriptor.bound())
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            let kind = match bound {
                UnitTypeParameterBound::Any | UnitTypeParameterBound::Error => None,
                UnitTypeParameterBound::Interface(interface) => {
                    let mut bound_types = BTreeMap::new();
                    let expected = substitute(
                        self.signatures.types_mut(),
                        interface,
                        &substitutions,
                        &mut bound_types,
                    )?;
                    if let Some(message) = self.invalid_storage_in_substitution(&bound_types) {
                        return Ok(Err(UnitInstantiationFailure::Storage {
                            primary: callee_span,
                            declaration: candidate.declaration_span,
                            message,
                        }));
                    }
                    if self.satisfies_interface(actual, expected)? {
                        None
                    } else if self.could_satisfy_pending_interface(actual, expected)? {
                        pending = true;
                        None
                    } else {
                        Some(UnitBoundFailureKind::Interface)
                    }
                }
                UnitTypeParameterBound::Capability(Capability::Copyable) => {
                    match self.copyability_of(actual) {
                        Copyability::Copyable => None,
                        Copyability::Unknown => {
                            // An unresolved nominal field need not appear in its type arguments.
                            pending = true;
                            None
                        }
                        _ => Some(UnitBoundFailureKind::Copyable),
                    }
                }
                UnitTypeParameterBound::Capability(Capability::Transferable) => {
                    match self.transferability_of(actual) {
                        UnitTransferability::Transferable => None,
                        UnitTransferability::Unknown => {
                            pending = true;
                            None
                        }
                        _ => Some(UnitBoundFailureKind::Transferable),
                    }
                }
                UnitTypeParameterBound::Capability(Capability::Hashable) => {
                    (!self.is_hashable_type(actual)).then_some(UnitBoundFailureKind::Hashable)
                }
            };
            if let Some(kind) = kind {
                return Ok(Err(UnitInstantiationFailure::Bound {
                    kind,
                    primary,
                    parameter,
                }));
            }
        }

        let constraints = if pending || owner_formals.is_some() {
            let explicit = parameters.iter().copied().zip(type_refs.iter().copied());
            PendingConstraints::new(self, &substitutions, explicit, || {
                owner_formals.unwrap_or_else(|| {
                    candidate
                        .parameters
                        .iter()
                        .map(|parameter| parameter.ty)
                        .collect()
                })
            })
        } else {
            None
        };
        if !substitutions.is_empty()
            && let Some(message) =
                self.substitute_candidate_types(&mut candidate, &substitutions)?
        {
            return Ok(Err(UnitInstantiationFailure::Storage {
                primary: callee_span,
                declaration: candidate.declaration_span,
                message,
            }));
        }
        candidate
            .instance_arguments
            .extend(parameters.iter().map(|parameter| substitutions[parameter]));
        candidate.type_parameters.clear();
        pending |= candidate
            .instance_arguments
            .iter()
            .copied()
            .chain(candidate.parameters.iter().map(|parameter| parameter.ty))
            .chain(candidate.receiver.iter().map(|(_, ty)| *ty))
            .chain(std::iter::once(candidate.return_type))
            .any(|ty| self.contains_unresolved_call_type(ty));
        Ok(Ok(InstantiatedCandidate {
            call: candidate,
            pending,
            constraints,
        }))
    }

    pub(super) fn substitute_candidate_types(
        &mut self,
        candidate: &mut CallCandidate,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    ) -> Result<Option<&'static str>, CompilationUnitTypeError> {
        let mut instantiated = BTreeMap::new();
        let parameter_types = candidate
            .parameters
            .iter()
            .map(|parameter| {
                substitute(
                    self.signatures.types_mut(),
                    parameter.ty,
                    substitutions,
                    &mut instantiated,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let return_type = substitute(
            self.signatures.types_mut(),
            candidate.return_type,
            substitutions,
            &mut instantiated,
        )?;
        if let Some(message) = self.invalid_storage_in_substitution(&instantiated) {
            return Ok(Some(message));
        }
        for (parameter, ty) in candidate.parameters.iter_mut().zip(parameter_types) {
            parameter.ty = ty;
        }
        candidate.return_type = return_type;
        Ok(None)
    }

    pub(super) fn emit_unit_instantiation_failure(
        &mut self,
        failure: UnitInstantiationFailure,
    ) -> Result<(), CompilationUnitTypeError> {
        match failure {
            UnitInstantiationFailure::Poisoned => Ok(()),
            UnitInstantiationFailure::Storage {
                primary,
                declaration,
                message,
            } => self.emit_maybe_label(
                codes::INVALID_CONTAINER_ELEMENT,
                message,
                primary,
                declaration,
                "generic callable declared here",
            ),
            UnitInstantiationFailure::Arity { primary, parameter } => {
                if let Some(parameter) = parameter {
                    self.emit_maybe_label(
                        codes::TYPE_ARGUMENT_ARITY,
                        "callable type argument count does not match its declaration",
                        primary,
                        Some(parameter),
                        "unfilled callable type parameter declared here",
                    )
                } else {
                    self.emit(
                        codes::TYPE_ARGUMENT_ARITY,
                        "callable does not accept these type arguments",
                        primary,
                    )
                }
            }
            UnitInstantiationFailure::Inference { primary, parameter } => self.emit_maybe_label(
                codes::GENERIC_CALL_INFERENCE,
                "callable type arguments cannot be inferred completely and consistently",
                primary,
                Some(self.unit_symbol_span(parameter)?),
                "unresolved or conflicting type parameter declared here",
            ),
            UnitInstantiationFailure::Bound {
                kind,
                primary,
                parameter,
            } => {
                let (code, message) = match kind {
                    UnitBoundFailureKind::RangeCarrier => (
                        codes::BORROW_RESULT_ESCAPE,
                        "range carrier cannot be a generic type argument",
                    ),
                    UnitBoundFailureKind::Interface => (
                        codes::TYPE_ARGUMENT_BOUND,
                        "type argument does not satisfy its interface bound",
                    ),
                    UnitBoundFailureKind::Copyable => (
                        codes::COPYABLE_TYPE_ARGUMENT_BOUND,
                        "type argument does not satisfy its Copyable bound",
                    ),
                    UnitBoundFailureKind::Transferable => (
                        codes::TRANSFERABLE_TYPE_ARGUMENT_BOUND,
                        "type argument does not satisfy its Transferable bound",
                    ),
                    UnitBoundFailureKind::Hashable => (
                        codes::HASHABLE_TYPE_ARGUMENT_BOUND,
                        "type argument does not satisfy its Hashable bound",
                    ),
                };
                self.emit_maybe_label(
                    code,
                    message,
                    primary,
                    Some(self.unit_symbol_span(parameter)?),
                    "type parameter bound declared here",
                )
            }
        }
    }

    pub(in crate::type_checking::compilation_unit::bodies::checker) fn infer_unit_type_arguments(
        &self,
        template: UnitTypeId,
        actual: UnitTypeId,
        parameters: &BTreeSet<UnitSymbolId>,
        substitutions: &mut BTreeMap<UnitSymbolId, UnitTypeId>,
        origins: &mut BTreeMap<UnitSymbolId, Span>,
        origin: Span,
    ) -> Result<(), UnitSymbolId> {
        self.infer_unit_type_arguments_tracking(
            template,
            actual,
            parameters,
            substitutions,
            origins,
            origin,
            &mut None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_unit_type_arguments_tracking(
        &self,
        template: UnitTypeId,
        actual: UnitTypeId,
        parameters: &BTreeSet<UnitSymbolId>,
        substitutions: &mut BTreeMap<UnitSymbolId, UnitTypeId>,
        origins: &mut BTreeMap<UnitSymbolId, Span>,
        origin: Span,
        pending: &mut Option<&mut Vec<UnitTypeId>>,
    ) -> Result<(), UnitSymbolId> {
        if let Some(pending) = pending.as_deref_mut()
            && (self.is_unresolved_call_type(actual)
                || matches!(
                    self.signatures.types().get(template),
                    Some(UnitTypeKind::TypeParameter(parameter)) if parameters.contains(parameter)
                ) && self.contains_unresolved_call_type(actual))
        {
            if let Some(UnitTypeKind::TypeParameter(parameter)) =
                self.signatures.types().get(template)
                && parameters.contains(parameter)
                && let Some(&previous) = substitutions.get(parameter)
                && !self.types_could_be_equal(previous, actual)
            {
                return Err(*parameter);
            }
            pending.push(template);
            return Ok(());
        }
        if let Some(UnitTypeKind::TypeParameter(parameter)) = self.signatures.types().get(template)
            && parameters.contains(parameter)
        {
            if self.is_deferred(actual) {
                return Ok(());
            }
            return match substitutions.get(parameter).copied() {
                Some(previous)
                    if previous != actual
                        && (pending.is_none() || !self.types_could_be_equal(previous, actual)) =>
                {
                    Err(*parameter)
                }
                Some(_) => Ok(()),
                None => {
                    substitutions.insert(*parameter, actual);
                    origins.insert(*parameter, origin);
                    Ok(())
                }
            };
        }
        if self
            .first_contained_unit_parameter(template, parameters, &mut BTreeSet::new())
            .is_none()
        {
            return Ok(());
        }
        match (
            self.signatures.types().get(template),
            self.signatures.types().get(actual),
        ) {
            (Some(UnitTypeKind::Nullable(template)), Some(UnitTypeKind::Nullable(actual)))
            | (Some(UnitTypeKind::StaticSelf(template)), Some(UnitTypeKind::StaticSelf(actual))) => {
                self.infer_unit_type_arguments_tracking(
                    *template,
                    *actual,
                    parameters,
                    substitutions,
                    origins,
                    origin,
                    pending,
                )
            }
            (
                Some(UnitTypeKind::Function {
                    move_only: template_move,
                    parameters: template_parameters,
                    return_type: template_return,
                }),
                Some(UnitTypeKind::Function {
                    move_only: actual_move,
                    parameters: actual_parameters,
                    return_type: actual_return,
                }),
            ) if template_move == actual_move
                && template_parameters.len() == actual_parameters.len()
                && template_parameters
                    .iter()
                    .zip(actual_parameters)
                    .all(|(template, actual)| template.mode() == actual.mode()) =>
            {
                for (template, actual) in template_parameters.iter().zip(actual_parameters) {
                    self.infer_unit_type_arguments_tracking(
                        template.ty(),
                        actual.ty(),
                        parameters,
                        substitutions,
                        origins,
                        origin,
                        pending,
                    )?;
                }
                self.infer_unit_type_arguments_tracking(
                    *template_return,
                    *actual_return,
                    parameters,
                    substitutions,
                    origins,
                    origin,
                    pending,
                )
            }
            (
                Some(UnitTypeKind::Nominal {
                    declaration: template_declaration,
                    arguments: template_arguments,
                }),
                Some(UnitTypeKind::Nominal {
                    declaration: actual_declaration,
                    arguments: actual_arguments,
                }),
            ) if template_declaration == actual_declaration
                && template_arguments.len() == actual_arguments.len() =>
            {
                self.infer_unit_argument_lists(
                    template_arguments,
                    actual_arguments,
                    parameters,
                    substitutions,
                    origins,
                    origin,
                    pending,
                )
            }
            (
                Some(UnitTypeKind::Intrinsic {
                    constructor: template_constructor,
                    arguments: template_arguments,
                }),
                Some(UnitTypeKind::Intrinsic {
                    constructor: actual_constructor,
                    arguments: actual_arguments,
                }),
            ) if template_constructor == actual_constructor
                && template_arguments.len() == actual_arguments.len() =>
            {
                self.infer_unit_argument_lists(
                    template_arguments,
                    actual_arguments,
                    parameters,
                    substitutions,
                    origins,
                    origin,
                    pending,
                )
            }
            _ => Err(self
                .first_contained_unit_parameter(template, parameters, &mut BTreeSet::new())
                .expect("a recursive inference path contains a requested type parameter")),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_unit_argument_lists(
        &self,
        templates: &[UnitTypeId],
        actuals: &[UnitTypeId],
        parameters: &BTreeSet<UnitSymbolId>,
        substitutions: &mut BTreeMap<UnitSymbolId, UnitTypeId>,
        origins: &mut BTreeMap<UnitSymbolId, Span>,
        origin: Span,
        pending: &mut Option<&mut Vec<UnitTypeId>>,
    ) -> Result<(), UnitSymbolId> {
        for (&template, &actual) in templates.iter().zip(actuals) {
            self.infer_unit_type_arguments_tracking(
                template,
                actual,
                parameters,
                substitutions,
                origins,
                origin,
                pending,
            )?;
        }
        Ok(())
    }

    fn first_contained_unit_parameter(
        &self,
        ty: UnitTypeId,
        parameters: &BTreeSet<UnitSymbolId>,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> Option<UnitSymbolId> {
        if !active.insert(ty) {
            return None;
        }
        let result = match self.signatures.types().get(ty) {
            Some(UnitTypeKind::TypeParameter(parameter)) if parameters.contains(parameter) => {
                Some(*parameter)
            }
            Some(UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner)) => {
                self.first_contained_unit_parameter(*inner, parameters, active)
            }
            Some(UnitTypeKind::Function {
                parameters: function_parameters,
                return_type,
                ..
            }) => function_parameters
                .iter()
                .find_map(|parameter| {
                    self.first_contained_unit_parameter(parameter.ty(), parameters, active)
                })
                .or_else(|| self.first_contained_unit_parameter(*return_type, parameters, active)),
            Some(
                UnitTypeKind::Nominal { arguments, .. } | UnitTypeKind::Intrinsic { arguments, .. },
            ) => arguments.iter().find_map(|argument| {
                self.first_contained_unit_parameter(*argument, parameters, active)
            }),
            Some(UnitTypeKind::EnumCase { root, .. }) => {
                self.first_contained_unit_parameter(*root, parameters, active)
            }
            _ => None,
        };
        active.remove(&ty);
        result
    }
}
