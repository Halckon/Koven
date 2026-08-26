//! v0.30 nominal、enum case、intrinsic `Box` 与 `Rc` 构造检查。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::{EnumCaseId, Namespace, ReferenceTarget, SymbolId},
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        Capability, ConstructionArgumentDescriptor, ConstructionDescriptor, ConstructionTarget,
        ExternalTypeBinding, IntrinsicTypeConstructor, NominalId, NominalKind, ParameterMode,
        TypeId, TypeKind, TypeParameterBound,
    },
};

use super::{Checker, ExprCheck, MappedParameter, TypeCheckingError};

#[derive(Clone, Copy)]
enum Target {
    Nominal(NominalId),
    EnumCase(EnumCaseId),
    Box,
    Rc,
    Invalid,
}

#[derive(Clone)]
struct Parameter {
    symbol: Option<SymbolId>,
    name: String,
    ty: TypeId,
    span: Option<Span>,
}

struct SourceShape {
    target: ConstructionTarget,
    root: NominalId,
    parameters: Vec<Parameter>,
    type_parameters: Vec<SymbolId>,
    result_template: TypeId,
}

struct ConstructionInstance {
    arguments: Vec<TypeId>,
    substitutions: BTreeMap<SymbolId, TypeId>,
}

impl Parameter {
    fn as_call_parameter(&self) -> MappedParameter<TypeId> {
        MappedParameter {
            name: Some(self.name.clone()),
            mode: ParameterMode::Value,
            ty: self.ty,
            span: self.span,
        }
    }
}

