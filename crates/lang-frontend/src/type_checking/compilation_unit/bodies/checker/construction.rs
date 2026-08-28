//! SPEC-0197 compilation-unit 源码 nominal 与 enum case construction。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::{DeclarationId, Namespace, SourceUnitId, UnitReferenceTarget, UnitSymbolId},
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        Capability, CompilationUnitTypeError, Copyability, ExternalTypeBinding,
        IntrinsicTypeConstructor, NominalKind, ParameterMode, TypeCheckingError,
        UnitConstructionArgumentDescriptor, UnitConstructionDescriptor,
        UnitConstructionInstanceKey, UnitConstructionTarget, UnitExpressionId, UnitFieldSignature,
        UnitTypeId, UnitTypeKind, UnitTypeParameterBound,
        argument_mapping::{MappedParameter, map_arguments},
    },
};

use super::{BodyChecker, ExpressionCheck, copyability::UnitTransferability};

mod intrinsic;

#[derive(Clone, Copy)]
enum Target {
    Nominal(DeclarationId),
    EnumCase(UnitSymbolId),
    Box,
    Rc,
    Invalid,
}

#[derive(Clone)]
struct Shape {
    target: UnitConstructionTarget,
    root: DeclarationId,
    parameters: Vec<UnitFieldSignature>,
    type_parameters: Vec<UnitSymbolId>,
    result_template: UnitTypeId,
}

