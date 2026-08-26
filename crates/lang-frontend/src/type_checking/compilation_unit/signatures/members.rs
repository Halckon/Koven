use std::collections::BTreeMap;

use crate::{
    ast::ItemId,
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
    },
    parser::{
        ClassifierDeclaration, ClassifierKind, DeclarationModifiers, FunctionBody, FunctionForm,
        Item, VariableKind, VisibilityModifier,
    },
    source::Span,
    type_checking::{
        BuiltinType, Capability, IntrinsicTypeConstructor, NominalKind, ParameterMode,
        TypeCheckingError,
    },
};

use super::super::{
    CompilationUnitTypeError, SignatureCollector, UnitCallableSignature, UnitCallableTarget,
    UnitDelegationPlan, UnitTypeId, UnitTypeKind, marker_span, unwrapped_item,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ShapeType {
    Builtin(BuiltinType),
    Nullable(Box<Self>),
    Function(bool, Vec<Self>, Box<Self>),
    Nominal(DeclarationId, Vec<Self>),
    Intrinsic(IntrinsicTypeConstructor, Vec<Self>),
    Parameter(usize),
    OuterParameter(UnitSymbolId),
    Capability(Capability),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct MemberShape {
    name: String,
    generic_arity: usize,
    parameters: Vec<ShapeType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MemberContract {
    modes: Vec<ParameterMode>,
    return_type: ShapeType,
}

#[derive(Clone, Debug)]
struct MemberSignature {
    shape: MemberShape,
    contract: MemberContract,
    name_span: Span,
    modifiers: DeclarationModifiers,
    has_body: bool,
    owner: UnitTypeId,
}

#[derive(Clone, Copy, Debug)]
struct FunctionFacts {
    modifiers: DeclarationModifiers,
    form: FunctionForm,
}

#[derive(Clone, Debug, Default)]
struct DelegationFacts {
    valid: Vec<UnitDelegationPlan>,
    invalid: Vec<UnitTypeId>,
}

impl SignatureCollector<'_> {
    pub(super) fn check_member_interface_contracts(
        &mut self,
    ) -> Result<(), CompilationUnitTypeError> {
        let owners = self.nominals.values().cloned().collect::<Vec<_>>();
        for owner in &owners {
            self.check_concrete_member_bodies(owner.declaration())?;
        }
        let delegations = self.validate_delegations(&owners)?;
        for owner in owners {
            let local = self.members_for_instance(owner.ty())?;
            let mut inherited = Vec::new();
            for interface in owner.interfaces() {
                inherited.extend(self.members_for_instance(*interface)?);
            }
            let inherited = self.unshadowed_members(inherited)?;
            if owner.kind() == NominalKind::Interface {
                self.check_interface_replacements(&local, &inherited)?;
            } else {
                self.check_concrete_implementation(
                    owner.declaration(),
                    &local,
                    &inherited,
                    delegations
                        .get(&owner.declaration())
                        .cloned()
                        .unwrap_or_default(),
                )?;
            }
        }
        Ok(())
    }

    fn check_concrete_member_bodies(
        &mut self,
        owner: DeclarationId,
    ) -> Result<(), CompilationUnitTypeError> {
        let (source, classifier) = self.classifier(owner)?;
        if matches!(classifier.kind, ClassifierKind::Interface { .. }) {
            return Ok(());
        }
        let Some(body) = classifier.body else {
            return Ok(());
        };
        let owner_span = marker_span(classifier.name);
        for member in body.members {
            let Some(facts) = self.function_facts(source, member)? else {
                continue;
            };
            if function_has_body(facts.form) {
                continue;
            }
            let item = unwrapped_item(self.inputs[source.index()].ast(), member)?;
            let Item::Function { name, .. } = item else {
                return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
            };
            self.emit_with_label(
                codes::CONCRETE_MEMBER_BODY,
                "concrete classifier member requires a body",
                marker_span(*name),
                owner_span,
                "concrete classifier declared here",
            )?;
        }
        Ok(())
    }

    fn validate_delegations(
        &mut self,
        owners: &[super::super::UnitNominalSignature],
    ) -> Result<BTreeMap<DeclarationId, DelegationFacts>, CompilationUnitTypeError> {
        let mut result = BTreeMap::new();
        for owner in owners {
            let (source, classifier) = self.classifier(owner.declaration())?;
            for supertype in &classifier.supertypes {
                let Some(delegation) = supertype.delegation else {
                    continue;
                };
                let interface = self
                    .type_ref_types
                    .get(&(source, supertype.type_ref.index()))
                    .copied()
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                let interface_span = self.inputs[source.index()]
                    .ast()
                    .type_refs()
                    .get(supertype.type_ref)
                    .map_err(TypeCheckingError::from)?
                    .span();
                let accepted_edge = self
                    .nominal_declaration(interface)
                    .and_then(|target| {
                        self.interface_edge_spans
                            .get(&(owner.declaration(), target))
                    })
                    .is_some_and(|span| *span == interface_span);
                if self.is_error(interface)
                    || !owner.direct_interfaces().contains(&interface)
                    || !accepted_edge
                {
                    continue;
                }
                let target_span = marker_span(delegation.target);
                let field_symbol = match self.reference(source, target_span, Namespace::Value) {
                    Some(UnitReferenceTarget::Symbol(symbol))
                        if self.names.source_units()[symbol.source_unit().index()]
                            .resolution()
                            .symbols()
                            .get(symbol.symbol().index())
                            .is_some_and(|symbol| symbol.kind() == SymbolKind::Field) =>
                    {
                        Some(*symbol)
                    }
                    _ => None,
                };
                let field = field_symbol.and_then(|symbol| {
                    classifier
                        .primary_constructor
                        .as_ref()?
                        .fields
                        .iter()
                        .find(|field| self.marker_symbol(source, field.name) == Some(symbol))
                });
                let valid_target = matches!(classifier.kind, ClassifierKind::Class { .. })
                    && matches!(field, Some(field) if field.kind == VariableKind::Val);
                if !valid_target {
                    self.emit(
                        codes::INVALID_DELEGATION_TARGET,
                        "delegation target must be an immutable field of the same primary constructor",
                        target_span,
                    )?;
                    result
                        .entry(owner.declaration())
                        .or_insert_with(DelegationFacts::default)
                        .invalid
                        .push(interface);
                    continue;
                }
                let field_type = owner
                    .fields()
                    .iter()
                    .find(|field| Some(field.symbol()) == field_symbol)
                    .map(|field| field.ty())
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                if !self.satisfies_interface(field_type, interface)? {
                    self.emit_with_label(
                        codes::DELEGATE_INTERFACE_MISMATCH,
                        "delegate field type does not satisfy the target interface",
                        target_span,
                        interface_span,
                        "delegated interface declared here",
                    )?;
                    result
                        .entry(owner.declaration())
                        .or_insert_with(DelegationFacts::default)
                        .invalid
                        .push(interface);
                    continue;
                }
                let plan = UnitDelegationPlan::new(
                    owner.declaration(),
                    interface,
                    field_symbol.ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?,
                    delegation.span,
                    delegation.by_span,
                );
                result
                    .entry(owner.declaration())
                    .or_insert_with(DelegationFacts::default)
                    .valid
                    .push(plan);
                self.delegations.push(plan);
            }
        }
        Ok(result)
    }

    fn check_interface_replacements(
        &mut self,
        local: &[MemberSignature],
        inherited: &[MemberSignature],
    ) -> Result<(), CompilationUnitTypeError> {
        for member in local {
            let Some(parent) = inherited.iter().find(|parent| parent.shape == member.shape) else {
                continue;
            };
            if member.contract != parent.contract {
                self.emit_with_label(
                    codes::INTERFACE_MEMBER_MISMATCH,
                    "interface member replacement has a different callable contract",
                    member.name_span,
                    parent.name_span,
                    "inherited member declared here",
                )?;
            }
        }
        Ok(())
    }

    fn check_concrete_implementation(
        &mut self,
        owner: DeclarationId,
        local: &[MemberSignature],
        inherited: &[MemberSignature],
        delegations: DelegationFacts,
    ) -> Result<(), CompilationUnitTypeError> {
        let owner_span = self.names.index().declarations()[owner.index()].name_span();
        let mut by_shape = BTreeMap::<MemberShape, Vec<&MemberSignature>>::new();
        for member in inherited {
            by_shape
                .entry(member.shape.clone())
                .or_default()
                .push(member);
        }
        for member in local {
            let targets = by_shape.remove(&member.shape).unwrap_or_default();
            let public = !matches!(
                member.modifiers.visibility,
                Some(VisibilityModifier::Internal(_) | VisibilityModifier::Private(_))
            );
            let valid_target = !targets.is_empty()
                && targets
                    .iter()
                    .all(|target| target.contract == member.contract);
            if member.modifiers.override_span.is_some() && (!valid_target || !public)
                || member.modifiers.override_span.is_none() && !targets.is_empty()
            {
                self.emit_with_label(
                    codes::INVALID_OVERRIDE,
                    "member override is missing, has no target, or has an incompatible contract",
                    member.modifiers.override_span.unwrap_or(member.name_span),
                    targets
                        .first()
                        .map_or(owner_span, |target| target.name_span),
                    "related interface member or owner declared here",
                )?;
            }
        }
        for sources in by_shape.into_values() {
            let mut active = Vec::new();
            for source in sources {
                let mut poisoned = false;
                for interface in &delegations.invalid {
                    poisoned |= self.satisfies_interface(*interface, source.owner)?;
                }
                if !poisoned {
                    active.push(source);
                }
            }
            if active.is_empty() {
                continue;
            }
            let mut providing_delegates = Vec::new();
            for plan in &delegations.valid {
                let mut provides = false;
                for source in &active {
                    provides |= self.satisfies_interface(plan.interface(), source.owner)?;
                }
                if provides {
                    providing_delegates.push(*plan);
                }
            }
            let mut foreign_defaults = Vec::new();
            for source in &active {
                if !source.has_body {
                    continue;
                }
                let mut covered = false;
                for plan in &providing_delegates {
                    covered |= self.satisfies_interface(plan.interface(), source.owner)?;
                }
                if !covered {
                    foreign_defaults.push(*source);
                }
            }
            if providing_delegates.len() > 1
                || !providing_delegates.is_empty() && !foreign_defaults.is_empty()
            {
                let primary = providing_delegates
                    .last()
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
                    .by_span();
                let previous = if providing_delegates.len() > 1 {
                    providing_delegates[providing_delegates.len() - 2].by_span()
                } else {
                    foreign_defaults[0].name_span
                };
                let requirement = active
                    .iter()
                    .find(|source| !source.has_body)
                    .unwrap_or(&active[0])
                    .name_span;
                self.emit_delegation_conflict(primary, previous, requirement)?;
                continue;
            }
            if providing_delegates.len() == 1 {
                continue;
            }
            let defaults = active.iter().filter(|source| source.has_body).count();
            if defaults > 1 {
                self.emit_with_label(
                    codes::DEFAULT_MEMBER_CONFLICT,
                    "multiple interface defaults require an explicit override",
                    owner_span,
                    active[1].name_span,
                    "conflicting default declared here",
                )?;
            } else if defaults == 0 {
                self.emit_with_label(
                    codes::MISSING_INTERFACE_MEMBER,
                    "concrete classifier does not implement an abstract interface member",
                    owner_span,
                    active[0].name_span,
                    "required member declared here",
                )?;
            }
        }
        Ok(())
    }

    fn emit_delegation_conflict(
        &mut self,
        primary: Span,
        previous: Span,
        requirement: Span,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(codes::DELEGATION_MEMBER_CONFLICT)?;
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            "delegation and inherited implementations require an explicit override",
            primary,
        )?;
        diagnostic.add_label(
            self.sources,
            previous,
            "previous delegated or default implementation source is here",
        )?;
        diagnostic.add_label(
            self.sources,
            requirement,
            "conflicting interface member requirement declared here",
        )?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn unshadowed_members(
        &mut self,
        members: Vec<MemberSignature>,
    ) -> Result<Vec<MemberSignature>, CompilationUnitTypeError> {
        let mut shadowed = vec![false; members.len()];
        for ancestor in 0..members.len() {
            for descendant in 0..members.len() {
                if ancestor == descendant || members[ancestor].shape != members[descendant].shape {
                    continue;
                }
                if self.satisfies_interface(members[descendant].owner, members[ancestor].owner)? {
                    shadowed[ancestor] = true;
                    break;
                }
            }
        }
        Ok(members
            .into_iter()
            .zip(shadowed)
            .filter_map(|(member, shadowed)| (!shadowed).then_some(member))
            .collect())
    }

    fn members_for_instance(
        &mut self,
        instance: UnitTypeId,
    ) -> Result<Vec<MemberSignature>, CompilationUnitTypeError> {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.types.get(instance).cloned()
        else {
            return Ok(Vec::new());
        };
        let nominal = self
            .nominals
            .get(&declaration)
            .cloned()
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        let substitutions = nominal
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments)
            .collect::<BTreeMap<_, _>>();
        let (source, classifier) = self.classifier(declaration)?;
        let Some(body) = classifier.body else {
            return Ok(Vec::new());
        };
        let mut result = Vec::new();
        for item in body.members {
            let Some(facts) = self.function_facts(source, item)? else {
                continue;
            };
            let symbol = self
                .item_symbol(source, item)?
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            let callable = nominal
                .members()
                .iter()
                .find(|member| member.target() == UnitCallableTarget::Symbol(symbol))
                .cloned()
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            if let Some(member) =
                self.member_signature(&callable, facts, instance, &substitutions)?
            {
                result.push(member);
            }
        }
        Ok(result)
    }

    fn member_signature(
        &mut self,
        callable: &UnitCallableSignature,
        facts: FunctionFacts,
        owner: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    ) -> Result<Option<MemberSignature>, CompilationUnitTypeError> {
        let slots = callable
            .type_parameters()
            .iter()
            .enumerate()
            .map(|(slot, symbol)| (*symbol, slot))
            .collect::<BTreeMap<_, _>>();
        let mut parameters = Vec::with_capacity(callable.parameters().len());
        let mut modes = Vec::with_capacity(callable.parameters().len());
        for parameter in callable.parameters() {
            let ty = self.substitute_type(parameter.ty(), substitutions)?;
            let Some(shape) = self.shape_type(ty, &slots) else {
                return Ok(None);
            };
            parameters.push(shape);
            modes.push(parameter.mode());
        }
        let return_type = self.substitute_type(callable.return_type(), substitutions)?;
        let Some(return_type) = self.shape_type(return_type, &slots) else {
            return Ok(None);
        };
        Ok(Some(MemberSignature {
            shape: MemberShape {
                name: callable.name().to_owned(),
                generic_arity: callable.type_parameters().len(),
                parameters,
            },
            contract: MemberContract { modes, return_type },
            name_span: callable.name_span(),
            modifiers: facts.modifiers,
            has_body: function_has_body(facts.form),
            owner,
        }))
    }

    fn shape_type(
        &self,
        ty: UnitTypeId,
        parameters: &BTreeMap<UnitSymbolId, usize>,
    ) -> Option<ShapeType> {
        match self.types.get(ty) {
            Some(UnitTypeKind::Builtin(builtin)) => Some(ShapeType::Builtin(*builtin)),
            Some(UnitTypeKind::Nullable(inner)) => Some(ShapeType::Nullable(Box::new(
                self.shape_type(*inner, parameters)?,
            ))),
            Some(UnitTypeKind::Function {
                move_only,
                parameters: function_parameters,
                return_type,
            }) => Some(ShapeType::Function(
                *move_only,
                function_parameters
                    .iter()
                    .map(|parameter| self.shape_type(parameter.ty(), parameters))
                    .collect::<Option<Vec<_>>>()?,
                Box::new(self.shape_type(*return_type, parameters)?),
            )),
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => Some(ShapeType::Nominal(
                *declaration,
                arguments
                    .iter()
                    .map(|argument| self.shape_type(*argument, parameters))
                    .collect::<Option<Vec<_>>>()?,
            )),
            Some(UnitTypeKind::Intrinsic {
                constructor,
                arguments,
            }) => Some(ShapeType::Intrinsic(
                *constructor,
                arguments
                    .iter()
                    .map(|argument| self.shape_type(*argument, parameters))
                    .collect::<Option<Vec<_>>>()?,
            )),
            Some(UnitTypeKind::TypeParameter(symbol)) => Some(
                parameters
                    .get(symbol)
                    .copied()
                    .map_or(ShapeType::OuterParameter(*symbol), ShapeType::Parameter),
            ),
            Some(UnitTypeKind::StaticSelf(interface)) => self.shape_type(*interface, parameters),
            Some(UnitTypeKind::Capability(capability)) => Some(ShapeType::Capability(*capability)),
            Some(UnitTypeKind::EnumCase { root, .. }) => self.shape_type(*root, parameters),
            Some(UnitTypeKind::Deferred(_)) | Some(UnitTypeKind::Error) | None => None,
        }
    }

    fn classifier(
        &self,
        declaration: DeclarationId,
    ) -> Result<(SourceUnitId, ClassifierDeclaration), CompilationUnitTypeError> {
        let declaration = &self.names.index().declarations()[declaration.index()];
        let source = declaration.source_unit();
        let item = unwrapped_item(self.inputs[source.index()].ast(), declaration.root())?;
        let Item::Classifier(classifier) = item else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        Ok((source, classifier.as_ref().clone()))
    }

    fn function_facts(
        &self,
        source: SourceUnitId,
        item: ItemId,
    ) -> Result<Option<FunctionFacts>, CompilationUnitTypeError> {
        match self.inputs[source.index()]
            .ast()
            .items()
            .get(item)
            .map_err(TypeCheckingError::from)?
            .payload()
        {
            Item::Modified {
                modifiers,
                declaration,
            } => {
                let Some(facts) = self.function_facts(source, *declaration)? else {
                    return Ok(None);
                };
                Ok(Some(FunctionFacts {
                    modifiers: *modifiers,
                    ..facts
                }))
            }
            Item::Function { form, .. } => Ok(Some(FunctionFacts {
                modifiers: DeclarationModifiers::default(),
                form: *form,
            })),
            _ => Ok(None),
        }
    }
}

const fn function_has_body(form: FunctionForm) -> bool {
    match form {
        FunctionForm::ImplicitUnitAbsent => false,
        FunctionForm::ImplicitUnitBlock(_) => true,
        FunctionForm::Explicit { body, .. } => !matches!(body, FunctionBody::Absent),
    }
}