impl Checker<'_> {
    pub(super) fn check_construction_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        expected: Option<TypeId>,
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let Some(target) = self.construction_target(callee)? else {
            return Ok(None);
        };
        if matches!(target, Target::Invalid) {
            self.check_construction_operands(arguments)?;
            self.emit(
                self.invalid_construction_target_code,
                "this type identity cannot be constructed",
                self.ast().expressions().get(callee)?.span(),
            )?;
            return Ok(Some(self.failed_construction()));
        }
        if let Target::Box = target {
            return self
                .check_box_construction(expression, call_span, callee, type_arguments, arguments)
                .map(Some);
        }
        if let Target::Rc = target {
            return self
                .check_rc_construction(expression, call_span, callee, type_arguments, arguments)
                .map(Some);
        }

        let shape = self.source_construction_shape(target)?;
        if shape.parameters.is_empty() && matches!(target, Target::EnumCase(_)) {
            return Ok(None);
        }
        let call_parameters = shape
            .parameters
            .iter()
            .map(Parameter::as_call_parameter)
            .collect::<Vec<_>>();
        let mapping = match self.map_arguments(&call_parameters, arguments, call_span)? {
            Ok(mapping) => mapping,
            Err(error) => {
                self.emit_mapping_error(error)?;
                self.check_construction_operands(arguments)?;
                return Ok(Some(self.failed_construction()));
            }
        };
        let Some(instance) = self.infer_construction_instance(
            callee,
            type_arguments,
            arguments,
            &mapping,
            &shape.parameters,
            &shape.type_parameters,
            shape.root,
            expected,
        )?
        else {
            return Ok(Some(self.failed_construction()));
        };
        let result_type = self.substitute_type(shape.result_template, &instance.substitutions)?;
        self.finish_construction(
            expression,
            shape.target,
            instance.arguments,
            result_type,
            arguments,
            &mapping,
            &shape.parameters,
            &instance.substitutions,
        )
        .map(Some)
    }

    fn construction_target(
        &self,
        callee: ExpressionId,
    ) -> Result<Option<Target>, TypeCheckingError> {
        let node = self.ast().expressions().get(callee)?;
        match node.payload() {
            Expression::Member {
                name_span, safe, ..
            } if !safe => {
                return Ok(self
                    .reference(*name_span, Namespace::Value)
                    .and_then(|target| match target {
                        ReferenceTarget::Symbol(symbol) => self
                            .enum_case_by_value_symbol
                            .get(symbol)
                            .copied()
                            .map(Target::EnumCase),
                        _ => None,
                    }));
            }
            Expression::Name => {
                if let Some(ReferenceTarget::Symbol(symbol)) =
                    self.reference(node.span(), Namespace::Value)
                    && let Some(case) = self.enum_case_by_value_symbol.get(symbol)
                {
                    return Ok(Some(Target::EnumCase(*case)));
                }
            }
            _ => return Ok(None),
        }
        Ok(match self.reference(node.span(), Namespace::Type) {
            Some(ReferenceTarget::Symbol(symbol)) => {
                self.nominal_by_symbol.get(symbol).and_then(|id| {
                    self.nominals
                        .iter()
                        .find(|item| item.id() == *id)
                        .map(|item| {
                            if matches!(item.kind(), NominalKind::Class | NominalKind::ValueClass) {
                                Target::Nominal(*id)
                            } else {
                                Target::Invalid
                            }
                        })
                })
            }
            Some(ReferenceTarget::External(external)) => {
                match self.environment.binding(*external) {
                    Some(ExternalTypeBinding::Intrinsic(IntrinsicTypeConstructor::Box)) => {
                        Some(Target::Box)
                    }
                    Some(ExternalTypeBinding::Intrinsic(IntrinsicTypeConstructor::Rc)) => {
                        Some(Target::Rc)
                    }
                    Some(ExternalTypeBinding::Builtin(_) | ExternalTypeBinding::Capability(_)) => {
                        Some(Target::Invalid)
                    }
                    _ => None,
                }
            }
            _ => None,
        })
    }

    fn source_construction_shape(
        &mut self,
        target: Target,
    ) -> Result<SourceShape, TypeCheckingError> {
        match target {
            Target::Nominal(id) => {
                let nominal = self
                    .nominals
                    .iter()
                    .find(|item| item.id() == id)
                    .cloned()
                    .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                let parameters = nominal
                    .fields()
                    .iter()
                    .map(|&symbol| self.source_parameter(symbol, self.symbol_types[symbol.index()]))
                    .collect::<Result<Vec<_>, _>>()?;
                let arguments = nominal
                    .type_parameters()
                    .iter()
                    .map(|&symbol| self.types.intern(TypeKind::TypeParameter(symbol)))
                    .collect();
                let result = self.types.intern(TypeKind::Nominal {
                    nominal: id,
                    arguments,
                });
                Ok(SourceShape {
                    target: ConstructionTarget::Nominal(id),
                    root: id,
                    parameters,
                    type_parameters: nominal.type_parameters().to_vec(),
                    result_template: result,
                })
            }
            Target::EnumCase(id) => {
                let case = self
                    .enum_case(id)
                    .cloned()
                    .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                let root = self
                    .nominals
                    .iter()
                    .find(|item| item.id() == case.root())
                    .cloned()
                    .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                let parameters = case
                    .payloads()
                    .iter()
                    .map(|&(symbol, ty)| self.source_parameter(symbol, Some(ty)))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(SourceShape {
                    target: ConstructionTarget::EnumCase(id),
                    root: case.root(),
                    parameters,
                    type_parameters: root.type_parameters().to_vec(),
                    result_template: case.root_type(),
                })
            }
            Target::Box | Target::Rc | Target::Invalid => unreachable!("source target expected"),
        }
    }

    fn source_parameter(
        &self,
        symbol: SymbolId,
        ty: Option<TypeId>,
    ) -> Result<Parameter, TypeCheckingError> {
        let span = self.symbol_spans[symbol.index()];
        Ok(Parameter {
            symbol: Some(symbol),
            name: self.sources.slice(span)?.to_owned(),
            ty: ty.ok_or(TypeCheckingError::InvalidExternalBinding)?,
            span: Some(span),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_construction_instance(
        &mut self,
        callee: ExpressionId,
        type_refs: &[TypeRefId],
        arguments: &[CallArgument],
        mapping: &[usize],
        parameters: &[Parameter],
        type_parameters: &[SymbolId],
        root: NominalId,
        expected: Option<TypeId>,
    ) -> Result<Option<ConstructionInstance>, TypeCheckingError> {
        let callee_span = self.ast().expressions().get(callee)?.span();
        let mut substitutions = BTreeMap::new();
        let mut origins = BTreeMap::new();
        if !type_refs.is_empty() {
            if type_refs.len() != type_parameters.len() {
                self.emit(
                    self.type_argument_arity_code,
                    "constructor type argument count does not match its declaration",
                    callee_span,
                )?;
                self.check_construction_operands(arguments)?;
                return Ok(None);
            }
            for (&parameter, &type_ref) in type_parameters.iter().zip(type_refs) {
                substitutions.insert(parameter, self.resolve_type_ref(type_ref)?);
                origins.insert(parameter, self.ast().type_refs().get(type_ref)?.span());
            }
        } else if !type_parameters.is_empty() {
            let parameter_set = type_parameters.iter().copied().collect::<BTreeSet<_>>();
            for (argument_index, argument) in arguments.iter().enumerate() {
                if self.is_lambda_literal(argument.value)? {
                    continue;
                }
                let actual = self.check_expression(argument.value, None, None)?.ty;
                let span = self.ast().expressions().get(argument.value)?.span();
                if self
                    .infer_type_arguments(
                        parameters[mapping[argument_index]].ty,
                        actual,
                        &parameter_set,
                        &mut substitutions,
                        &mut origins,
                        span,
                    )
                    .is_err()
                {
                    return self.reject_construction_inference(callee_span, type_parameters);
                }
            }
            if !self.candidate_local_expected
                && let Some(expected) = expected
                && let TypeKind::Nominal { nominal, arguments } = self.kind(expected).clone()
                && nominal == root
                && arguments.len() == type_parameters.len()
                && arguments
                    .iter()
                    .all(|argument| self.is_complete_construction_expected(*argument))
            {
                for (&parameter, actual) in type_parameters.iter().zip(arguments) {
                    if let Some(previous) = substitutions.insert(parameter, actual)
                        && previous != actual
                    {
                        return self.reject_construction_inference(callee_span, type_parameters);
                    }
                    origins.entry(parameter).or_insert(callee_span);
                }
            }
            if type_parameters
                .iter()
                .any(|parameter| !substitutions.contains_key(parameter))
            {
                return self.reject_construction_inference(callee_span, type_parameters);
            }
        } else if !type_refs.is_empty() {
            self.emit(
                self.type_argument_arity_code,
                "constructor does not accept type arguments",
                callee_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(None);
        }
        for &parameter in type_parameters {
            let bound = self
                .type_parameters
                .iter()
                .find(|item| item.symbol() == parameter)
                .map(|item| item.bound())
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            if !self.construction_bound_satisfied(
                substitutions[&parameter],
                bound,
                &substitutions,
            )? {
                let (code, message) = match bound {
                    TypeParameterBound::Capability(Capability::Copyable) => (
                        self.copyable_type_argument_bound_code,
                        "constructor type argument does not satisfy its Copyable bound",
                    ),
                    TypeParameterBound::Capability(Capability::Transferable) => (
                        self.transferable_type_argument_bound_code,
                        "constructor type argument does not satisfy its Transferable bound",
                    ),
                    TypeParameterBound::Any
                    | TypeParameterBound::Interface(_)
                    | TypeParameterBound::Error => (
                        self.type_argument_bound_code,
                        "constructor type argument does not satisfy its interface bound",
                    ),
                };
                self.emit_with_label(
                    code,
                    message,
                    origins.get(&parameter).copied().unwrap_or(callee_span),
                    self.symbol_spans[parameter.index()],
                    "type parameter bound declared here",
                )?;
                return Ok(None);
            }
        }
        Ok(Some(ConstructionInstance {
            arguments: type_parameters
                .iter()
                .map(|parameter| substitutions[parameter])
                .collect(),
            substitutions,
        }))
    }

    fn reject_construction_inference<T>(
        &mut self,
        primary: Span,
        parameters: &[SymbolId],
    ) -> Result<Option<T>, TypeCheckingError> {
        let parameter = parameters
            .first()
            .copied()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        self.emit_with_label(
            self.construction_inference_code,
            "constructor type arguments cannot be inferred completely and consistently",
            primary,
            self.symbol_spans[parameter.index()],
            "unresolved or conflicting constructor type parameter declared here",
        )?;
        Ok(None)
    }

    fn is_complete_construction_expected(&self, ty: TypeId) -> bool {
        fn complete(checker: &Checker<'_>, ty: TypeId, active: &mut BTreeSet<TypeId>) -> bool {
            if !active.insert(ty) {
                return true;
            }
            let result = match checker.kind(ty) {
                TypeKind::Error
                | TypeKind::Deferred(_)
                | TypeKind::TypeParameter(_)
                | TypeKind::StaticSelf(_)
                | TypeKind::Capability(_) => false,
                TypeKind::Nullable(inner) => complete(checker, *inner, active),
                TypeKind::Function {
                    parameters,
                    return_type,
                    ..
                } => {
                    parameters
                        .iter()
                        .all(|parameter| complete(checker, parameter.ty, active))
                        && complete(checker, *return_type, active)
                }
                TypeKind::Nominal { arguments, .. } | TypeKind::Intrinsic { arguments, .. } => {
                    arguments
                        .iter()
                        .all(|argument| complete(checker, *argument, active))
                }
                TypeKind::EnumCase { root, .. } => complete(checker, *root, active),
                TypeKind::Builtin(_) | TypeKind::IntegerLiteral(_) => true,
            };
            active.remove(&ty);
            result
        }
        complete(self, ty, &mut BTreeSet::new())
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_construction(
        &mut self,
        expression: ExpressionId,
        target: ConstructionTarget,
        instance_arguments: Vec<TypeId>,
        result_type: TypeId,
        arguments: &[CallArgument],
        mapping: &[usize],
        parameters: &[Parameter],
        substitutions: &BTreeMap<SymbolId, TypeId>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let mut valid = true;
        let mut descriptors = Vec::with_capacity(arguments.len());
        for (evaluation_index, argument) in arguments.iter().enumerate() {
            let parameter_index = mapping[evaluation_index];
            let parameter = &parameters[parameter_index];
            let parameter_type = self.substitute_type(parameter.ty, substitutions)?;
            let already_checked = self.expression_types[argument.value.index()].is_some();
            let checked =
                self.check_expression(argument.value, Some(parameter_type), parameter.span)?;
            if already_checked
                && !self.is_error(checked.ty)
                && !self.is_deferred(checked.ty)
                && !self.assignable(checked.ty, parameter_type)
            {
                self.mismatch(
                    self.ast().expressions().get(argument.value)?.span(),
                    parameter.span,
                    checked.ty,
                    parameter_type,
                )?;
                valid = false;
            }
            valid &= !self.is_error(checked.ty) && !self.is_deferred(checked.ty);
            descriptors.push((
                parameter_index,
                ConstructionArgumentDescriptor::new(
                    parameter_index,
                    parameter.symbol,
                    parameter.name.clone(),
                    parameter_type,
                    argument.value,
                    evaluation_index,
                    self.expression_categories[argument.value.index()],
                ),
            ));
        }
        if !valid {
            return Ok(self.failed_construction());
        }
        descriptors.sort_by_key(|(parameter_index, _)| *parameter_index);
        self.constructions.push(ConstructionDescriptor::new(
            expression,
            target,
            instance_arguments,
            result_type,
            descriptors.into_iter().map(|(_, item)| item).collect(),
        ));
        Ok(ExprCheck {
            ty: result_type,
            falls_through: true,
        })
    }

    fn check_box_construction(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<ExprCheck, TypeCheckingError> {
        let placeholder = Parameter {
            symbol: None,
            name: "element".to_owned(),
            ty: self.error_type(),
            span: None,
        };
        let parameters = [placeholder.as_call_parameter()];
        let mapping = match self.map_arguments(&parameters, arguments, call_span)? {
            Ok(mapping) => mapping,
            Err(error) => {
                self.emit_mapping_error(error)?;
                self.check_construction_operands(arguments)?;
                return Ok(self.failed_construction());
            }
        };
        let element = match type_arguments {
            [] => self.check_expression(arguments[0].value, None, None)?.ty,
            [type_ref] => self.resolve_type_ref(*type_ref)?,
            _ => {
                self.emit(
                    self.type_argument_arity_code,
                    "Box constructor accepts exactly one type argument",
                    self.ast().expressions().get(callee)?.span(),
                )?;
                self.check_construction_operands(arguments)?;
                return Ok(self.failed_construction());
            }
        };
        let valid = matches!(self.kind(element), TypeKind::Nominal { nominal, .. }
            if self.nominals.iter().any(|item| item.id() == *nominal && item.kind() == NominalKind::ValueClass));
        if !valid {
            self.emit(
                self.invalid_box_argument_code,
                "Box type argument must be a concrete value class instance",
                self.ast().expressions().get(arguments[0].value)?.span(),
            )?;
            return Ok(self.failed_construction());
        }
        let result_type = self.types.intern(TypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Box,
            arguments: vec![element],
        });
        self.finish_construction(
            expression,
            ConstructionTarget::IntrinsicBox,
            vec![element],
            result_type,
            arguments,
            &mapping,
            &[Parameter {
                ty: element,
                ..placeholder
            }],
            &BTreeMap::new(),
        )
    }

    fn check_rc_construction(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<ExprCheck, TypeCheckingError> {
        let placeholder = Parameter {
            symbol: None,
            name: "value".to_owned(),
            ty: self.error_type(),
            span: None,
        };
        let parameters = [placeholder.as_call_parameter()];
        let mapping = match self.map_arguments(&parameters, arguments, call_span)? {
            Ok(mapping) => mapping,
            Err(error) => {
                self.emit_mapping_error(error)?;
                self.check_construction_operands(arguments)?;
                return Ok(self.failed_construction());
            }
        };
        let payload = match type_arguments {
            [] => self.check_expression(arguments[0].value, None, None)?.ty,
            [type_ref] => self.resolve_type_ref(*type_ref)?,
            _ => {
                self.emit(
                    self.type_argument_arity_code,
                    "Rc constructor accepts at most one type argument",
                    self.ast().expressions().get(callee)?.span(),
                )?;
                self.check_construction_operands(arguments)?;
                return Ok(self.failed_construction());
            }
        };
        if !self.is_error(payload) && !self.is_structurally_storable_type(payload) {
            self.emit(
                self.invalid_container_element_code,
                "Rc payload type is not structurally storable",
                self.ast().expressions().get(arguments[0].value)?.span(),
            )?;
            return Ok(self.failed_construction());
        }
        if self.is_error(payload) || self.is_deferred(payload) {
            return Ok(self.failed_construction());
        }
        let result_type = self.types.intern(TypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Rc,
            arguments: vec![payload],
        });
        self.finish_construction(
            expression,
            ConstructionTarget::IntrinsicRc,
            vec![payload],
            result_type,
            arguments,
            &mapping,
            &[Parameter {
                ty: payload,
                ..placeholder
            }],
            &BTreeMap::new(),
        )
    }

    pub(super) fn check_construction_operands(
        &mut self,
        arguments: &[CallArgument],
    ) -> Result<(), TypeCheckingError> {
        for argument in arguments {
            self.check_expression(argument.value, None, None)?;
        }
        Ok(())
    }

    fn failed_construction(&mut self) -> ExprCheck {
        ExprCheck {
            ty: self.error_type(),
            falls_through: true,
        }
    }

    /// 无 payload enum case 是值表达式，不经过普通 call 分派。
    pub(super) fn check_bare_enum_construction(
        &mut self,
        expression: ExpressionId,
        expected: Option<TypeId>,
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let node = self.ast().expressions().get(expression)?;
        let name_span = match node.payload() {
            Expression::Name => node.span(),
            Expression::Member { name_span, .. } => *name_span,
            _ => return Ok(None),
        };
        let Some(ReferenceTarget::Symbol(symbol)) =
            self.reference(name_span, Namespace::Value).cloned()
        else {
            return Ok(None);
        };
        let Some(case) = self.enum_case_by_value_symbol.get(&symbol).copied() else {
            return Ok(None);
        };
        let descriptor = self
            .enum_case(case)
            .cloned()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        if !descriptor.payloads().is_empty() {
            return Ok(None);
        }
        let nominal = self
            .nominals
            .iter()
            .find(|item| item.id() == descriptor.root())
            .cloned()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        let (result_type, arguments) = if nominal.type_parameters().is_empty() {
            (
                self.types.intern(TypeKind::Nominal {
                    nominal: descriptor.root(),
                    arguments: Vec::new(),
                }),
                Vec::new(),
            )
        } else if !self.candidate_local_expected
            && let Some(expected) = expected
            && let TypeKind::Nominal {
                nominal: root,
                arguments,
            } = self.kind(expected).clone()
            && root == descriptor.root()
            && arguments.len() == nominal.type_parameters().len()
            && arguments
                .iter()
                .all(|argument| self.is_complete_construction_expected(*argument))
        {
            (expected, arguments)
        } else {
            return self.reject_construction_inference(name_span, nominal.type_parameters());
        };
        self.constructions.push(ConstructionDescriptor::new(
            expression,
            ConstructionTarget::EnumCase(case),
            arguments,
            result_type,
            Vec::new(),
        ));
        Ok(Some(ExprCheck {
            ty: result_type,
            falls_through: true,
        }))
    }
}