struct ConstructionInstance {
    arguments: Vec<UnitTypeId>,
    substitutions: BTreeMap<UnitSymbolId, UnitTypeId>,
}

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_source_construction_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        let Some(target) = self.construction_target(source, callee)? else {
            return Ok(None);
        };
        let callee_span = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?
            .span();
        if matches!(target, Target::Invalid) {
            self.check_construction_operands(source, arguments, return_type)?;
            self.emit(
                codes::INVALID_CONSTRUCTION_TARGET,
                "this type identity cannot be constructed",
                callee_span,
            )?;
            return Ok(Some(self.failed_construction()));
        }
        if matches!(target, Target::Box | Target::Rc) {
            return self
                .check_intrinsic_construction(
                    source,
                    expression,
                    call_span,
                    callee_span,
                    type_arguments,
                    arguments,
                    target,
                    expected,
                    expected_span,
                    return_type,
                )
                .map(Some);
        }
        let shape = self.source_construction_shape(target)?;
        if shape.parameters.is_empty() && matches!(target, Target::EnumCase(_)) {
            return Ok(None);
        }
        let mapped = shape
            .parameters
            .iter()
            .map(|parameter| MappedParameter {
                name: Some(parameter.name().to_owned()),
                mode: ParameterMode::Value,
                ty: parameter.ty(),
                span: Some(parameter.span()),
            })
            .collect::<Vec<_>>();
        let mapping =
            match map_arguments(self.sources, &mapped, arguments, call_span, |argument| {
                Ok(self.is_syntactic_place(source, argument))
            })? {
                Ok(mapping) => mapping,
                Err(error) => {
                    self.emit_mapping_error(error)?;
                    self.check_construction_operands(source, arguments, return_type)?;
                    return Ok(Some(self.failed_construction()));
                }
            };
        let Some(instance) = self.infer_construction_instance(
            source,
            callee_span,
            type_arguments,
            arguments,
            &mapping,
            &shape,
            expected,
            return_type,
        )?
        else {
            return Ok(Some(self.failed_construction()));
        };
        let result_type = self.substitute_type(shape.result_template, &instance.substitutions)?;
        self.finish_construction(
            source,
            expression,
            shape,
            instance.arguments,
            instance.substitutions,
            result_type,
            arguments,
            &mapping,
            expected,
            expected_span,
            return_type,
        )
        .map(Some)
    }

    pub(super) fn check_bare_enum_construction(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        reference_span: Span,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        let Some(case) = self.enum_case_by_value_symbol(source, reference_span) else {
            return Ok(None);
        };
        let shape = self.source_construction_shape(Target::EnumCase(case))?;
        if !shape.parameters.is_empty() {
            return Ok(None);
        }
        let Some(instance) =
            self.infer_empty_construction_instance(reference_span, &shape, expected)?
        else {
            return Ok(Some(self.failed_construction()));
        };
        let result_type = self.substitute_type(shape.result_template, &instance.substitutions)?;
        let expression_span = self
            .file(source)
            .ast()
            .expressions()
            .get(expression)
            .map_err(TypeCheckingError::from)?
            .span();
        if !self.validate_construction_result(
            result_type,
            expected,
            expression_span,
            expected_span,
        )? {
            return Ok(Some(self.failed_construction()));
        }
        self.parts.constructions.push(UnitConstructionDescriptor {
            expression: UnitExpressionId::new(source, expression),
            instance: UnitConstructionInstanceKey {
                target: shape.target,
                type_arguments: instance.arguments,
            },
            result_type,
            arguments: Vec::new(),
        });
        Ok(Some(ExpressionCheck {
            ty: result_type,
            falls_through: true,
        }))
    }

    fn construction_target(
        &self,
        source: SourceUnitId,
        callee: ExpressionId,
    ) -> Result<Option<Target>, CompilationUnitTypeError> {
        let node = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?;
        let lookup_span = match node.payload() {
            Expression::Member {
                name_span,
                safe: false,
                ..
            } => {
                if let Some(case) = self.enum_case_by_value_symbol(source, *name_span) {
                    return Ok(Some(Target::EnumCase(case)));
                }
                return Ok(None);
            }
            Expression::Name => node.span(),
            _ => return Ok(None),
        };
        if let Some(case) = self.enum_case_by_value_symbol(source, lookup_span) {
            return Ok(Some(Target::EnumCase(case)));
        }
        if !matches!(
            self.reference(source, lookup_span, Namespace::Value),
            None | Some(UnitReferenceTarget::Unresolved)
        ) {
            return Ok(None);
        }
        Ok(match self.reference(source, lookup_span, Namespace::Type) {
            Some(UnitReferenceTarget::Declaration(declaration)) => self
                .signatures
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .map(|nominal| {
                    if matches!(nominal.kind(), NominalKind::Class | NominalKind::ValueClass) {
                        Target::Nominal(*declaration)
                    } else {
                        Target::Invalid
                    }
                }),
            Some(UnitReferenceTarget::External(external)) => {
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
            Some(UnitReferenceTarget::Symbol(_)) | Some(UnitReferenceTarget::Symbols(_)) => {
                Some(Target::Invalid)
            }
            _ => None,
        })
    }

    fn enum_case_by_value_symbol(&self, source: SourceUnitId, span: Span) -> Option<UnitSymbolId> {
        let UnitReferenceTarget::Symbol(symbol) = self.reference(source, span, Namespace::Value)?
        else {
            return None;
        };
        self.names
            .names()
            .index()
            .declarations()
            .iter()
            .find_map(|declaration| {
                self.signatures
                    .declaration(declaration.id())?
                    .nominal()?
                    .enum_cases()
                    .iter()
                    .any(|case| case.value_symbol() == *symbol)
                    .then_some(*symbol)
            })
    }

    fn source_construction_shape(&self, target: Target) -> Result<Shape, CompilationUnitTypeError> {
        match target {
            Target::Nominal(declaration) => {
                let nominal = self
                    .signatures
                    .declaration(declaration)
                    .and_then(|item| item.nominal())
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                Ok(Shape {
                    target: UnitConstructionTarget::Nominal(declaration),
                    root: declaration,
                    parameters: nominal.fields().to_vec(),
                    type_parameters: nominal.type_parameters().to_vec(),
                    result_template: nominal.ty(),
                })
            }
            Target::EnumCase(symbol) => self
                .names
                .names()
                .index()
                .declarations()
                .iter()
                .find_map(|declaration| {
                    let nominal = self.signatures.declaration(declaration.id())?.nominal()?;
                    let case = nominal
                        .enum_cases()
                        .iter()
                        .find(|case| case.value_symbol() == symbol)?;
                    let UnitTypeKind::EnumCase { root, .. } =
                        self.signatures.types().get(case.case_type())?
                    else {
                        return None;
                    };
                    Some(Shape {
                        target: UnitConstructionTarget::EnumCase(symbol),
                        root: declaration.id(),
                        parameters: case.payloads().to_vec(),
                        type_parameters: nominal.type_parameters().to_vec(),
                        result_template: *root,
                    })
                })
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol),
            Target::Box | Target::Rc | Target::Invalid => {
                unreachable!("intrinsic or invalid construction has no source shape")
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_construction_instance(
        &mut self,
        source: SourceUnitId,
        callee_span: Span,
        type_refs: &[TypeRefId],
        arguments: &[CallArgument],
        mapping: &[usize],
        shape: &Shape,
        expected: Option<UnitTypeId>,
        return_type: UnitTypeId,
    ) -> Result<Option<ConstructionInstance>, CompilationUnitTypeError> {
        let mut substitutions = BTreeMap::new();
        let mut origins = BTreeMap::new();
        if !type_refs.is_empty() {
            if type_refs.len() != shape.type_parameters.len() {
                self.emit(
                    codes::TYPE_ARGUMENT_ARITY,
                    "constructor type argument count does not match its declaration",
                    callee_span,
                )?;
                self.check_construction_operands(source, arguments, return_type)?;
                return Ok(None);
            }
            for (&parameter, &type_ref) in shape.type_parameters.iter().zip(type_refs) {
                let actual = self.resolve_body_type_ref(source, type_ref)?;
                if self.construction_type_contains_poison(actual) {
                    self.check_construction_operands(source, arguments, return_type)?;
                    return Ok(None);
                }
                substitutions.insert(parameter, actual);
                origins.insert(
                    parameter,
                    self.file(source)
                        .ast()
                        .type_refs()
                        .get(type_ref)
                        .map_err(TypeCheckingError::from)?
                        .span(),
                );
            }
        } else if !shape.type_parameters.is_empty() {
            let parameter_set = shape
                .type_parameters
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            let mut poisoned = false;
            for (argument_index, argument) in arguments.iter().enumerate() {
                if self.is_lambda_syntax(source, argument.value) {
                    continue;
                }
                let actual = self
                    .check_expression(source, argument.value, None, None, return_type)?
                    .ty;
                if self.construction_type_contains_poison(actual) {
                    poisoned = true;
                    continue;
                }
                let span = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(argument.value)
                    .map_err(TypeCheckingError::from)?
                    .span();
                if let Err(parameter) = self.infer_unit_type_arguments(
                    shape.parameters[mapping[argument_index]].ty(),
                    actual,
                    &parameter_set,
                    &mut substitutions,
                    &mut origins,
                    span,
                ) {
                    let rejected = self.reject_construction_inference(callee_span, parameter)?;
                    self.check_construction_operands(source, arguments, return_type)?;
                    return Ok(rejected);
                }
            }
            if poisoned {
                self.check_construction_operands(source, arguments, return_type)?;
                return Ok(None);
            }
            if let Some(parameter) = self.add_expected_construction_arguments(
                shape,
                expected,
                callee_span,
                &mut substitutions,
                &mut origins,
            )? {
                let rejected = self.reject_construction_inference(callee_span, parameter)?;
                self.check_construction_operands(source, arguments, return_type)?;
                return Ok(rejected);
            }
            if let Some(&parameter) = shape
                .type_parameters
                .iter()
                .find(|parameter| !substitutions.contains_key(parameter))
            {
                let rejected = self.reject_construction_inference(callee_span, parameter)?;
                self.check_construction_operands(source, arguments, return_type)?;
                return Ok(rejected);
            }
        } else if !type_refs.is_empty() {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "constructor does not accept type arguments",
                callee_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(None);
        }
        if !self.construction_bounds_satisfied(
            &shape.type_parameters,
            &substitutions,
            &origins,
            callee_span,
        )? {
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(None);
        }
        Ok(Some(ConstructionInstance {
            arguments: shape
                .type_parameters
                .iter()
                .map(|parameter| substitutions[parameter])
                .collect(),
            substitutions,
        }))
    }

    fn infer_empty_construction_instance(
        &mut self,
        span: Span,
        shape: &Shape,
        expected: Option<UnitTypeId>,
    ) -> Result<Option<ConstructionInstance>, CompilationUnitTypeError> {
        let mut substitutions = BTreeMap::new();
        let mut origins = BTreeMap::new();
        if let Some(parameter) = self.add_expected_construction_arguments(
            shape,
            expected,
            span,
            &mut substitutions,
            &mut origins,
        )? {
            return self.reject_construction_inference(span, parameter);
        }
        if let Some(&parameter) = shape
            .type_parameters
            .iter()
            .find(|parameter| !substitutions.contains_key(parameter))
        {
            return self.reject_construction_inference(span, parameter);
        }
        if !self.construction_bounds_satisfied(
            &shape.type_parameters,
            &substitutions,
            &origins,
            span,
        )? {
            return Ok(None);
        }
        Ok(Some(ConstructionInstance {
            arguments: shape
                .type_parameters
                .iter()
                .map(|parameter| substitutions[parameter])
                .collect(),
            substitutions,
        }))
    }

    fn add_expected_construction_arguments(
        &self,
        shape: &Shape,
        expected: Option<UnitTypeId>,
        origin: Span,
        substitutions: &mut BTreeMap<UnitSymbolId, UnitTypeId>,
        origins: &mut BTreeMap<UnitSymbolId, Span>,
    ) -> Result<Option<UnitSymbolId>, CompilationUnitTypeError> {
        if self.candidate_local_expected {
            return Ok(None);
        }
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = expected.and_then(|ty| self.signatures.types().get(ty))
        else {
            return Ok(None);
        };
        if *declaration != shape.root
            || arguments.len() != shape.type_parameters.len()
            || !arguments
                .iter()
                .all(|argument| self.complete_construction_expected(*argument))
        {
            return Ok(None);
        }
        for (&parameter, &actual) in shape.type_parameters.iter().zip(arguments) {
            if let Some(previous) = substitutions.insert(parameter, actual)
                && previous != actual
            {
                substitutions.remove(&parameter);
                return Ok(Some(parameter));
            }
            origins.entry(parameter).or_insert(origin);
        }
        Ok(None)
    }

    fn complete_construction_expected(&self, ty: UnitTypeId) -> bool {
        fn complete(
            checker: &BodyChecker<'_>,
            ty: UnitTypeId,
            active: &mut BTreeSet<UnitTypeId>,
        ) -> bool {
            if !active.insert(ty) {
                return true;
            }
            let result = match checker.signatures.types().get(ty) {
                Some(
                    UnitTypeKind::Error
                    | UnitTypeKind::Deferred(_)
                    | UnitTypeKind::TypeParameter(_)
                    | UnitTypeKind::StaticSelf(_)
                    | UnitTypeKind::Capability(_),
                )
                | None => false,
                Some(UnitTypeKind::Nullable(inner)) => complete(checker, *inner, active),
                Some(UnitTypeKind::Function {
                    parameters,
                    return_type,
                    ..
                }) => {
                    parameters
                        .iter()
                        .all(|parameter| complete(checker, parameter.ty(), active))
                        && complete(checker, *return_type, active)
                }
                Some(
                    UnitTypeKind::Nominal { arguments, .. }
                    | UnitTypeKind::Intrinsic { arguments, .. },
                ) => arguments
                    .iter()
                    .all(|argument| complete(checker, *argument, active)),
                Some(UnitTypeKind::EnumCase { root, .. }) => complete(checker, *root, active),
                Some(UnitTypeKind::Builtin(_) | UnitTypeKind::IntegerLiteral(_)) => true,
            };
            active.remove(&ty);
            result
        }
        complete(self, ty, &mut BTreeSet::new())
    }

    fn construction_bounds_satisfied(
        &mut self,
        parameters: &[UnitSymbolId],
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        origins: &BTreeMap<UnitSymbolId, Span>,
        fallback: Span,
    ) -> Result<bool, CompilationUnitTypeError> {
        for &parameter in parameters {
            let actual = substitutions[&parameter];
            let bound = self
                .signatures
                .type_parameter(parameter)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
                .bound();
            let failure = match bound {
                UnitTypeParameterBound::Any | UnitTypeParameterBound::Error => None,
                UnitTypeParameterBound::Interface(interface) => {
                    let expected = self.substitute_type(interface, substitutions)?;
                    (!self.satisfies_interface(actual, expected)?).then_some((
                        codes::TYPE_ARGUMENT_BOUND,
                        "constructor type argument does not satisfy its interface bound",
                    ))
                }
                UnitTypeParameterBound::Capability(Capability::Copyable) => {
                    (self.copyability_of(actual) != Copyability::Copyable).then_some((
                        codes::COPYABLE_TYPE_ARGUMENT_BOUND,
                        "constructor type argument does not satisfy its Copyable bound",
                    ))
                }
                UnitTypeParameterBound::Capability(Capability::Transferable) => {
                    (self.transferability_of(actual) != UnitTransferability::Transferable)
                        .then_some((
                            codes::TRANSFERABLE_TYPE_ARGUMENT_BOUND,
                            "constructor type argument does not satisfy its Transferable bound",
                        ))
                }
            };
            if let Some((code, message)) = failure {
                self.emit_maybe_label(
                    code,
                    message,
                    origins.get(&parameter).copied().unwrap_or(fallback),
                    Some(self.unit_symbol_span(parameter)?),
                    "type parameter bound declared here",
                )?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn reject_construction_inference<T>(
        &mut self,
        primary: Span,
        parameter: UnitSymbolId,
    ) -> Result<Option<T>, CompilationUnitTypeError> {
        self.emit_maybe_label(
            codes::CONSTRUCTION_INFERENCE,
            "constructor type arguments cannot be inferred completely and consistently",
            primary,
            Some(self.unit_symbol_span(parameter)?),
            "unresolved or conflicting constructor type parameter declared here",
        )?;
        Ok(None)
    }

    pub(super) fn construction_type_contains_poison(&self, ty: UnitTypeId) -> bool {
        fn contains(
            checker: &BodyChecker<'_>,
            ty: UnitTypeId,
            active: &mut BTreeSet<UnitTypeId>,
        ) -> bool {
            if !active.insert(ty) {
                return false;
            }
            let result = match checker.signatures.types().get(ty) {
                Some(UnitTypeKind::Error | UnitTypeKind::Deferred(_)) | None => true,
                Some(UnitTypeKind::Nullable(inner)) => contains(checker, *inner, active),
                Some(UnitTypeKind::Function {
                    parameters,
                    return_type,
                    ..
                }) => {
                    parameters
                        .iter()
                        .any(|parameter| contains(checker, parameter.ty(), active))
                        || contains(checker, *return_type, active)
                }
                Some(
                    UnitTypeKind::Nominal { arguments, .. }
                    | UnitTypeKind::Intrinsic { arguments, .. },
                ) => arguments
                    .iter()
                    .any(|argument| contains(checker, *argument, active)),
                Some(UnitTypeKind::EnumCase { root, .. }) => contains(checker, *root, active),
                Some(
                    UnitTypeKind::Builtin(_)
                    | UnitTypeKind::TypeParameter(_)
                    | UnitTypeKind::StaticSelf(_)
                    | UnitTypeKind::Capability(_)
                    | UnitTypeKind::IntegerLiteral(_),
                ) => false,
            };
            active.remove(&ty);
            result
        }

        contains(self, ty, &mut BTreeSet::new())
    }

    fn validate_construction_result(
        &mut self,
        actual: UnitTypeId,
        expected: Option<UnitTypeId>,
        primary: Span,
        expected_span: Option<Span>,
    ) -> Result<bool, CompilationUnitTypeError> {
        let Some(expected) = expected else {
            return Ok(true);
        };
        if self.assignable(actual, expected)
            || self.is_error(actual)
            || self.is_error(expected)
            || self.is_deferred(actual)
            || self.is_deferred(expected)
        {
            return Ok(true);
        }
        self.emit_maybe_label(
            codes::TYPE_MISMATCH,
            "expression type does not match the expected type",
            primary,
            expected_span,
            format!(
                "expected {}, found {}",
                self.type_name(expected),
                self.type_name(actual)
            ),
        )?;
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_construction(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        shape: Shape,
        instance_arguments: Vec<UnitTypeId>,
        substitutions: BTreeMap<UnitSymbolId, UnitTypeId>,
        result_type: UnitTypeId,
        arguments: &[CallArgument],
        mapping: &[usize],
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let mut valid = true;
        let mut descriptors = Vec::with_capacity(arguments.len());
        for (evaluation_index, argument) in arguments.iter().enumerate() {
            let parameter_index = mapping[evaluation_index];
            let parameter = &shape.parameters[parameter_index];
            let parameter_type = self.substitute_type(parameter.ty(), &substitutions)?;
            let parameter_poisoned = self.construction_type_contains_poison(parameter_type);
            let argument_key = UnitExpressionId::new(source, argument.value);
            let already_checked = self.parts.expression_types.contains_key(&argument_key);
            let checked = self.check_expression(
                source,
                argument.value,
                Some(parameter_type),
                Some(parameter.span()),
                return_type,
            )?;
            if already_checked
                && !self.is_error(checked.ty)
                && !self.is_deferred(checked.ty)
                && !self.assignable(checked.ty, parameter_type)
            {
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
                    Some(parameter.span()),
                    format!(
                        "expected {}, found {}",
                        self.type_name(parameter_type),
                        self.type_name(checked.ty)
                    ),
                )?;
                valid = false;
            }
            valid &=
                !parameter_poisoned && !self.is_error(checked.ty) && !self.is_deferred(checked.ty);
            descriptors.push((
                parameter_index,
                UnitConstructionArgumentDescriptor {
                    parameter_index,
                    parameter_symbol: Some(parameter.symbol()),
                    parameter_name: parameter.name().to_owned(),
                    parameter_type,
                    argument: argument_key,
                    evaluation_index,
                    category: self.expression_category(source, argument.value),
                },
            ));
        }
        if !valid {
            return Ok(self.failed_construction());
        }
        let expression_span = self
            .file(source)
            .ast()
            .expressions()
            .get(expression)
            .map_err(TypeCheckingError::from)?
            .span();
        if !self.validate_construction_result(
            result_type,
            expected,
            expression_span,
            expected_span,
        )? {
            return Ok(self.failed_construction());
        }
        descriptors.sort_by_key(|(index, _)| *index);
        self.parts.constructions.push(UnitConstructionDescriptor {
            expression: UnitExpressionId::new(source, expression),
            instance: UnitConstructionInstanceKey {
                target: shape.target,
                type_arguments: instance_arguments,
            },
            result_type,
            arguments: descriptors
                .into_iter()
                .map(|(_, descriptor)| descriptor)
                .collect(),
        });
        Ok(ExpressionCheck {
            ty: result_type,
            falls_through: true,
        })
    }

    fn check_construction_operands(
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

    fn failed_construction(&mut self) -> ExpressionCheck {
        ExpressionCheck {
            ty: self.error_type(),
            falls_through: true,
        }
    }
}
