//! SPEC-0197 compilation-unit source member callable 与自动结构分量选择。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::{
        DeclarationId, DeclarationVisibility, Namespace, SourceUnitId, UnitReferenceTarget,
        UnitSymbolId,
    },
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        BuiltinType, Capability, CompilationUnitTypeError, ExpressionCategory,
        IntrinsicTypeConstructor, NominalKind, TypeCheckingError,
        UnitAggregateProjectionDescriptor, UnitAggregateProjectionKind,
        UnitAggregateProjectionReceiver, UnitCallDescriptor, UnitCallReceiverDescriptor,
        UnitCallReceiverOrigin, UnitCallTarget, UnitCallableInstanceKey, UnitCallableSignature,
        UnitCallableTarget, UnitExpressionId, UnitNominalSignature, UnitTypeId, UnitTypeKind,
    },
};

use super::{super::BodyChecker, super::ExpressionCheck, CallCandidate};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MemberCallShapeType {
    Builtin(BuiltinType),
    Nullable(Box<Self>),
    Function(bool, Vec<Self>, Box<Self>),
    Nominal(DeclarationId, Vec<Self>),
    Intrinsic(IntrinsicTypeConstructor, Vec<Self>),
    Parameter(usize),
    OuterParameter(UnitSymbolId),
    Capability(Capability),
    Other(UnitTypeId),
}

