use crate::parser::{
    DeclarationModifiers, FunctionBody, FunctionForm, TypeParameter, ValueParameter,
    VisibilityModifier,
};

use super::*;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ShapeType {
    Builtin(BuiltinType),
    Nullable(Box<ShapeType>),
    Function {
        move_only: bool,
        parameters: Vec<ShapeType>,
        return_type: Box<ShapeType>,
    },
    Nominal(NominalId, Vec<ShapeType>),
    Intrinsic(IntrinsicTypeConstructor, Vec<ShapeType>),
    TypeParameter(usize),
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

#[derive(Clone)]
struct MemberSignature {
    shape: MemberShape,
    contract: MemberContract,
    name_span: Span,
    modifiers: DeclarationModifiers,
    has_body: bool,
    owner: TypeId,
}

struct FunctionParts {
    modifiers: DeclarationModifiers,
    name: NameMarker,
    type_parameters: Vec<TypeParameter>,
    parameters: Vec<ValueParameter>,
    form: FunctionForm,
}

impl Checker<'_> {
    pub(super) fn check_callable_shapes_and_bodies(&mut self) -> Result<(), TypeCheckingError> {
        self.check_duplicate_callable_shapes()?;
        self.check_concrete_member_bodies()?;
        self.check_interface_member_contracts()
    }

    fn check_duplicate_callable_shapes(&mut self) -> Result<(), TypeCheckingError> {
        let functions = self
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Item::Function {
                    name,
                    type_parameters,
                    parameters,
                    ..
                } => Some((*name, type_parameters.clone(), parameters.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut first_by_shape = BTreeMap::new();
        for (name, type_parameters, parameters) in functions {
            let NameMarker::Present(name_span) = name else {
                continue;
            };
            let Some(symbol) = self.symbol_at(name_span) else {
                continue;
            };
            let parameter_slots = type_parameters
                .iter()
                .enumerate()
                .filter_map(|(slot, parameter)| match parameter.name {
                    NameMarker::Present(span) => self.symbol_at(span).map(|symbol| (symbol, slot)),
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>();
            let mut shape_parameters = Vec::with_capacity(parameters.len());
            let mut poisoned = false;
            for parameter in parameters {
                let ty = self.resolve_type_ref(parameter.type_ref)?;
                let Some(shape) = self.shape_type(ty, &parameter_slots) else {
                    poisoned = true;
                    break;
                };
                shape_parameters.push(shape);
            }
            if poisoned {
                continue;
            }
            let name_text = self.sources.slice(name_span)?.to_owned();
            let key = (
                self.symbol_scopes[symbol.index()],
                name_text,
                type_parameters.len(),
                shape_parameters,
            );
            if let Some(first) = first_by_shape.get(&key).copied() {
                self.emit_with_label(
                    self.duplicate_callable_shape_code,
                    "callable has a duplicate overload shape",
                    name_span,
                    first,
                    "first callable with this shape declared here",
                )?;
            } else {
                first_by_shape.insert(key, name_span);
            }
        }
        Ok(())
    }

    fn check_concrete_member_bodies(&mut self) -> Result<(), TypeCheckingError> {
        let classifiers = self
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Item::Classifier(classifier) => Some(classifier.as_ref().clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for classifier in classifiers {
            if matches!(classifier.kind, ClassifierKind::Interface { .. }) {
                continue;
            }
            let Some(body) = classifier.body else {
                continue;
            };
            let owner_span = match classifier.name {
                NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => {
                    span
                }
            };
            for member in body.members {
                let function = self.function_payload(member)?;
                let Some((name, form)) = function else {
                    continue;
                };
                if function_has_body(form) {
                    continue;
                }
                let primary = match name {
                    NameMarker::Present(span)
                    | NameMarker::Missing(span)
                    | NameMarker::Error(span) => span,
                };
                self.emit_with_label(
                    self.concrete_member_body_code,
                    "concrete classifier member requires a body",
                    primary,
                    owner_span,
                    "concrete classifier declared here",
                )?;
            }
        }
        Ok(())
    }

    fn function_payload(
        &self,
        id: ItemId,
    ) -> Result<Option<(NameMarker, FunctionForm)>, TypeCheckingError> {
        match self.ast().items().get(id)?.payload() {
            Item::Modified { declaration, .. } => self.function_payload(*declaration),
            Item::Function { name, form, .. } => Ok(Some((*name, *form))),
            _ => Ok(None),
        }
    }

    fn shape_type(
        &self,
        ty: TypeId,
        parameter_slots: &BTreeMap<SymbolId, usize>,
    ) -> Option<ShapeType> {
        match self.kind(ty) {
            TypeKind::Builtin(builtin) => Some(ShapeType::Builtin(*builtin)),
            TypeKind::Nullable(inner) => Some(ShapeType::Nullable(Box::new(
                self.shape_type(*inner, parameter_slots)?,
            ))),
            TypeKind::Function {
                move_only,
                parameters,
                return_type,
            } => Some(ShapeType::Function {
                move_only: *move_only,
                parameters: parameters
                    .iter()
                    .map(|parameter| self.shape_type(parameter.ty, parameter_slots))
                    .collect::<Option<Vec<_>>>()?,
                return_type: Box::new(self.shape_type(*return_type, parameter_slots)?),
            }),
            TypeKind::Nominal { nominal, arguments } => Some(ShapeType::Nominal(
                *nominal,
                arguments
                    .iter()
                    .map(|argument| self.shape_type(*argument, parameter_slots))
                    .collect::<Option<Vec<_>>>()?,
            )),
            TypeKind::Intrinsic {
                constructor,
                arguments,
            } => Some(ShapeType::Intrinsic(
                *constructor,
                arguments
                    .iter()
                    .map(|argument| self.shape_type(*argument, parameter_slots))
                    .collect::<Option<Vec<_>>>()?,
            )),
            TypeKind::TypeParameter(symbol) => parameter_slots
                .get(symbol)
                .copied()
                .map(ShapeType::TypeParameter),
            TypeKind::StaticSelf(interface) => self.shape_type(*interface, parameter_slots),
            TypeKind::Capability(capability) => Some(ShapeType::Capability(*capability)),
            TypeKind::EnumCase { .. }
            | TypeKind::IntegerLiteral(_)
            | TypeKind::Error
            | TypeKind::Deferred(_) => None,
        }
    }

    fn check_interface_member_contracts(&mut self) -> Result<(), TypeCheckingError> {
        let owners = self.nominals.clone();
        for owner in owners {
            let owner_type = self
                .symbol_type(owner.id().symbol())
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            let local = self.members_for_instance(owner_type)?;
            let mut inherited = Vec::new();
            for &interface in owner.interfaces() {
                inherited.extend(self.members_for_instance(interface)?);
            }
            let inherited = self.unshadowed_members(inherited)?;
            if owner.kind() == NominalKind::Interface {
                self.check_interface_replacements(&local, &inherited)?;
            } else {
                self.check_concrete_implementation(owner.id(), &local, &inherited)?;
            }
        }
        Ok(())
    }

    fn check_interface_replacements(
        &mut self,
        local: &[MemberSignature],
        inherited: &[MemberSignature],
    ) -> Result<(), TypeCheckingError> {
        for member in local {
            let Some(parent) = inherited.iter().find(|parent| parent.shape == member.shape) else {
                continue;
            };
            if member.contract != parent.contract {
                self.emit_with_label(
                    self.interface_member_mismatch_code,
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
        owner: NominalId,
        local: &[MemberSignature],
        inherited: &[MemberSignature],
    ) -> Result<(), TypeCheckingError> {
        let owner_span = self.symbol_spans[owner.symbol().index()];
        let delegations = self
            .delegations
            .iter()
            .filter(|plan| plan.owner() == owner)
            .copied()
            .collect::<Vec<_>>();
        let invalid_delegations = self
            .invalid_delegations
            .iter()
            .filter(|(candidate, _)| *candidate == owner)
            .map(|(_, interface)| *interface)
            .collect::<Vec<_>>();
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
                let label = targets
                    .first()
                    .map_or(owner_span, |target| target.name_span);
                self.emit_with_label(
                    self.invalid_override_code,
                    "member override is missing, has no target, or has an incompatible contract",
                    member.modifiers.override_span.unwrap_or(member.name_span),
                    label,
                    "related interface member or owner declared here",
                )?;
            }
        }
        for sources in by_shape.into_values() {
            let mut active = Vec::new();
            for source in sources {
                let mut poisoned = false;
                for &interface in &invalid_delegations {
                    poisoned |= self.satisfies_interface(interface, source.owner)?;
                }
                if !poisoned {
                    active.push(source);
                }
            }
            if active.is_empty() {
                continue;
            }
            let mut providing_delegates = Vec::new();
            for plan in &delegations {
                let mut provides = false;
                for source in &active {
                    provides |= self.satisfies_interface(plan.interface(), source.owner)?;
                }
                if provides {
                    providing_delegates.push(*plan);
                }
            }
            let mut foreign_defaults = 0;
            for source in &active {
                if !source.has_body {
                    continue;
                }
                let mut covered = false;
                for plan in &providing_delegates {
                    covered |= self.satisfies_interface(plan.interface(), source.owner)?;
                }
                foreign_defaults += usize::from(!covered);
            }
            if providing_delegates.len() > 1
                || !providing_delegates.is_empty() && foreign_defaults > 0
            {
                self.emit_with_label(
                    self.delegation_member_conflict_code,
                    "delegation and inherited implementations require an explicit override",
                    owner_span,
                    active[0].name_span,
                    "conflicting interface member declared here",
                )?;
                continue;
            }
            if providing_delegates.len() == 1 {
                continue;
            }
            let defaults = active.iter().filter(|source| source.has_body).count();
            if defaults > 1 {
                self.emit_with_label(
                    self.default_member_conflict_code,
                    "multiple interface defaults require an explicit override",
                    owner_span,
                    active[1].name_span,
                    "conflicting default declared here",
                )?;
            } else if defaults == 0 {
                self.emit_with_label(
                    self.missing_interface_member_code,
                    "concrete classifier does not implement an abstract interface member",
                    owner_span,
                    active[0].name_span,
                    "required member declared here",
                )?;
            }
        }
        Ok(())
    }

    fn unshadowed_members(
        &mut self,
        members: Vec<MemberSignature>,
    ) -> Result<Vec<MemberSignature>, TypeCheckingError> {
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
        instance: TypeId,
    ) -> Result<Vec<MemberSignature>, TypeCheckingError> {
        let TypeKind::Nominal { nominal, arguments } = self.kind(instance).clone() else {
            return Ok(Vec::new());
        };
        let descriptor = self
            .nominals
            .iter()
            .find(|descriptor| descriptor.id() == nominal)
            .cloned()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        let substitutions = descriptor
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments)
            .collect::<BTreeMap<_, _>>();
        let classifier = self
            .ast()
            .items()
            .iter()
            .find_map(|(_, node)| match node.payload() {
                Item::Classifier(classifier)
                    if matches!(classifier.name, NameMarker::Present(span) if self.symbol_at(span) == Some(nominal.symbol())) =>
                {
                    Some(classifier.as_ref().clone())
                }
                _ => None,
            });
        let Some(body) = classifier.and_then(|classifier| classifier.body) else {
            return Ok(Vec::new());
        };
        body.members
            .into_iter()
            .filter_map(|member| {
                self.member_signature(member, instance, &substitutions)
                    .transpose()
            })
            .collect()
    }

    fn member_signature(
        &mut self,
        id: ItemId,
        owner: TypeId,
        substitutions: &BTreeMap<SymbolId, TypeId>,
    ) -> Result<Option<MemberSignature>, TypeCheckingError> {
        let Some(FunctionParts {
            modifiers,
            name,
            type_parameters,
            parameters,
            form,
        }) = self.function_parts(id)?
        else {
            return Ok(None);
        };
        let NameMarker::Present(name_span) = name else {
            return Ok(None);
        };
        let slots = type_parameters
            .iter()
            .enumerate()
            .filter_map(|(slot, parameter)| match parameter.name {
                NameMarker::Present(span) => self.symbol_at(span).map(|symbol| (symbol, slot)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let mut shape_parameters = Vec::new();
        let mut modes = Vec::new();
        for parameter in parameters {
            let ty = self.resolve_type_ref(parameter.type_ref)?;
            let ty = self.substitute_type(ty, substitutions)?;
            let Some(shape) = self.shape_type(ty, &slots) else {
                return Ok(None);
            };
            shape_parameters.push(shape);
            modes.push(source_parameter_mode(parameter.mode_marker));
        }
        let return_type = self.function_return_type(form)?;
        let return_type = self.substitute_type(return_type, substitutions)?;
        let Some(return_type) = self.shape_type(return_type, &slots) else {
            return Ok(None);
        };
        Ok(Some(MemberSignature {
            shape: MemberShape {
                name: self.sources.slice(name_span)?.to_owned(),
                generic_arity: type_parameters.len(),
                parameters: shape_parameters,
            },
            contract: MemberContract { modes, return_type },
            name_span,
            modifiers,
            has_body: function_has_body(form),
            owner,
        }))
    }

    fn function_parts(&self, id: ItemId) -> Result<Option<FunctionParts>, TypeCheckingError> {
        match self.ast().items().get(id)?.payload() {
            Item::Modified {
                modifiers,
                declaration,
            } => {
                let Some(parts) = self.function_parts(*declaration)? else {
                    return Ok(None);
                };
                Ok(Some(FunctionParts {
                    modifiers: *modifiers,
                    ..parts
                }))
            }
            Item::Function {
                name,
                type_parameters,
                parameters,
                form,
                ..
            } => Ok(Some(FunctionParts {
                modifiers: DeclarationModifiers::default(),
                name: *name,
                type_parameters: type_parameters.clone(),
                parameters: parameters.clone(),
                form: *form,
            })),
            _ => Ok(None),
        }
    }
}

fn function_has_body(form: FunctionForm) -> bool {
    match form {
        FunctionForm::ImplicitUnitAbsent => false,
        FunctionForm::ImplicitUnitBlock(_) => true,
        FunctionForm::Explicit { body, .. } => !matches!(body, FunctionBody::Absent),
    }
}
