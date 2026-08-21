use crate::parser::{Item, NameMarker, TypeRef};

use super::*;

impl Checker<'_> {
    pub(super) fn check_direct_interfaces(&mut self) -> Result<(), TypeCheckingError> {
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
            let NameMarker::Present(name_span) = classifier.name else {
                continue;
            };
            let Some(symbol) = self.symbol_at(name_span) else {
                continue;
            };
            let Some(nominal) = self.nominal_by_symbol.get(&symbol).copied() else {
                continue;
            };
            let mut accepted = Vec::new();
            let mut first_by_nominal = BTreeMap::new();
            for supertype in classifier.supertypes {
                let ty = self.resolve_static_type_ref(supertype.type_ref)?;
                if self.is_error(ty) {
                    continue;
                }
                let TypeKind::Nominal {
                    nominal: target, ..
                } = self.kind(ty)
                else {
                    self.emit(
                        self.invalid_supertype_code,
                        "class-family supertype must be an interface",
                        self.ast().type_refs().get(supertype.type_ref)?.span(),
                    )?;
                    continue;
                };
                let target = *target;
                let is_interface = self.nominals.iter().any(|descriptor| {
                    descriptor.id() == target && descriptor.kind() == NominalKind::Interface
                });
                if !is_interface {
                    self.emit_with_label(
                        self.invalid_supertype_code,
                        "class-family supertype must be an interface",
                        self.ast().type_refs().get(supertype.type_ref)?.span(),
                        self.symbol_spans[target.symbol().index()],
                        "non-interface type declared here",
                    )?;
                    continue;
                }
                let span = self.ast().type_refs().get(supertype.type_ref)?.span();
                if let Some(first) = first_by_nominal.insert(target, span) {
                    self.emit_with_label(
                        self.invalid_supertype_code,
                        "interface appears more than once in the direct supertype list",
                        span,
                        first,
                        "first interface instance appears here",
                    )?;
                    continue;
                }
                accepted.push(ty);
                self.interface_edge_spans.insert((nominal, target), span);
            }
            let descriptor = self
                .nominals
                .iter_mut()
                .find(|descriptor| descriptor.id() == nominal)
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            descriptor.direct_interfaces = accepted.clone();
            descriptor.interfaces = accepted;
        }
        Ok(())
    }

    pub(super) fn check_interface_cycles(&mut self) -> Result<(), TypeCheckingError> {
        loop {
            let mut colors = self
                .nominals
                .iter()
                .map(|descriptor| (descriptor.id(), 0_u8))
                .collect::<BTreeMap<_, _>>();
            let roots = self
                .nominals
                .iter()
                .filter(|descriptor| descriptor.kind() == NominalKind::Interface)
                .map(NominalDescriptor::id)
                .collect::<Vec<_>>();
            let cycle = roots.into_iter().find_map(|root| {
                (colors[&root] == 0)
                    .then(|| find_interface_cycle(root, &self.nominals, &self.types, &mut colors))
                    .flatten()
            });
            let Some((from, to)) = cycle else {
                return Ok(());
            };
            let primary = self.interface_edge_spans[&(from, to)];
            self.emit_with_label(
                self.interface_cycle_code,
                "interface inheritance forms a cycle",
                primary,
                self.symbol_spans[to.symbol().index()],
                "cycle reaches this interface again",
            )?;
            let descriptor = self
                .nominals
                .iter_mut()
                .find(|descriptor| descriptor.id() == from)
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            descriptor.direct_interfaces.retain(|&ty| {
                !matches!(self.types.get(ty), Some(TypeKind::Nominal { nominal, .. }) if *nominal == to)
            });
            descriptor.interfaces = descriptor.direct_interfaces.clone();
        }
    }

    pub(super) fn check_type_parameter_bounds(&mut self) -> Result<(), TypeCheckingError> {
        let parameters = self
            .ast()
            .items()
            .iter()
            .flat_map(|(_, node)| match node.payload() {
                Item::Function {
                    type_parameters, ..
                } => type_parameters.clone(),
                Item::Classifier(classifier) => classifier.type_parameters.clone(),
                _ => Vec::new(),
            })
            .collect::<Vec<_>>();
        for parameter in parameters {
            let Some(bound) = parameter.bound else {
                continue;
            };
            let ty = self.resolve_static_type_ref(bound)?;
            let valid = matches!(
                self.kind(ty),
                TypeKind::Builtin(BuiltinType::Any) | TypeKind::Capability(_)
            ) || matches!(self.kind(ty), TypeKind::Nominal { nominal, .. }
                    if self.nominals.iter().any(|descriptor| descriptor.id() == *nominal && descriptor.kind() == NominalKind::Interface));
            let normalized = match self.kind(ty) {
                TypeKind::Builtin(BuiltinType::Any) => TypeParameterBound::Any,
                TypeKind::Capability(capability) => TypeParameterBound::Capability(*capability),
                TypeKind::Nominal { .. } if valid => TypeParameterBound::Interface(ty),
                _ => TypeParameterBound::Error,
            };
            if let NameMarker::Present(span) = parameter.name
                && let Some(symbol) = self.symbol_at(span)
                && let Some(&index) = self.type_parameter_by_symbol.get(&symbol)
            {
                self.type_parameters[index].bound = normalized;
            }
            if !self.is_error(ty) && !valid {
                let primary = self.ast().type_refs().get(bound)?.span();
                let label = match parameter.name {
                    NameMarker::Present(span)
                    | NameMarker::Missing(span)
                    | NameMarker::Error(span) => span,
                };
                self.emit_with_label(
                    self.invalid_type_bound_code,
                    "type parameter bound must be Any, an interface, or a compiler capability",
                    primary,
                    label,
                    "type parameter declared here",
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn collect_nominals(&mut self) -> Result<(), TypeCheckingError> {
        for index in 0..self.symbol_kinds.len() {
            if self.symbol_kinds[index] == SymbolKind::TypeParameter {
                let symbol = SymbolId(index);
                let ty = self.types.intern(TypeKind::TypeParameter(symbol));
                self.set_symbol(symbol, ty);
                self.type_parameter_by_symbol
                    .insert(symbol, self.type_parameters.len());
                self.type_parameters.push(TypeParameterDescriptor {
                    symbol,
                    bound: TypeParameterBound::Any,
                });
            }
        }
        let classifiers = self
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| {
                if let Item::Classifier(classifier) = node.payload() {
                    Some((node.span(), classifier.as_ref().clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for (classifier_span, classifier) in classifiers {
            let NameMarker::Present(name_span) = classifier.name else {
                continue;
            };
            let Some(symbol) = self.symbol_at(name_span) else {
                continue;
            };
            let id = NominalId::new(symbol);
            let fields = classifier
                .primary_constructor
                .as_ref()
                .into_iter()
                .flat_map(|constructor| &constructor.fields)
                .filter_map(|field| match field.name {
                    NameMarker::Present(span) => self.symbol_at(span),
                    _ => None,
                })
                .collect();
            let variants = classifier
                .body
                .as_ref()
                .into_iter()
                .flat_map(|body| &body.variants)
                .filter_map(|variant| match variant.name {
                    NameMarker::Present(span) => self.symbol_at(span),
                    _ => None,
                })
                .collect();
            let mut parameters = Vec::new();
            for parameter in classifier.type_parameters {
                if let NameMarker::Present(span) = parameter.name
                    && let Some(parameter_symbol) = self.symbol_at(span)
                {
                    parameters.push(parameter_symbol);
                    let ty = self.types.intern(TypeKind::TypeParameter(parameter_symbol));
                    self.set_symbol(parameter_symbol, ty);
                }
            }
            let kind = match classifier.kind {
                ClassifierKind::ValueClass { .. } => NominalKind::ValueClass,
                ClassifierKind::Class { .. } => NominalKind::Class,
                ClassifierKind::Interface { .. } => NominalKind::Interface,
                ClassifierKind::EnumClass { .. } => NominalKind::EnumClass,
                ClassifierKind::Object { .. } => NominalKind::Object,
            };
            self.nominal_by_symbol.insert(symbol, id);
            if let Some(scope) = self
                .classifier_scope_by_span
                .get(&(classifier_span.start(), classifier_span.end()))
                .copied()
            {
                self.nominal_by_scope.insert(scope, id);
            }
            self.nominals.push(NominalDescriptor {
                id,
                kind,
                type_parameters: parameters.clone(),
                direct_interfaces: Vec::new(),
                interfaces: Vec::new(),
                fields,
                variants,
                members: Vec::new(),
            });
            let arguments = parameters
                .into_iter()
                .map(|parameter| self.types.intern(TypeKind::TypeParameter(parameter)))
                .collect();
            let ty = self.types.intern(TypeKind::Nominal {
                nominal: id,
                arguments,
            });
            self.set_symbol(symbol, ty);
        }
        Ok(())
    }

    pub(super) fn collect_enum_cases(&mut self) -> Result<(), TypeCheckingError> {
        let variants = self
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Item::Classifier(classifier) => classifier.body.as_ref(),
                _ => None,
            })
            .flat_map(|body| body.variants.iter().cloned())
            .map(|variant| {
                (
                    (variant.span.start(), variant.span.end()),
                    variant.parameters,
                )
            })
            .collect::<BTreeMap<_, _>>();
        for source in self.source_enum_cases.clone() {
            let root = self
                .nominal_by_symbol
                .get(&source.root())
                .copied()
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            let root_type = self
                .symbol_type(source.root())
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            let parameters = variants
                .get(&(source.span().start(), source.span().end()))
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            if parameters.len() != source.payloads().len() {
                return Err(TypeCheckingError::InvalidExternalBinding);
            }
            let mut payloads = Vec::with_capacity(parameters.len());
            for (parameter, &symbol) in parameters.iter().zip(source.payloads()) {
                let ty = self.resolve_type_ref(parameter.type_ref)?;
                self.set_symbol(symbol, ty);
                self.component_type_spans[symbol.index()] =
                    Some(self.ast().type_refs().get(parameter.type_ref)?.span());
                self.enum_case_by_payload_symbol.insert(symbol, source.id());
                payloads.push((symbol, ty));
            }
            let case_type = self.types.intern(TypeKind::EnumCase {
                case: source.id(),
                root: root_type,
            });
            self.set_symbol(source.type_symbol(), case_type);
            let value_type = if payloads.is_empty() {
                root_type
            } else {
                self.types.intern(TypeKind::Function {
                    move_only: false,
                    parameters: payloads
                        .iter()
                        .map(|&(_, ty)| FunctionParameterType {
                            mode: ParameterMode::Value,
                            ty,
                        })
                        .collect(),
                    return_type: root_type,
                })
            };
            self.set_symbol(source.value_symbol(), value_type);
            let index = self.enum_cases.len();
            self.enum_case_by_id.insert(source.id(), index);
            self.enum_case_by_type_symbol
                .insert(source.type_symbol(), source.id());
            self.enum_case_by_value_symbol
                .insert(source.value_symbol(), source.id());
            self.enum_cases.push(EnumCaseDescriptor {
                id: source.id(),
                root,
                root_type,
                value_symbol: source.value_symbol(),
                type_symbol: source.type_symbol(),
                payloads,
            });
        }
        Ok(())
    }

    pub(super) fn collect_nominal_fields(&mut self) -> Result<(), TypeCheckingError> {
        let fields = self
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Item::Classifier(classifier) => classifier.primary_constructor.as_ref(),
                _ => None,
            })
            .flat_map(|constructor| constructor.fields.iter())
            .map(|field| (field.name, field.type_ref))
            .collect::<Vec<_>>();
        for (name, type_ref) in fields {
            let ty = self.resolve_type_ref(type_ref)?;
            let NameMarker::Present(span) = name else {
                continue;
            };
            let Some(symbol) = self.symbol_at(span) else {
                continue;
            };
            self.set_symbol(symbol, ty);
            self.component_type_spans[symbol.index()] =
                Some(self.ast().type_refs().get(type_ref)?.span());
        }
        Ok(())
    }

    pub(super) fn compute_interface_closures(&mut self) -> Result<(), TypeCheckingError> {
        let snapshot = self.nominals.clone();
        let mut memo = BTreeMap::new();
        for owner in snapshot.iter().map(NominalDescriptor::id) {
            let closure = self.interface_closure_for(owner, &snapshot, &mut memo)?;
            let descriptor = self
                .nominals
                .iter_mut()
                .find(|descriptor| descriptor.id() == owner)
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            descriptor.interfaces = closure;
        }
        Ok(())
    }

    /// Expands each validated interface DAG node once; additional work is proportional to the
    /// concrete closure entries that the typed result must materialize.
    fn interface_closure_for(
        &mut self,
        owner: NominalId,
        snapshot: &[NominalDescriptor],
        memo: &mut BTreeMap<NominalId, Vec<TypeId>>,
    ) -> Result<Vec<TypeId>, TypeCheckingError> {
        if let Some(closure) = memo.get(&owner) {
            return Ok(closure.clone());
        }
        let descriptor = snapshot
            .iter()
            .find(|descriptor| descriptor.id() == owner)
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        let direct_interfaces = descriptor.direct_interfaces().to_vec();
        let mut closure = direct_interfaces.clone();
        for direct in direct_interfaces {
            let TypeKind::Nominal {
                nominal: target,
                arguments,
            } = self.kind(direct).clone()
            else {
                continue;
            };
            let target_descriptor = snapshot
                .iter()
                .find(|descriptor| descriptor.id() == target)
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            let substitutions = target_descriptor
                .type_parameters()
                .iter()
                .copied()
                .zip(arguments)
                .collect::<BTreeMap<_, _>>();
            for inherited in self.interface_closure_for(target, snapshot, memo)? {
                let inherited = self.substitute_type(inherited, &substitutions)?;
                if !closure.contains(&inherited) {
                    closure.push(inherited);
                }
            }
        }
        memo.insert(owner, closure.clone());
        Ok(closure)
    }

    pub(super) fn validate_type_argument_bounds(&mut self) -> Result<(), TypeCheckingError> {
        let uses = self
            .ast()
            .type_refs()
            .iter()
            .map(|(id, node)| (id, node.payload().clone()))
            .collect::<Vec<_>>();
        for (id, payload) in uses {
            let TypeRef::Qualified { segments, .. } = payload else {
                continue;
            };
            let [segment] = segments.as_slice() else {
                continue;
            };
            let Some(ty) = self.type_ref_types[id.index()] else {
                continue;
            };
            let ty = match self.kind(ty) {
                TypeKind::Nullable(inner) => *inner,
                _ => ty,
            };
            let TypeKind::Nominal { nominal, arguments } = self.kind(ty).clone() else {
                continue;
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
                .zip(arguments.iter().copied())
                .collect::<BTreeMap<_, _>>();
            for (index, (&parameter, &argument)) in descriptor
                .type_parameters()
                .iter()
                .zip(&arguments)
                .enumerate()
            {
                let Some(parameter) = self
                    .type_parameters
                    .iter()
                    .find(|descriptor| descriptor.symbol() == parameter)
                    .copied()
                else {
                    return Err(TypeCheckingError::InvalidExternalBinding);
                };
                match parameter.bound() {
                    TypeParameterBound::Interface(bound) => {
                        let bound = self.substitute_type(bound, &substitutions)?;
                        if !self.is_error(argument) && !self.satisfies_interface(argument, bound)? {
                            self.emit_with_label(
                                self.type_argument_bound_code,
                                "type argument does not satisfy its interface bound",
                                self.ast().type_refs().get(segment.arguments[index])?.span(),
                                self.symbol_spans[parameter.symbol().index()],
                                "interface bound declared here",
                            )?;
                        }
                    }
                    TypeParameterBound::Capability(Capability::Copyable) => {
                        if self.copyability_of(argument) == Copyability::MoveOnly {
                            self.emit_with_label(
                                self.copyable_type_argument_bound_code,
                                "type argument does not satisfy its Copyable bound",
                                self.ast().type_refs().get(segment.arguments[index])?.span(),
                                self.symbol_spans[parameter.symbol().index()],
                                "Copyable bound declared here",
                            )?;
                        }
                    }
                    TypeParameterBound::Any
                    | TypeParameterBound::Capability(Capability::Transferable)
                    | TypeParameterBound::Error => {}
                }
            }
        }
        Ok(())
    }

    pub(super) fn substitute_type(
        &mut self,
        ty: TypeId,
        substitutions: &BTreeMap<SymbolId, TypeId>,
    ) -> Result<TypeId, TypeCheckingError> {
        let kind = self.kind(ty).clone();
        let substituted = match kind {
            TypeKind::TypeParameter(symbol) => {
                return Ok(substitutions.get(&symbol).copied().unwrap_or(ty));
            }
            TypeKind::StaticSelf(interface) => {
                TypeKind::StaticSelf(self.substitute_type(interface, substitutions)?)
            }
            TypeKind::Nullable(inner) => {
                TypeKind::Nullable(self.substitute_type(inner, substitutions)?)
            }
            TypeKind::Function {
                move_only,
                parameters,
                return_type,
            } => TypeKind::Function {
                move_only,
                parameters: parameters
                    .into_iter()
                    .map(|parameter| {
                        Ok(FunctionParameterType {
                            mode: parameter.mode,
                            ty: self.substitute_type(parameter.ty, substitutions)?,
                        })
                    })
                    .collect::<Result<Vec<_>, TypeCheckingError>>()?,
                return_type: self.substitute_type(return_type, substitutions)?,
            },
            TypeKind::Nominal { nominal, arguments } => TypeKind::Nominal {
                nominal,
                arguments: arguments
                    .into_iter()
                    .map(|argument| self.substitute_type(argument, substitutions))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            TypeKind::Intrinsic {
                constructor,
                arguments,
            } => TypeKind::Intrinsic {
                constructor,
                arguments: arguments
                    .into_iter()
                    .map(|argument| self.substitute_type(argument, substitutions))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            other => return Ok(self.types.intern(other)),
        };
        Ok(self.types.intern(substituted))
    }

    pub(super) fn satisfies_interface(
        &mut self,
        actual: TypeId,
        expected: TypeId,
    ) -> Result<bool, TypeCheckingError> {
        if actual == expected {
            return Ok(true);
        }
        match self.kind(actual).clone() {
            TypeKind::Nominal { nominal, arguments } => {
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
                for interface in descriptor.interfaces {
                    if self.substitute_type(interface, &substitutions)? == expected {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            TypeKind::TypeParameter(symbol) => {
                let bound = self
                    .type_parameters
                    .iter()
                    .find(|descriptor| descriptor.symbol() == symbol)
                    .map(|descriptor| descriptor.bound());
                let Some(TypeParameterBound::Interface(bound)) = bound else {
                    return Ok(false);
                };
                self.satisfies_interface(bound, expected)
            }
            _ => Ok(false),
        }
    }
}

fn find_interface_cycle(
    current: NominalId,
    nominals: &[NominalDescriptor],
    types: &TypeTable,
    colors: &mut BTreeMap<NominalId, u8>,
) -> Option<(NominalId, NominalId)> {
    colors.insert(current, 1);
    let descriptor = nominals
        .iter()
        .find(|descriptor| descriptor.id() == current)?;
    for &interface in descriptor.direct_interfaces() {
        let TypeKind::Nominal {
            nominal: target, ..
        } = types.get(interface)?
        else {
            continue;
        };
        match colors.get(target).copied().unwrap_or_default() {
            1 => return Some((current, *target)),
            0 => {
                if let Some(cycle) = find_interface_cycle(*target, nominals, types, colors) {
                    return Some(cycle);
                }
            }
            _ => {}
        }
    }
    colors.insert(current, 2);
    None
}
