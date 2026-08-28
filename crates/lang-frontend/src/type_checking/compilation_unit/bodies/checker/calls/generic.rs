//! SPEC-0197 compilation-unit source callable 的泛型实例化与不变结构推导。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::TypeRefId,
    diagnostic::codes,
    name_resolution::{SourceUnitId, UnitSymbolId},
    parser::CallArgument,
    source::Span,
    type_checking::{
        Capability, CompilationUnitTypeError, Copyability, UnitTypeId, UnitTypeKind,
        UnitTypeParameterBound,
    },
};

use super::super::BodyChecker;
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
}

#[derive(Clone, Copy)]
pub(super) enum UnitBoundFailureKind {
    Interface,
    Copyable,
    Transferable,
}

impl BodyChecker<'_> {
    pub(super) fn check_generic_inference_arguments(
        &mut self,
        source: SourceUnitId,
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<Vec<Option<UnitTypeId>>, CompilationUnitTypeError> {
        arguments
            .iter()
            .map(|argument| {
                if self.is_lambda_syntax(source, argument.value) {
                    Ok(None)
                } else {
                    self.check_expression(source, argument.value, None, None, return_type)
                        .map(|result| Some(result.ty))
                }
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn instantiate_source_candidate(
        &mut self,
        source: SourceUnitId,
        mut candidate: CallCandidate,
        mapping: &[usize],
        arguments: &[CallArgument],
        inference_types: &[Option<UnitTypeId>],
        explicit: (&[TypeRefId], &[UnitTypeId]),
        callee_span: Span,
    ) -> Result<Result<CallCandidate, UnitInstantiationFailure>, CompilationUnitTypeError> {
        let (type_refs, explicit_types) = explicit;
        let parameters = candidate.type_parameters.clone();
        let parameter_set = parameters.iter().copied().collect::<BTreeSet<_>>();
        let mut substitutions = candidate.owner_substitutions.clone();
        let mut origins = BTreeMap::new();

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
        } else if !parameters.is_empty() {
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
                if let Err(parameter) = self.infer_unit_type_arguments(
                    template,
                    actual,
                    &parameter_set,
                    &mut substitutions,
                    &mut origins,
                    span,
                ) {
                    return Ok(Err(UnitInstantiationFailure::Inference {
                        primary: span,
                        parameter,
                    }));
                }
            }
            if let Some(&parameter) = parameters
                .iter()
                .find(|parameter| !substitutions.contains_key(parameter))
            {
                return Ok(Err(UnitInstantiationFailure::Inference {
                    primary: callee_span,
                    parameter,
                }));
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

        for &parameter in &parameters {
            let actual = substitutions[&parameter];
            let primary = origins.get(&parameter).copied().unwrap_or(callee_span);
            let bound = self
                .signatures
                .type_parameter(parameter)
                .map(|descriptor| descriptor.bound())
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            let kind = match bound {
                UnitTypeParameterBound::Any | UnitTypeParameterBound::Error => None,
                UnitTypeParameterBound::Interface(interface) => {
                    let expected = self.substitute_type(interface, &substitutions)?;
                    (!self.satisfies_interface(actual, expected)?)
                        .then_some(UnitBoundFailureKind::Interface)
                }
                UnitTypeParameterBound::Capability(Capability::Copyable) => {
                    (self.copyability_of(actual) != Copyability::Copyable)
                        .then_some(UnitBoundFailureKind::Copyable)
                }
                UnitTypeParameterBound::Capability(Capability::Transferable) => {
                    (self.transferability_of(actual) != UnitTransferability::Transferable)
                        .then_some(UnitBoundFailureKind::Transferable)
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

        for parameter in &mut candidate.parameters {
            parameter.ty = self.substitute_type(parameter.ty, &substitutions)?;
        }
        candidate.return_type = self.substitute_type(candidate.return_type, &substitutions)?;
        candidate
            .instance_arguments
            .extend(parameters.iter().map(|parameter| substitutions[parameter]));
        candidate.type_parameters.clear();
        Ok(Ok(candidate))
    }

    pub(super) fn emit_unit_instantiation_failure(
        &mut self,
        failure: UnitInstantiationFailure,
    ) -> Result<(), CompilationUnitTypeError> {
        match failure {
            UnitInstantiationFailure::Poisoned => Ok(()),
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
        if let Some(UnitTypeKind::TypeParameter(parameter)) = self.signatures.types().get(template)
            && parameters.contains(parameter)
        {
            if self.is_deferred(actual) {
                return Ok(());
            }
            return match substitutions.get(parameter).copied() {
                Some(previous) if previous != actual => Err(*parameter),
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
                self.infer_unit_type_arguments(
                    *template,
                    *actual,
                    parameters,
                    substitutions,
                    origins,
                    origin,
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
                    self.infer_unit_type_arguments(
                        template.ty(),
                        actual.ty(),
                        parameters,
                        substitutions,
                        origins,
                        origin,
                    )?;
                }
                self.infer_unit_type_arguments(
                    *template_return,
                    *actual_return,
                    parameters,
                    substitutions,
                    origins,
                    origin,
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
                )
            }
            _ => Err(self
                .first_contained_unit_parameter(template, parameters, &mut BTreeSet::new())
                .expect("a recursive inference path contains a requested type parameter")),
        }
    }

    fn infer_unit_argument_lists(
        &self,
        templates: &[UnitTypeId],
        actuals: &[UnitTypeId],
        parameters: &BTreeSet<UnitSymbolId>,
        substitutions: &mut BTreeMap<UnitSymbolId, UnitTypeId>,
        origins: &mut BTreeMap<UnitSymbolId, Span>,
        origin: Span,
    ) -> Result<(), UnitSymbolId> {
        for (&template, &actual) in templates.iter().zip(actuals) {
            self.infer_unit_type_arguments(
                template,
                actual,
                parameters,
                substitutions,
                origins,
                origin,
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
