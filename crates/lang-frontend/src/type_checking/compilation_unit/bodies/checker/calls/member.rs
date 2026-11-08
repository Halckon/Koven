//! SPEC-0197 compilation-unit source member callable 选择。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::{
        DeclarationId, DeclarationVisibility, Namespace, SourceUnitId, UnitReferenceTarget,
        UnitSymbolId,
    },
    source::Span,
    type_checking::{
        BuiltinType, Capability, CompilationUnitTypeError, IntrinsicTypeConstructor, NominalKind,
        TypeCheckingError, UnitCallTarget, UnitCallableSignature, UnitCallableTarget,
        UnitNominalSignature, UnitTypeId, UnitTypeKind,
    },
};

use super::{super::BodyChecker, CallCandidate, candidate::MemberOwner};

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
        if self.current_receiver.is_some_and(|ty| self.is_error(ty)) {
            return Ok(Vec::new());
        }
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
        let checkpoint = self.signatures.types().len();
        let inherited = self.satisfies_interface(receiver, interface);
        self.signatures.types_mut().discard_after(checkpoint);
        if !inherited? {
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
        let instances = self.member_owners(interface, &nominal, arguments);
        let mut candidates = Vec::new();
        let mut seen_shapes = BTreeSet::new();
        for (owner, instance) in instances {
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
                let mut candidate = self.source_member_candidate(callable, instance.clone())?;
                if let Some((mode, _)) = candidate.receiver {
                    candidate.receiver = Some((mode, receiver));
                }
                if self.keep_member_candidate(&candidate, &mut seen_shapes)? {
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
            let mut candidate = self.source_member_candidate(
                &callable,
                MemberOwner::Direct {
                    declaration: nominal.declaration(),
                    arguments: owner_arguments,
                },
            )?;
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
        if self.signatures.types().get(receiver_type)
            == Some(&UnitTypeKind::Builtin(BuiltinType::String))
        {
            return self.text_extension_candidates(source, receiver_type, name_span);
        }
        let Some((owner_declaration, owner_arguments)) = self.nominal_type_parts(receiver_type)
        else {
            return self.range_extension_candidates(source, receiver_type, name_span);
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
        let instances = self.member_owners(receiver_type, &owner, owner_arguments);
        let mut candidates = Vec::new();
        let mut seen_shapes = BTreeSet::new();
        for (declaration, instance) in instances {
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
                let mut candidate = self.source_member_candidate(callable, instance.clone())?;
                if let Some((mode, _)) = candidate.receiver {
                    candidate.receiver = Some((mode, receiver_type));
                }
                if self.keep_member_candidate(&candidate, &mut seen_shapes)? {
                    candidates.push(candidate);
                }
            }
        }
        Ok(candidates)
    }

    fn member_owners(
        &self,
        receiver: UnitTypeId,
        nominal: &UnitNominalSignature,
        arguments: Vec<UnitTypeId>,
    ) -> Vec<(DeclarationId, MemberOwner)> {
        let declaration = nominal.declaration();
        let direct = (
            declaration,
            MemberOwner::Direct {
                declaration,
                arguments,
            },
        );
        std::iter::once(direct)
            .chain(nominal.interfaces().iter().filter_map(|&template| {
                let (declaration, _) = self.nominal_type_parts(template)?;
                Some((declaration, MemberOwner::Inherited { receiver, template }))
            }))
            .collect()
    }

    fn source_member_candidate(
        &self,
        callable: &UnitCallableSignature,
        owner: MemberOwner,
    ) -> Result<CallCandidate, CompilationUnitTypeError> {
        let UnitCallableTarget::Symbol(symbol) = callable.target() else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        let mut candidate = CallCandidate::from_source(UnitCallTarget::Symbol(symbol), callable);
        if !matches!(&owner, MemberOwner::Direct { arguments, .. } if arguments.is_empty()) {
            candidate.member_owner = Some(owner);
        }
        Ok(candidate)
    }

    /// Resolve the owner only within the caller's type-table checkpoint.
    pub(super) fn specialize_member_owner(
        &mut self,
        candidate: &mut CallCandidate,
    ) -> Result<BTreeMap<UnitSymbolId, UnitTypeId>, CompilationUnitTypeError> {
        let Some(owner) = candidate.member_owner.take() else {
            return Ok(BTreeMap::new());
        };
        let (declaration, arguments) = match owner {
            MemberOwner::Direct {
                declaration,
                arguments,
            } => (declaration, arguments),
            MemberOwner::Inherited { receiver, template } => {
                let (declaration, arguments) = self
                    .nominal_type_parts(receiver)
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                let receiver_substitutions =
                    self.member_owner_substitutions(declaration, &arguments)?;
                let owner = self.substitute_type(template, &receiver_substitutions)?;
                self.nominal_type_parts(owner)
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
            }
        };
        let substitutions = self.member_owner_substitutions(declaration, &arguments)?;
        candidate.instance_arguments = arguments;
        Ok(substitutions)
    }

    fn member_owner_substitutions(
        &self,
        declaration: DeclarationId,
        arguments: &[UnitTypeId],
    ) -> Result<BTreeMap<UnitSymbolId, UnitTypeId>, CompilationUnitTypeError> {
        let owner = self
            .signatures
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        Ok(owner
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments.iter().copied())
            .collect())
    }

    fn keep_member_candidate(
        &mut self,
        candidate: &CallCandidate,
        seen_shapes: &mut BTreeSet<(usize, Vec<MemberCallShapeType>)>,
    ) -> Result<bool, CompilationUnitTypeError> {
        if candidate.member_owner.is_none() {
            return Ok(seen_shapes.insert(self.member_call_shape(candidate)));
        }
        // Compare owner-specialized shapes without publishing their temporary type IDs.
        let checkpoint = self.signatures.types().len();
        let mut specialized = candidate.clone();
        let result = self
            .specialize_member_owner(&mut specialized)
            .and_then(|substitutions| {
                self.substitute_candidate_types(&mut specialized, &substitutions)
            })
            .map(|failure| {
                // Keep invalid candidates for the existing storage diagnostic path.
                failure.is_some() || seen_shapes.insert(self.member_call_shape(&specialized))
            });
        self.signatures.types_mut().discard_after(checkpoint);
        result
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
}