impl BodyChecker<'_> {
    pub(super) fn super_call_candidates(
        &mut self,
        source: SourceUnitId,
        interface: TypeRefId,
        name_span: Span,
    ) -> Result<Vec<CallCandidate>, CompilationUnitTypeError> {
        let interface = self.resolve_static_body_type_ref(source, interface)?;
        let Some((declaration, arguments)) = self.nominal_type_parts(interface) else {
            return Ok(Vec::new());
        };
        let nominal = self
            .signatures
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
            .cloned()
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        if nominal.kind() != NominalKind::Interface {
            self.emit(
                crate::diagnostic::codes::INTERFACE_MEMBER_MISMATCH,
                "super qualifier must name an inherited interface",
                name_span,
            )?;
            return Ok(Vec::new());
        }
        let receiver = self
            .current_receiver
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        let inherited = self.satisfies_interface(receiver, interface)?;
        if !inherited {
            self.emit(
                crate::diagnostic::codes::INTERFACE_MEMBER_MISMATCH,
                "super qualifier must name an inherited interface",
                name_span,
            )?;
            return Ok(Vec::new());
        }
        let name = self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?;
        let substitutions = nominal
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments.iter().copied())
            .collect::<BTreeMap<_, _>>();
        let mut instances = vec![(declaration, arguments)];
        for inherited in nominal.interfaces() {
            let inherited = self.substitute_type(*inherited, &substitutions)?;
            if let Some(instance) = self.nominal_type_parts(inherited) {
                instances.push(instance);
            }
        }
        let mut candidates = Vec::new();
        let mut seen_shapes = BTreeSet::new();
        for (owner, arguments) in instances {
            let owner = self
                .signatures
                .declaration(owner)
                .and_then(|signature| signature.nominal())
                .cloned()
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            for callable in owner
                .members()
                .iter()
                .filter(|callable| callable.name() == name && callable.has_body())
            {
                let mut candidate =
                    self.source_member_candidate(callable, &owner, arguments.clone())?;
                if let Some((mode, _)) = candidate.receiver {
                    candidate.receiver = Some((mode, receiver));
                }
                if seen_shapes.insert(self.member_call_shape(&candidate)) {
                    candidates.push(candidate);
                }
            }
        }
        Ok(candidates)
    }

    pub(super) fn symbol_candidate(
        &mut self,
        symbol: UnitSymbolId,
    ) -> Result<Option<CallCandidate>, CompilationUnitTypeError> {
        let declarations = self.signatures.declarations().to_vec();
        for declaration in declarations {
            let Some(nominal) = declaration.nominal() else {
                continue;
            };
            if let Some(callable) = nominal
                .companion_members()
                .iter()
                .find(|callable| callable.target() == UnitCallableTarget::Symbol(symbol))
                .filter(|callable| self.member_visible(callable, nominal.declaration()))
            {
                return Ok(Some(CallCandidate::from_source(
                    UnitCallTarget::Symbol(symbol),
                    callable,
                )));
            }
            let Some(callable) = nominal
                .members()
                .iter()
                .find(|callable| callable.target() == UnitCallableTarget::Symbol(symbol))
                .filter(|callable| self.member_visible(callable, nominal.declaration()))
                .cloned()
            else {
                continue;
            };
            let owner_arguments = self
                .current_receiver
                .and_then(|receiver| self.nominal_type_parts(receiver))
                .filter(|(owner, _)| *owner == nominal.declaration())
                .map(|(_, arguments)| arguments)
                .unwrap_or_else(|| {
                    nominal
                        .type_parameters()
                        .iter()
                        .filter_map(|parameter| self.signatures.symbol_type(*parameter))
                        .collect()
                });
            let mut candidate =
                self.source_member_candidate(&callable, nominal, owner_arguments)?;
            if let Some((mode, _)) = candidate.receiver {
                let receiver = self
                    .current_receiver
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                candidate.receiver = Some((mode, receiver));
            }
            return Ok(Some(candidate));
        }
        Ok(None)
    }

    pub(super) fn member_call_candidates(
        &mut self,
        source: SourceUnitId,
        receiver: ExpressionId,
        name_span: Span,
        return_type: UnitTypeId,
    ) -> Result<Vec<CallCandidate>, CompilationUnitTypeError> {
        let receiver_span = self
            .file(source)
            .ast()
            .expressions()
            .get(receiver)
            .map_err(TypeCheckingError::from)?
            .span();
        let static_owner = match self.reference(source, receiver_span, Namespace::Type) {
            Some(UnitReferenceTarget::Declaration(declaration)) => Some(*declaration),
            _ => None,
        };
        let receiver_type = self
            .check_expression(source, receiver, None, None, return_type)?
            .ty;
        let Some((owner_declaration, owner_arguments)) = self.nominal_type_parts(receiver_type)
        else {
            return Ok(Vec::new());
        };
        let owner = self
            .signatures
            .declaration(owner_declaration)
            .and_then(|signature| signature.nominal())
            .cloned()
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        let name = self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?;
        if static_owner == Some(owner_declaration) && owner.kind() != NominalKind::Object {
            return Ok(owner
                .companion_members()
                .iter()
                .filter(|callable| callable.name() == name)
                .filter(|callable| self.member_visible(callable, owner.declaration()))
                .map(|callable| {
                    let UnitCallableTarget::Symbol(symbol) = callable.target() else {
                        unreachable!("companion callable signatures use source symbols")
                    };
                    CallCandidate::from_source(UnitCallTarget::Symbol(symbol), callable)
                })
                .collect());
        }
        let substitutions = owner
            .type_parameters()
            .iter()
            .copied()
            .zip(owner_arguments.iter().copied())
            .collect::<BTreeMap<_, _>>();
        let mut instances = vec![(owner_declaration, owner_arguments)];
        for interface in owner.interfaces() {
            let interface = self.substitute_type(*interface, &substitutions)?;
            if let Some(instance) = self.nominal_type_parts(interface) {
                instances.push(instance);
            }
        }
        let mut candidates = Vec::new();
        let mut seen_shapes = BTreeSet::new();
        for (declaration, arguments) in instances {
            let nominal = self
                .signatures
                .declaration(declaration)
                .and_then(|signature| signature.nominal())
                .cloned()
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            let callables = nominal
                .members()
                .iter()
                .filter(|callable| callable.name() == name)
                .filter(|callable| self.member_visible(callable, nominal.declaration()))
                .cloned()
                .collect::<Vec<_>>();
            for callable in &callables {
                let mut candidate =
                    self.source_member_candidate(callable, &nominal, arguments.clone())?;
                if let Some((mode, _)) = candidate.receiver {
                    candidate.receiver = Some((mode, receiver_type));
                }
                let shape = self.member_call_shape(&candidate);
                if seen_shapes.insert(shape) {
                    candidates.push(candidate);
                }
            }
        }
        Ok(candidates)
    }

    fn source_member_candidate(
        &mut self,
        callable: &UnitCallableSignature,
        owner: &UnitNominalSignature,
        owner_arguments: Vec<UnitTypeId>,
    ) -> Result<CallCandidate, CompilationUnitTypeError> {
        let UnitCallableTarget::Symbol(symbol) = callable.target() else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        let substitutions = owner
            .type_parameters()
            .iter()
            .copied()
            .zip(owner_arguments.iter().copied())
            .collect::<BTreeMap<_, _>>();
        let mut candidate = CallCandidate::from_source(UnitCallTarget::Symbol(symbol), callable);
        for parameter in &mut candidate.parameters {
            parameter.ty = self.substitute_type(parameter.ty, &substitutions)?;
        }
        if let Some((mode, ty)) = candidate.receiver {
            candidate.receiver = Some((mode, self.substitute_type(ty, &substitutions)?));
        }
        candidate.return_type = self.substitute_type(candidate.return_type, &substitutions)?;
        candidate.instance_arguments = owner_arguments;
        candidate.owner_substitutions = substitutions;
        Ok(candidate)
    }

    fn member_visible(&self, callable: &UnitCallableSignature, owner: DeclarationId) -> bool {
        callable.visibility() != DeclarationVisibility::Private || self.current_owner == Some(owner)
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
        ty: UnitTypeId,
        parameters: &BTreeMap<UnitSymbolId, usize>,
    ) -> MemberCallShapeType {
        match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Builtin(builtin)) => MemberCallShapeType::Builtin(*builtin),
            Some(UnitTypeKind::Nullable(inner)) => MemberCallShapeType::Nullable(Box::new(
                self.member_call_shape_type(*inner, parameters),
            )),
            Some(UnitTypeKind::Function {
                move_only,
                parameters: function_parameters,
                return_type,
            }) => MemberCallShapeType::Function(
                *move_only,
                function_parameters
                    .iter()
                    .map(|parameter| self.member_call_shape_type(parameter.ty(), parameters))
                    .collect(),
                Box::new(self.member_call_shape_type(*return_type, parameters)),
            ),
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => MemberCallShapeType::Nominal(
                *declaration,
                arguments
                    .iter()
                    .map(|argument| self.member_call_shape_type(*argument, parameters))
                    .collect(),
            ),
            Some(UnitTypeKind::Intrinsic {
                constructor,
                arguments,
            }) => MemberCallShapeType::Intrinsic(
                *constructor,
                arguments
                    .iter()
                    .map(|argument| self.member_call_shape_type(*argument, parameters))
                    .collect(),
            ),
            Some(UnitTypeKind::TypeParameter(symbol)) => parameters.get(symbol).copied().map_or(
                MemberCallShapeType::OuterParameter(*symbol),
                MemberCallShapeType::Parameter,
            ),
            Some(UnitTypeKind::StaticSelf(inner)) => {
                self.member_call_shape_type(*inner, parameters)
            }
            Some(UnitTypeKind::Capability(capability)) => {
                MemberCallShapeType::Capability(*capability)
            }
            Some(UnitTypeKind::EnumCase { root, .. }) => {
                self.member_call_shape_type(*root, parameters)
            }
            Some(UnitTypeKind::IntegerLiteral(_))
            | Some(UnitTypeKind::Deferred(_))
            | Some(UnitTypeKind::Error)
            | None => MemberCallShapeType::Other(ty),
        }
    }

    fn nominal_type_parts(&self, ty: UnitTypeId) -> Option<(DeclarationId, Vec<UnitTypeId>)> {
        let ty = match self.signatures.types().get(ty)? {
            UnitTypeKind::StaticSelf(inner) => *inner,
            _ => ty,
        };
        let UnitTypeKind::Nominal {
            declaration,
            arguments,
        } = self.signatures.types().get(ty)?
        else {
            return None;
        };
        Some((*declaration, arguments.clone()))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_structural_component_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        if !type_arguments.is_empty() || !arguments.is_empty() {
            return Ok(None);
        }
        let Expression::Member {
            receiver,
            name_span,
            safe: false,
            ..
        } = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?
            .payload()
            .clone()
        else {
            return Ok(None);
        };
        let name = self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?;
        let Some(component) = parse_component_name(name) else {
            return Ok(None);
        };
        let receiver_result = self.check_expression(source, receiver, None, None, return_type)?;
        let Some((declaration, owner_arguments)) = self.nominal_type_parts(receiver_result.ty)
        else {
            return Ok(None);
        };
        let owner = self
            .signatures
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
            .cloned()
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        if owner.kind() != NominalKind::ValueClass {
            return Ok(None);
        }
        let Some(field) = owner.fields().get(component - 1).cloned() else {
            return Ok(None);
        };
        let substitutions = owner
            .type_parameters()
            .iter()
            .copied()
            .zip(owner_arguments)
            .collect::<BTreeMap<_, _>>();
        let ty = self.substitute_type(field.ty(), &substitutions)?;
        let function_type = self.signatures.types_mut().intern(UnitTypeKind::Function {
            move_only: false,
            parameters: Vec::new(),
            return_type: ty,
        });
        let callee = UnitExpressionId::new(source, callee);
        self.parts.expression_types.insert(callee, function_type);
        self.parts
            .expression_categories
            .insert(callee, ExpressionCategory::Temporary);
        self.parts.calls.push(UnitCallDescriptor {
            expression: UnitExpressionId::new(source, expression),
            instance: UnitCallableInstanceKey {
                target: UnitCallTarget::StructuralComponent(field.symbol()),
                type_arguments: Vec::new(),
            },
            return_type: ty,
            receiver: Some(UnitCallReceiverDescriptor {
                origin: UnitCallReceiverOrigin::Expression(UnitExpressionId::new(source, receiver)),
                mode: crate::type_checking::ParameterMode::Borrow,
                category: self.expression_category(source, receiver),
                ty: receiver_result.ty,
            }),
            arguments: Vec::new(),
            aborts: false,
            prints_line: false,
        });
        self.parts
            .aggregate_projections
            .push(UnitAggregateProjectionDescriptor::new(
                UnitExpressionId::new(source, expression),
                UnitAggregateProjectionReceiver::Expression(UnitExpressionId::new(
                    source, receiver,
                )),
                field.symbol(),
                ty,
                UnitAggregateProjectionKind::StructuralComponent,
            ));
        Ok(Some(ExpressionCheck {
            ty,
            falls_through: receiver_result.falls_through,
        }))
    }
}

fn parse_component_name(name: &str) -> Option<usize> {
    let digits = name.strip_prefix("component")?;
    if digits.is_empty() || digits.starts_with('0') {
        return None;
    }
    let component = digits.parse::<usize>().ok()?;
    (component > 0 && component.to_string() == digits).then_some(component)
}

#[cfg(test)]
mod tests {
    use super::parse_component_name;

    #[test]
    fn component_names_are_positive_canonical_decimal_indices() {
        assert_eq!(parse_component_name("component1"), Some(1));
        assert_eq!(parse_component_name("component42"), Some(42));
        for invalid in ["component", "component0", "component01", "componentA", "x1"] {
            assert_eq!(parse_component_name(invalid), None, "{invalid}");
        }
    }
}
