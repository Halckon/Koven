use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::SymbolId,
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        BuiltinType, CallableTarget, Capability, Copyability, DeferredReason,
        IntrinsicTypeConstructor, NominalId, NominalKind, TypeId, TypeKind, TypeParameterBound,
    },
};

use super::super::*;
use super::CallCandidate;

#[derive(Clone, Copy)]
pub(super) enum InstantiationFailure {
    Poisoned,
    Arity {
        primary: Span,
        parameter: Option<Span>,
    },
    Inference {
        primary: Span,
        parameter: SymbolId,
    },
    Bound {
        kind: BoundFailureKind,
        primary: Span,
        parameter: SymbolId,
    },
}

#[derive(Clone, Copy)]
pub(super) enum BoundFailureKind {
    Interface,
    Copyable,
    Transferable,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::type_checking::checker) enum Transferability {
    Transferable,
    NotTransferable,
    Unknown,
    Error,
}

impl Checker<'_> {
    pub(in crate::type_checking::checker) fn construction_bound_satisfied(
        &mut self,
        actual: TypeId,
        bound: TypeParameterBound,
        substitutions: &BTreeMap<SymbolId, TypeId>,
    ) -> Result<bool, TypeCheckingError> {
        Ok(match bound {
            TypeParameterBound::Any | TypeParameterBound::Error => true,
            TypeParameterBound::Interface(interface) => {
                let expected = self.substitute_type(interface, substitutions)?;
                self.satisfies_interface(actual, expected)?
            }
            TypeParameterBound::Capability(Capability::Copyable) => {
                self.copyability_of(actual) == Copyability::Copyable
            }
            TypeParameterBound::Capability(Capability::Transferable) => {
                self.transferability_of(actual) == Transferability::Transferable
            }
        })
    }

    pub(super) fn check_inference_arguments(
        &mut self,
        arguments: &[CallArgument],
    ) -> Result<Vec<Option<TypeId>>, TypeCheckingError> {
        arguments
            .iter()
            .map(|argument| {
                if self.is_lambda_literal(argument.value)? {
                    Ok(None)
                } else {
                    self.check_expression(argument.value, None, None)
                        .map(|result| Some(result.ty))
                }
            })
            .collect()
    }

    pub(super) fn instantiate_candidate(
        &mut self,
        mut candidate: CallCandidate,
        mapping: &[usize],
        arguments: &[CallArgument],
        inference_types: &[Option<TypeId>],
        explicit: (&[TypeRefId], &[TypeId]),
        callee_span: Span,
    ) -> Result<Result<CallCandidate, InstantiationFailure>, TypeCheckingError> {
        let (type_refs, explicit_types) = explicit;
        if !matches!(candidate.target, CallableTarget::Source(_)) {
            return Ok(Ok(candidate));
        }
        let parameters = candidate.type_parameters.clone();
        let parameter_set = parameters.iter().copied().collect::<BTreeSet<_>>();
        let mut substitutions = BTreeMap::new();
        let mut origins = BTreeMap::new();

        if !explicit_types.is_empty() {
            if explicit_types.len() != parameters.len() {
                let primary = if explicit_types.len() > parameters.len() {
                    self.ast()
                        .type_refs()
                        .get(type_refs[parameters.len()])?
                        .span()
                } else {
                    callee_span
                };
                let parameter = parameters
                    .get(explicit_types.len())
                    .map(|parameter| self.symbol_spans[parameter.index()]);
                return Ok(Err(InstantiationFailure::Arity { primary, parameter }));
            }
            for ((&parameter, &actual), &type_ref) in
                parameters.iter().zip(explicit_types).zip(type_refs)
            {
                if self.is_error(actual) {
                    return Ok(Err(InstantiationFailure::Poisoned));
                }
                substitutions.insert(parameter, actual);
                origins.insert(parameter, self.ast().type_refs().get(type_ref)?.span());
            }
        } else if !parameters.is_empty() {
            for (argument_index, &parameter_index) in mapping.iter().enumerate() {
                let Some(actual) = inference_types[argument_index] else {
                    continue;
                };
                if self.is_error(actual) {
                    return Ok(Err(InstantiationFailure::Poisoned));
                }
                let template = candidate.parameters[parameter_index].ty;
                let span = self
                    .ast()
                    .expressions()
                    .get(arguments[argument_index].value)?
                    .span();
                if let Err(parameter) = self.infer_type_arguments(
                    template,
                    actual,
                    &parameter_set,
                    &mut substitutions,
                    &mut origins,
                    span,
                ) {
                    return Ok(Err(InstantiationFailure::Inference {
                        primary: span,
                        parameter,
                    }));
                }
            }
            if let Some(&parameter) = parameters
                .iter()
                .find(|parameter| !substitutions.contains_key(parameter))
            {
                return Ok(Err(InstantiationFailure::Inference {
                    primary: callee_span,
                    parameter,
                }));
            }
        }

        if parameters.is_empty() && !explicit_types.is_empty() {
            return Ok(Err(InstantiationFailure::Arity {
                primary: self.ast().type_refs().get(type_refs[0])?.span(),
                parameter: None,
            }));
        }

        for &parameter in &parameters {
            let actual = substitutions[&parameter];
            let primary = origins.get(&parameter).copied().unwrap_or(callee_span);
            let bound = self
                .type_parameters
                .iter()
                .find(|descriptor| descriptor.symbol() == parameter)
                .map(|descriptor| descriptor.bound())
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            let kind = match bound {
                TypeParameterBound::Any | TypeParameterBound::Error => None,
                TypeParameterBound::Interface(interface) => {
                    let expected = self.substitute_type(interface, &substitutions)?;
                    (!self.satisfies_interface(actual, expected)?)
                        .then_some(BoundFailureKind::Interface)
                }
                TypeParameterBound::Capability(Capability::Copyable) => {
                    (self.copyability_of(actual) != Copyability::Copyable)
                        .then_some(BoundFailureKind::Copyable)
                }
                TypeParameterBound::Capability(Capability::Transferable) => {
                    (self.transferability_of(actual) != Transferability::Transferable)
                        .then_some(BoundFailureKind::Transferable)
                }
            };
            if let Some(kind) = kind {
                return Ok(Err(InstantiationFailure::Bound {
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

    pub(super) fn emit_instantiation_failure(
        &mut self,
        failure: InstantiationFailure,
    ) -> Result<(), TypeCheckingError> {
        match failure {
            InstantiationFailure::Poisoned => Ok(()),
            InstantiationFailure::Arity { primary, parameter } => {
                if let Some(parameter) = parameter {
                    self.emit_with_label(
                        self.type_argument_arity_code,
                        "callable type argument count does not match its declaration",
                        primary,
                        parameter,
                        "unfilled callable type parameter declared here",
                    )
                } else {
                    self.emit(
                        self.type_argument_arity_code,
                        "callable does not accept these type arguments",
                        primary,
                    )
                }
            }
            InstantiationFailure::Inference { primary, parameter } => self.emit_with_label(
                self.generic_call_inference_code,
                "callable type arguments cannot be inferred completely and consistently",
                primary,
                self.symbol_spans[parameter.index()],
                "unresolved or conflicting type parameter declared here",
            ),
            InstantiationFailure::Bound {
                kind,
                primary,
                parameter,
            } => {
                let (code, message) = match kind {
                    BoundFailureKind::Interface => (
                        self.type_argument_bound_code,
                        "type argument does not satisfy its interface bound",
                    ),
                    BoundFailureKind::Copyable => (
                        self.copyable_type_argument_bound_code,
                        "type argument does not satisfy its Copyable bound",
                    ),
                    BoundFailureKind::Transferable => (
                        self.transferable_type_argument_bound_code,
                        "type argument does not satisfy its Transferable bound",
                    ),
                };
                self.emit_with_label(
                    code,
                    message,
                    primary,
                    self.symbol_spans[parameter.index()],
                    "type parameter bound declared here",
                )
            }
        }
    }

    pub(in crate::type_checking::checker) fn is_lambda_literal(
        &self,
        expression: ExpressionId,
    ) -> Result<bool, TypeCheckingError> {
        let node = self.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Lambda { .. } => Ok(true),
            Expression::Group { expression } => self.is_lambda_literal(*expression),
            _ => Ok(false),
        }
    }

    pub(in crate::type_checking::checker) fn transferability_of(
        &self,
        ty: TypeId,
    ) -> Transferability {
        self.transferability_with(ty, &BTreeMap::new(), &mut BTreeSet::new())
    }

    fn transferability_with(
        &self,
        ty: TypeId,
        substitutions: &BTreeMap<SymbolId, TypeId>,
        active: &mut BTreeSet<NominalId>,
    ) -> Transferability {
        match self.kind(ty) {
            TypeKind::Builtin(
                BuiltinType::Byte
                | BuiltinType::Short
                | BuiltinType::Int
                | BuiltinType::Long
                | BuiltinType::UByte
                | BuiltinType::UShort
                | BuiltinType::UInt
                | BuiltinType::ULong
                | BuiltinType::Float
                | BuiltinType::Double
                | BuiltinType::Boolean
                | BuiltinType::Char
                | BuiltinType::String
                | BuiltinType::Unit
                | BuiltinType::Nothing,
            )
            | TypeKind::IntegerLiteral(_) => Transferability::Transferable,
            TypeKind::Builtin(BuiltinType::Any) | TypeKind::Function { .. } => {
                Transferability::NotTransferable
            }
            TypeKind::Nullable(inner) => self.transferability_with(*inner, substitutions, active),
            TypeKind::Nominal { nominal, arguments } => {
                self.nominal_transferability(*nominal, arguments, substitutions, active)
            }
            TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Rc,
                ..
            } => Transferability::NotTransferable,
            TypeKind::Intrinsic {
                constructor:
                    IntrinsicTypeConstructor::Box
                    | IntrinsicTypeConstructor::Array
                    | IntrinsicTypeConstructor::List
                    | IntrinsicTypeConstructor::MutableList,
                arguments,
            } => arguments
                .iter()
                .fold(Transferability::Transferable, |state, argument| {
                    combine_transferability(
                        state,
                        self.transferability_with(*argument, substitutions, active),
                    )
                }),
            TypeKind::EnumCase { root, .. } => {
                self.transferability_with(*root, substitutions, active)
            }
            TypeKind::TypeParameter(parameter) => {
                if let Some(actual) = substitutions.get(parameter).copied()
                    && actual != ty
                {
                    return self.transferability_with(actual, substitutions, active);
                }
                match self
                    .type_parameters
                    .iter()
                    .find(|descriptor| descriptor.symbol() == *parameter)
                    .map(|descriptor| descriptor.bound())
                {
                    Some(TypeParameterBound::Capability(Capability::Transferable)) => {
                        Transferability::Transferable
                    }
                    Some(TypeParameterBound::Error) => Transferability::Error,
                    Some(
                        TypeParameterBound::Any
                        | TypeParameterBound::Interface(_)
                        | TypeParameterBound::Capability(Capability::Copyable),
                    )
                    | None => Transferability::NotTransferable,
                }
            }
            TypeKind::Deferred(DeferredReason::AnyValueRepresentation) => {
                Transferability::NotTransferable
            }
            TypeKind::Deferred(_) => Transferability::Unknown,
            TypeKind::Error | TypeKind::StaticSelf(_) | TypeKind::Capability(_) => {
                Transferability::Error
            }
        }
    }

    fn nominal_transferability(
        &self,
        nominal: NominalId,
        arguments: &[TypeId],
        outer: &BTreeMap<SymbolId, TypeId>,
        active: &mut BTreeSet<NominalId>,
    ) -> Transferability {
        if !active.insert(nominal) {
            return Transferability::Transferable;
        }
        let Some(descriptor) = self
            .nominals
            .iter()
            .find(|descriptor| descriptor.id() == nominal)
            .cloned()
        else {
            active.remove(&nominal);
            return Transferability::Error;
        };
        let result = match descriptor.kind() {
            NominalKind::Object => Transferability::NotTransferable,
            NominalKind::Interface => Transferability::Error,
            NominalKind::ValueClass | NominalKind::Class | NominalKind::EnumClass => {
                let mut substitutions = outer.clone();
                substitutions.extend(
                    descriptor
                        .type_parameters()
                        .iter()
                        .copied()
                        .zip(arguments.iter().copied()),
                );
                let components = if descriptor.kind() == NominalKind::EnumClass {
                    self.enum_cases
                        .iter()
                        .filter(|case| case.root() == nominal)
                        .flat_map(|case| case.payloads().iter().map(|(_, ty)| *ty))
                        .collect::<Vec<_>>()
                } else {
                    let Some(fields) = descriptor
                        .fields()
                        .iter()
                        .map(|field| self.symbol_type(*field))
                        .collect::<Option<Vec<_>>>()
                    else {
                        active.remove(&nominal);
                        return Transferability::Error;
                    };
                    fields
                };
                components
                    .into_iter()
                    .fold(Transferability::Transferable, |state, component| {
                        combine_transferability(
                            state,
                            self.transferability_with(component, &substitutions, active),
                        )
                    })
            }
        };
        active.remove(&nominal);
        result
    }
}

fn combine_transferability(left: Transferability, right: Transferability) -> Transferability {
    match (left, right) {
        (Transferability::Error, _) | (_, Transferability::Error) => Transferability::Error,
        (Transferability::Unknown, _) | (_, Transferability::Unknown) => Transferability::Unknown,
        (Transferability::NotTransferable, _) | (_, Transferability::NotTransferable) => {
            Transferability::NotTransferable
        }
        (Transferability::Transferable, Transferability::Transferable) => {
            Transferability::Transferable
        }
    }
}
