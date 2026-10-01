use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::{ExternalSymbolId, Namespace, ReferenceTarget, SymbolId, SymbolKind},
    parser::{CallArgument, Expression, ParameterModeMarker},
    source::Span,
    type_checking::{
        CallArgumentDescriptor, CallDescriptor, CallReceiverDescriptor, CallReceiverOrigin,
        CallableTarget, EnvironmentFunctionEffect, ExpressionCategory, ExternalTypeBinding,
    },
};

use super::*;

mod generic;

#[derive(Clone)]
struct CallCandidate {
    target: CallableTarget,
    declaration_span: Option<Span>,
    type_parameters: Vec<SymbolId>,
    instance_arguments: Vec<TypeId>,
    receiver: Option<CallReceiverDescriptor>,
    parameters: Vec<MappedParameter<TypeId>>,
    return_type: TypeId,
    cross_thread_parameters: BTreeSet<usize>,
    aborts: bool,
    prints_line: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MemberCallShapeType {
    Builtin(BuiltinType),
    Nullable(Box<Self>),
    Function(bool, Vec<Self>, Box<Self>),
    Nominal(NominalId, Vec<Self>),
    Intrinsic(IntrinsicTypeConstructor, Vec<Self>),
    Parameter(usize),
    OuterParameter(SymbolId),
    Capability(Capability),
    Other(TypeId),
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
        if let Some(result) = self.check_construction_call(
            expression,
            call_span,
            callee,
            &type_arguments,
            &arguments,
            expected,
        )? {
            return Ok(result);
        }
        if let Some(result) =
            self.check_rc_share_call(expression, call_span, callee, &type_arguments, &arguments)?
        {
            return Ok(result);
        }
        if let Some(result) = self.check_intrinsic_ownership_primitive_call(
            expression,
            call_span,
            callee,
            &type_arguments,
            &arguments,
        )? {
            return Ok(result);
        }
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
        let explicit_types = type_arguments
            .iter()
            .map(|&type_argument| self.resolve_type_ref(type_argument))
            .collect::<Result<Vec<_>, _>>()?;

        let mut candidates = self.call_candidates(callee)?;
        let handles_source_instantiation = candidates.iter().any(|candidate| {
            matches!(candidate.target, CallableTarget::Source(_))
                && (!candidate.type_parameters.is_empty() || !type_arguments.is_empty())
        });
        if (!type_arguments.is_empty()
            || candidates
                .iter()
                .any(|candidate| !candidate.type_parameters.is_empty()))
            && !handles_source_instantiation
        {
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
                    type_parameters: Vec::new(),
                    instance_arguments: Vec::new(),
                    receiver: None,
                    parameters: parameters
                        .into_iter()
                        .map(|parameter| MappedParameter {
                            name: None,
                            mode: parameter.mode,
                            ty: parameter.ty,
                            span: None,
                        })
                        .collect(),
                    return_type,
                    cross_thread_parameters: BTreeSet::new(),
                    aborts: false,
                    prints_line: false,
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

        let initial_candidate_count = candidates.len();
        let mut mapped = Vec::new();
        let mut first_error = None;
        for candidate in candidates {
            match self.map_arguments(&candidate.parameters, &arguments, call_span)? {
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

        let needs_inference = explicit_types.is_empty()
            && mapped
                .iter()
                .any(|(candidate, _)| !candidate.type_parameters.is_empty());
        let inference_types = if needs_inference {
            self.check_inference_arguments(&arguments)?
        } else {
            vec![None; arguments.len()]
        };
        let mut instantiated = Vec::with_capacity(mapped.len());
        let mut first_failure = None;
        for (candidate, mapping) in mapped {
            match self.instantiate_candidate(
                candidate,
                &mapping,
                &arguments,
                &inference_types,
                (&type_arguments, &explicit_types),
                self.ast().expressions().get(callee)?.span(),
            )? {
                Ok(candidate) => instantiated.push((candidate, mapping)),
                Err(failure) => {
                    first_failure.get_or_insert(failure);
                }
            }
        }
        if instantiated.is_empty() {
            if initial_candidate_count == 1 {
                if let Some(failure) = first_failure {
                    self.emit_instantiation_failure(failure)?;
                }
            } else {
                self.emit(
                    self.no_matching_overload_code,
                    "no overload matches the call arguments",
                    self.ast().expressions().get(callee)?.span(),
                )?;
            }
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        let mut mapped = instantiated;

        if mapped.len() == 1 {
            let (candidate, mapping) = mapped.pop().expect("one mapped candidate");
            return self.finish_unique_call(expression, callee, &arguments, candidate, mapping);
        }

        let lambda_arguments = arguments
            .iter()
            .map(|argument| self.is_lambda_literal(argument.value))
            .collect::<Result<Vec<_>, _>>()?;
        if lambda_arguments.iter().any(|is_lambda| *is_lambda) {
            return self.finish_overload_lambda_call(
                expression,
                callee,
                &arguments,
                mapped,
                &lambda_arguments,
            );
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
            0 => self.no_matching_overload(callee),
            1 => {
                let (candidate, mapping) = mapped.pop().expect("one matching candidate");
                // 初始映射只看到语法形态；类型过滤之后仍须检查真实的 inout place。
                self.finish_unique_call(expression, callee, &arguments, candidate, mapping)
            }
            _ => {
                let spans = mapped
                    .iter()
                    .filter_map(|(candidate, _)| candidate.declaration_span)
                    .take(2)
                    .collect::<Vec<_>>();
                self.ambiguous_overload(callee, &spans)
            }
        }
    }

    fn finish_overload_lambda_call(
        &mut self,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        mut candidates: Vec<(CallCandidate, Vec<usize>)>,
        lambda_arguments: &[bool],
    ) -> Result<ExprCheck, TypeCheckingError> {
        let mut argument_types = vec![None; arguments.len()];
        let mut poisoned = false;
        for (index, argument) in arguments.iter().enumerate() {
            if lambda_arguments[index] {
                continue;
            }
            let result = self.check_expression(argument.value, None, None)?;
            poisoned |= self.is_error(result.ty) || self.is_deferred(result.ty);
            argument_types[index] = Some(result.ty);
        }
        if poisoned {
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        candidates.retain(|(candidate, mapping)| {
            argument_types
                .iter()
                .enumerate()
                .all(|(argument_index, argument_type)| {
                    argument_type.is_none_or(|argument_type| {
                        self.assignable(
                            argument_type,
                            candidate.parameters[mapping[argument_index]].ty,
                        )
                    })
                })
        });
        match candidates.len() {
            0 => return self.no_matching_overload(callee),
            1 => {
                let (candidate, mapping) = candidates.pop().expect("one matching candidate");
                return self.finish_unique_call(expression, callee, arguments, candidate, mapping);
            }
            _ => {}
        }

        let baseline = self.trial_state();
        let baseline_diagnostics = self.diagnostics.len();
        let mut successes = Vec::new();
        let previous_candidate_local_expected = self.candidate_local_expected;
        self.candidate_local_expected = true;
        for (candidate, mapping) in candidates {
            self.restore_trial_state(baseline.clone());
            let declaration_span = candidate.declaration_span;
            let result =
                self.finish_unique_call(expression, callee, arguments, candidate, mapping)?;
            if self.diagnostics.len() == baseline_diagnostics
                && !self.is_error(result.ty)
                && !self.is_deferred(result.ty)
            {
                successes.push((self.trial_state(), result, declaration_span));
            }
        }
        self.candidate_local_expected = previous_candidate_local_expected;
        self.restore_trial_state(baseline);

        match successes.len() {
            0 => self.no_matching_overload(callee),
            1 => {
                let (state, result, _) = successes.pop().expect("one successful candidate trial");
                self.restore_trial_state(state);
                Ok(result)
            }
            _ => {
                let spans = successes
                    .iter()
                    .filter_map(|(_, _, span)| *span)
                    .take(2)
                    .collect::<Vec<_>>();
                self.ambiguous_overload(callee, &spans)
            }
        }
    }

    fn no_matching_overload(
        &mut self,
        callee: ExpressionId,
    ) -> Result<ExprCheck, TypeCheckingError> {
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

    fn ambiguous_overload(
        &mut self,
        callee: ExpressionId,
        declarations: &[Span],
    ) -> Result<ExprCheck, TypeCheckingError> {
        let primary = self.ast().expressions().get(callee)?.span();
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.ambiguous_call_code,
            "call remains ambiguous after argument type checking",
            primary,
        )?;
        for &span in declarations {
            diagnostic.add_label(self.sources, span, "matching callable declared here")?;
        }
        self.diagnostics.push(diagnostic);
        Ok(ExprCheck {
            ty: self.error_type(),
            falls_through: true,
        })
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
            Expression::SuperMember {
                interface,
                name_span,
                ..
            } => self.super_call_candidates(interface, name_span),
            _ => Ok(Vec::new()),
        }
    }

    fn super_call_candidates(
        &mut self,
        interface: TypeRefId,
        name_span: Span,
    ) -> Result<Vec<CallCandidate>, TypeCheckingError> {
        let interface = self.resolve_static_type_ref(interface)?;
        let TypeKind::Nominal { nominal, arguments } = self.kind(interface).clone() else {
            return Ok(Vec::new());
        };
        let owner = self
            .nominals
            .iter()
            .find(|descriptor| descriptor.id() == nominal)
            .cloned()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        if owner.kind() != NominalKind::Interface {
            self.emit(
                self.interface_member_mismatch_code,
                "super qualifier must name an inherited interface",
                name_span,
            )?;
            return Ok(Vec::new());
        }
        let current_receiver = self
            .classifiers
            .last()
            .copied()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        if !self.satisfies_interface(current_receiver, interface)? {
            self.emit(
                self.interface_member_mismatch_code,
                "super qualifier must name an inherited interface",
                name_span,
            )?;
            return Ok(Vec::new());
        }
        let substitutions = owner
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments.iter().copied())
            .collect::<BTreeMap<_, _>>();
        let name = self.sources.slice(name_span)?.to_owned();
        let mut instances = vec![interface];
        for inherited in owner.interfaces() {
            instances.push(self.substitute_type(*inherited, &substitutions)?);
        }
        let descriptors = self.typed_callables.clone();
        let mut candidates = Vec::new();
        let mut seen_shapes = BTreeSet::new();
        for instance in instances {
            let TypeKind::Nominal {
                nominal: instance_owner,
                arguments: instance_arguments,
            } = self.kind(instance).clone()
            else {
                continue;
            };
            let instance_descriptor = self
                .nominals
                .iter()
                .find(|descriptor| descriptor.id() == instance_owner)
                .cloned()
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            let instance_substitutions = instance_descriptor
                .type_parameters()
                .iter()
                .copied()
                .zip(instance_arguments.iter().copied())
                .collect::<BTreeMap<_, _>>();
            for descriptor in descriptors
                .iter()
                .filter(|descriptor| descriptor.owner() == Some(instance_owner))
            {
                if self
                    .sources
                    .slice(self.symbol_spans[descriptor.symbol().index()])?
                    != name
                    || !self.callable_has_body(descriptor.symbol())?
                {
                    continue;
                }
                if let Some(candidate) = self.source_candidate(
                    descriptor.symbol(),
                    instance_substitutions.clone(),
                    instance_arguments.clone(),
                    None,
                )? && seen_shapes.insert(self.member_call_shape(&candidate))
                {
                    candidates.push(candidate);
                }
            }
        }
        Ok(candidates)
    }

    fn name_call_candidates(
        &mut self,
        span: Span,
    ) -> Result<Vec<CallCandidate>, TypeCheckingError> {
        let target = self.reference(span, Namespace::Value).cloned();
        match target {
            Some(ReferenceTarget::Symbol(symbol)) => Ok(self
                .source_candidate(symbol, BTreeMap::new(), Vec::new(), None)?
                .into_iter()
                .collect()),
            Some(ReferenceTarget::OverloadSet(symbols)) => symbols
                .into_iter()
                .filter_map(|symbol| {
                    self.source_candidate(symbol, BTreeMap::new(), Vec::new(), None)
                        .transpose()
                })
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
        let receiver_category = self.expression_categories[receiver.index()];
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
                .zip(arguments.iter().copied())
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
                if let Some(candidate) = self.source_candidate(
                    descriptor.symbol(),
                    owner_substitutions.clone(),
                    arguments.clone(),
                    Some((receiver, receiver_type, receiver_category)),
                )? {
                    let shape = self.member_call_shape(&candidate);
                    if seen_shapes.insert(shape) {
                        candidates.push(candidate);
                    }
                }
            }
        }
        Ok(candidates)
    }

    fn member_call_shape(&self, candidate: &CallCandidate) -> (usize, Vec<MemberCallShapeType>) {
        let parameters = candidate
            .type_parameters
            .iter()
            .enumerate()
            .map(|(slot, symbol)| (*symbol, slot))
            .collect::<BTreeMap<_, _>>();
        (
            parameters.len(),
            candidate
                .parameters
                .iter()
                .map(|parameter| self.member_call_shape_type(parameter.ty, &parameters))
                .collect(),
        )
    }

    fn member_call_shape_type(
        &self,
        ty: TypeId,
        parameters: &BTreeMap<SymbolId, usize>,
    ) -> MemberCallShapeType {
        match self.kind(ty) {
            TypeKind::Builtin(builtin) => MemberCallShapeType::Builtin(*builtin),
            TypeKind::Nullable(inner) => MemberCallShapeType::Nullable(Box::new(
                self.member_call_shape_type(*inner, parameters),
            )),
            TypeKind::Function {
                move_only,
                parameters: function_parameters,
                return_type,
            } => MemberCallShapeType::Function(
                *move_only,
                function_parameters
                    .iter()
                    .map(|parameter| self.member_call_shape_type(parameter.ty, parameters))
                    .collect(),
                Box::new(self.member_call_shape_type(*return_type, parameters)),
            ),
            TypeKind::Nominal { nominal, arguments } => MemberCallShapeType::Nominal(
                *nominal,
                arguments
                    .iter()
                    .map(|argument| self.member_call_shape_type(*argument, parameters))
                    .collect(),
            ),
            TypeKind::Intrinsic {
                constructor,
                arguments,
            } => MemberCallShapeType::Intrinsic(
                *constructor,
                arguments
                    .iter()
                    .map(|argument| self.member_call_shape_type(*argument, parameters))
                    .collect(),
            ),
            TypeKind::TypeParameter(symbol) => parameters.get(symbol).copied().map_or(
                MemberCallShapeType::OuterParameter(*symbol),
                MemberCallShapeType::Parameter,
            ),
            TypeKind::StaticSelf(inner) => self.member_call_shape_type(*inner, parameters),
            TypeKind::Capability(capability) => MemberCallShapeType::Capability(*capability),
            TypeKind::EnumCase { root, .. } => self.member_call_shape_type(*root, parameters),
            TypeKind::IntegerLiteral(_) | TypeKind::Deferred(_) | TypeKind::Error => {
                MemberCallShapeType::Other(ty)
            }
        }
    }

    fn source_candidate(
        &mut self,
        symbol: SymbolId,
        substitutions: BTreeMap<SymbolId, TypeId>,
        owner_arguments: Vec<TypeId>,
        explicit_receiver: Option<(ExpressionId, TypeId, ExpressionCategory)>,
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
            parameters.push(MappedParameter {
                name: symbol
                    .map(|symbol| self.sources.slice(self.symbol_spans[symbol.index()]))
                    .transpose()?
                    .map(str::to_owned),
                mode: parameter.mode,
                ty: self.substitute_type(parameter.ty, &substitutions)?,
                span: symbol.map(|symbol| self.symbol_spans[symbol.index()]),
            });
        }
        let receiver = descriptor.receiver().and_then(|receiver| {
            let ty = explicit_receiver
                .map_or_else(|| self.classifiers.last().copied(), |(_, ty, _)| Some(ty))?;
            let origin = explicit_receiver.map_or_else(
                || {
                    self.current_receiver_nominal()
                        .map(CallReceiverOrigin::ImplicitThis)
                },
                |(expression, _, _)| Some(CallReceiverOrigin::Expression(expression)),
            )?;
            Some(CallReceiverDescriptor {
                origin,
                mode: receiver.mode(),
                category: explicit_receiver
                    .map_or(ExpressionCategory::Place, |(_, _, category)| category),
                ty,
            })
        });
        Ok(Some(CallCandidate {
            target: CallableTarget::Source(symbol),
            declaration_span: Some(self.symbol_spans[symbol.index()]),
            type_parameters: descriptor.type_parameters().to_vec(),
            instance_arguments: owner_arguments,
            receiver,
            parameters,
            return_type: self.substitute_type(descriptor.return_type(), &substitutions)?,
            cross_thread_parameters: BTreeSet::new(),
            aborts: false,
            prints_line: false,
        }))
    }

    pub(super) fn current_receiver_nominal(&self) -> Option<NominalId> {
        let mut ty = *self.classifiers.last()?;
        if let TypeKind::StaticSelf(inner) = self.kind(ty) {
            ty = *inner;
        }
        match self.kind(ty) {
            TypeKind::Nominal { nominal, .. } => Some(*nominal),
            _ => None,
        }
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
            .map(|parameter| MappedParameter {
                name: None,
                mode: parameter.mode,
                ty: self.normalize_environment_type(&parameter.ty),
                span: None,
            })
            .collect();
        Ok(Some(CallCandidate {
            target: CallableTarget::External(external),
            declaration_span: None,
            type_parameters: Vec::new(),
            instance_arguments: Vec::new(),
            receiver: None,
            parameters,
            return_type: self.normalize_environment_type(&signature.return_type),
            cross_thread_parameters: signature
                .effects
                .iter()
                .filter_map(|effect| match effect {
                    EnvironmentFunctionEffect::CrossThreadTransfer { parameter } => {
                        Some(*parameter)
                    }
                    EnvironmentFunctionEffect::Abort => None,
                    EnvironmentFunctionEffect::PrintLine => None,
                })
                .collect(),
            aborts: signature
                .effects
                .contains(&EnvironmentFunctionEffect::Abort),
            prints_line: signature
                .effects
                .contains(&EnvironmentFunctionEffect::PrintLine),
        }))
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
            let was_checked = self.expression_types[argument.value.index()].is_some();
            let result =
                self.check_expression(argument.value, Some(parameter.ty), parameter.span)?;
            valid &= !self.is_error(result.ty);
            deferred |= self.is_deferred(result.ty);
            if was_checked
                && !self.is_error(result.ty)
                && !self.is_deferred(result.ty)
                && !self.assignable(result.ty, parameter.ty)
            {
                self.mismatch(
                    self.ast().expressions().get(argument.value)?.span(),
                    parameter.span,
                    result.ty,
                    parameter.ty,
                )?;
                valid = false;
            }
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
        if let Expression::SuperMember { name_span, .. } =
            self.ast().expressions().get(callee)?.payload()
            && candidate.receiver.is_some_and(|receiver| {
                self.current_receiver_mode
                    .is_some_and(|current| !receiver_mode_allows(current, receiver.mode))
            })
        {
            self.emit(
                self.invalid_override_code,
                "current receiver mode cannot satisfy the selected super member contract",
                *name_span,
            )?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
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
                    candidate.parameters[mapping[argument_index]].ty,
                    candidate
                        .cross_thread_parameters
                        .contains(&mapping[argument_index]),
                )
            })
            .collect();
        self.calls.push(CallDescriptor::new(
            expression,
            candidate.target,
            candidate.instance_arguments,
            candidate.return_type,
            candidate.receiver,
            descriptors,
            candidate.aborts,
            candidate.prints_line,
        ));
        Ok(ExprCheck {
            ty: candidate.return_type,
            falls_through: !self.is_builtin(candidate.return_type, BuiltinType::Nothing),
        })
    }

    pub(super) fn classify_expression_category(
        &self,
        expression: ExpressionId,
        ty: TypeId,
    ) -> ExpressionCategory {
        if self
            .associated_constant_uses
            .contains_key(&expression.index())
        {
            return ExpressionCategory::Temporary;
        }
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

    pub(super) fn is_syntactic_place(&self, expression: ExpressionId) -> bool {
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
