use std::collections::{BTreeMap, BTreeSet};

use crate::{
    diagnostic::codes,
    name_resolution::{DeclarationId, SourceUnitId, UnitSymbolId},
    parser::{ClassifierDeclaration, Item, NameMarker, TypeParameter, TypeRef},
    type_checking::{BuiltinType, Capability, NominalKind, TypeCheckingError},
};

use super::{
    CompilationUnitTypeError, SignatureCollector, UnitFunctionParameterType, UnitTypeId,
    UnitTypeKind, UnitTypeParameterBound,
};

impl SignatureCollector<'_> {
    pub(super) fn collect_direct_interfaces(
        &mut self,
        source: SourceUnitId,
        classifier: &ClassifierDeclaration,
        owner: DeclarationId,
    ) -> Result<Vec<UnitTypeId>, CompilationUnitTypeError> {
        let mut accepted = Vec::new();
        let mut first_by_declaration = BTreeMap::new();
        for entry in &classifier.supertypes {
            let ty = self.resolve_type_ref(source, entry.type_ref)?;
            if self.is_error(ty) {
                continue;
            }
            let span = self.inputs[source.index()]
                .ast()
                .type_refs()
                .get(entry.type_ref)
                .map_err(TypeCheckingError::from)?
                .span();
            let Some(target) = self.nominal_declaration(ty) else {
                self.emit(
                    codes::INVALID_SUPERTYPE,
                    "class-family supertype must be an interface",
                    span,
                )?;
                continue;
            };
            let Some(target_signature) = self.nominals.get(&target) else {
                return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
            };
            if target_signature.kind() != NominalKind::Interface {
                let label = self.names.index().declarations()[target.index()].name_span();
                self.emit_with_label(
                    codes::INVALID_SUPERTYPE,
                    "class-family supertype must be an interface",
                    span,
                    label,
                    "non-interface type declared here",
                )?;
                continue;
            }
            if let Some(first) = first_by_declaration.insert(target, span) {
                self.emit_with_label(
                    codes::INVALID_SUPERTYPE,
                    "interface appears more than once in the direct supertype list",
                    span,
                    first,
                    "first interface instance appears here",
                )?;
                continue;
            }
            accepted.push(ty);
            self.interface_edge_spans.insert((owner, target), span);
        }
        Ok(accepted)
    }

    pub(super) fn collect_type_parameter_bounds(&mut self) -> Result<(), CompilationUnitTypeError> {
        for source_index in 0..self.inputs.len() {
            let source = self.names.source_units()[source_index].source_unit();
            let parameters = self.inputs[source_index]
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
                self.collect_type_parameter_bound(source, &parameter)?;
            }
        }
        Ok(())
    }

    fn collect_type_parameter_bound(
        &mut self,
        source: SourceUnitId,
        parameter: &TypeParameter,
    ) -> Result<(), CompilationUnitTypeError> {
        let Some(bound_ref) = parameter.bound else {
            return Ok(());
        };
        let Some(symbol) = self.marker_symbol(source, parameter.name) else {
            return Ok(());
        };
        let ty = self.resolve_type_ref(source, bound_ref)?;
        let bound = match self.types.get(ty) {
            Some(UnitTypeKind::Builtin(BuiltinType::Any)) => UnitTypeParameterBound::Any,
            Some(UnitTypeKind::Capability(capability)) => {
                UnitTypeParameterBound::Capability(*capability)
            }
            Some(UnitTypeKind::Nominal { declaration, .. })
                if self
                    .nominals
                    .get(declaration)
                    .is_some_and(|nominal| nominal.kind() == NominalKind::Interface) =>
            {
                UnitTypeParameterBound::Interface(ty)
            }
            Some(UnitTypeKind::Error) | None => UnitTypeParameterBound::Error,
            _ => {
                let primary = self.inputs[source.index()]
                    .ast()
                    .type_refs()
                    .get(bound_ref)
                    .map_err(TypeCheckingError::from)?
                    .span();
                self.emit_with_label(
                    codes::INVALID_TYPE_BOUND,
                    "type parameter bound must be Any, an interface, or a compiler capability",
                    primary,
                    marker_span(parameter.name),
                    "type parameter declared here",
                )?;
                UnitTypeParameterBound::Error
            }
        };
        self.type_parameters
            .get_mut(&symbol)
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
            .set_bound(bound);
        Ok(())
    }

    pub(super) fn check_interface_cycles(&mut self) -> Result<(), CompilationUnitTypeError> {
        loop {
            let Some((from, to)) = self.find_interface_cycle() else {
                return Ok(());
            };
            let primary = self.interface_edge_spans[&(from, to)];
            let label = self.names.index().declarations()[to.index()].name_span();
            self.emit_with_label(
                codes::INTERFACE_CYCLE,
                "interface inheritance forms a cycle",
                primary,
                label,
                "cycle reaches this interface again",
            )?;
            let retained = self.nominals[&from]
                .direct_interfaces()
                .iter()
                .copied()
                .filter(|ty| self.nominal_declaration(*ty) != Some(to))
                .collect();
            self.nominals
                .get_mut(&from)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
                .set_direct_interfaces(retained);
            self.interface_edge_spans.remove(&(from, to));
        }
    }

    fn find_interface_cycle(&self) -> Option<(DeclarationId, DeclarationId)> {
        let mut colors = self
            .nominals
            .keys()
            .copied()
            .map(|id| (id, 0_u8))
            .collect::<BTreeMap<_, _>>();
        let roots = self
            .nominals
            .iter()
            .filter(|(_, nominal)| nominal.kind() == NominalKind::Interface)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for root in roots {
            if colors[&root] != 0 {
                continue;
            }
            colors.insert(root, 1);
            let mut stack = vec![(root, 0_usize)];
            while let Some((current, next)) = stack.last_mut() {
                let targets = self.direct_interface_targets(*current);
                if *next == targets.len() {
                    colors.insert(*current, 2);
                    stack.pop();
                    continue;
                }
                let from = *current;
                let target = targets[*next];
                *next += 1;
                match colors.get(&target).copied().unwrap_or_default() {
                    1 => return Some((from, target)),
                    0 => {
                        colors.insert(target, 1);
                        stack.push((target, 0));
                    }
                    _ => {}
                }
            }
        }
        None
    }

    pub(super) fn compute_interface_closures(&mut self) -> Result<(), CompilationUnitTypeError> {
        let mut visited = BTreeSet::new();
        let mut order = Vec::new();
        for root in self.nominals.keys().copied().collect::<Vec<_>>() {
            if visited.contains(&root) {
                continue;
            }
            let mut stack = vec![(root, false)];
            while let Some((current, expanded)) = stack.pop() {
                if expanded {
                    order.push(current);
                    continue;
                }
                if !visited.insert(current) {
                    continue;
                }
                stack.push((current, true));
                for target in self.direct_interface_targets(current).into_iter().rev() {
                    if !visited.contains(&target) {
                        stack.push((target, false));
                    }
                }
            }
        }
        for owner in order {
            let direct = self.nominals[&owner].direct_interfaces().to_vec();
            let mut closure = direct.clone();
            for interface in direct {
                let Some(target) = self.nominal_declaration(interface) else {
                    continue;
                };
                let arguments = match self.types.get(interface) {
                    Some(UnitTypeKind::Nominal { arguments, .. }) => arguments.clone(),
                    _ => continue,
                };
                let target_signature = self.nominals[&target].clone();
                let substitutions = target_signature
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments)
                    .collect::<BTreeMap<_, _>>();
                for inherited in target_signature.interfaces() {
                    let inherited = self.substitute_type(*inherited, &substitutions)?;
                    if !closure.contains(&inherited) {
                        closure.push(inherited);
                    }
                }
            }
            self.nominals
                .get_mut(&owner)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
                .set_interfaces(closure);
        }
        Ok(())
    }

    pub(super) fn validate_type_argument_bounds(&mut self) -> Result<(), CompilationUnitTypeError> {
        for source_index in 0..self.inputs.len() {
            let source = self.names.source_units()[source_index].source_unit();
            let uses = self.inputs[source_index]
                .ast()
                .type_refs()
                .iter()
                .map(|(id, node)| (id, node.payload().clone()))
                .collect::<Vec<_>>();
            for (id, payload) in uses {
                let TypeRef::Qualified { segments, .. } = payload else {
                    continue;
                };
                let Some(segment) = segments.last() else {
                    continue;
                };
                let Some(mut ty) = self.type_ref_types.get(&(source, id.index())).copied() else {
                    continue;
                };
                if let Some(UnitTypeKind::Nullable(inner)) = self.types.get(ty) {
                    ty = *inner;
                }
                let Some(declaration) = self.nominal_declaration(ty) else {
                    continue;
                };
                let arguments = match self.types.get(ty) {
                    Some(UnitTypeKind::Nominal { arguments, .. }) => arguments.clone(),
                    _ => continue,
                };
                let nominal = self.nominals[&declaration].clone();
                let substitutions = nominal
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments.iter().copied())
                    .collect::<BTreeMap<_, _>>();
                for (index, (parameter, argument)) in nominal
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments)
                    .enumerate()
                {
                    let descriptor = self.type_parameters[&parameter];
                    match descriptor.bound() {
                        UnitTypeParameterBound::Interface(bound) => {
                            let bound = self.substitute_type(bound, &substitutions)?;
                            if !self.is_error(argument)
                                && !self.satisfies_interface(argument, bound)?
                            {
                                self.emit_with_label(
                                    codes::TYPE_ARGUMENT_BOUND,
                                    "type argument does not satisfy its interface bound",
                                    self.inputs[source_index]
                                        .ast()
                                        .type_refs()
                                        .get(segment.arguments[index])
                                        .map_err(TypeCheckingError::from)?
                                        .span(),
                                    self.unit_symbol_span(parameter)?,
                                    "interface bound declared here",
                                )?;
                            }
                        }
                        UnitTypeParameterBound::Any
                        | UnitTypeParameterBound::Capability(Capability::Copyable)
                        | UnitTypeParameterBound::Capability(Capability::Transferable)
                        | UnitTypeParameterBound::Error => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn direct_interface_targets(&self, owner: DeclarationId) -> Vec<DeclarationId> {
        self.nominals
            .get(&owner)
            .into_iter()
            .flat_map(|nominal| nominal.direct_interfaces())
            .filter_map(|ty| self.nominal_declaration(*ty))
            .collect()
    }

    fn substitute_type(
        &mut self,
        ty: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let kind = self.types.get(ty).cloned().unwrap_or(UnitTypeKind::Error);
        let substituted = match kind {
            UnitTypeKind::TypeParameter(symbol) => {
                return Ok(substitutions.get(&symbol).copied().unwrap_or(ty));
            }
            UnitTypeKind::Nullable(inner) => {
                UnitTypeKind::Nullable(self.substitute_type(inner, substitutions)?)
            }
            UnitTypeKind::Function {
                move_only,
                parameters,
                return_type,
            } => UnitTypeKind::Function {
                move_only,
                parameters: parameters
                    .into_iter()
                    .map(|parameter| {
                        Ok(UnitFunctionParameterType::new(
                            parameter.mode(),
                            self.substitute_type(parameter.ty(), substitutions)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompilationUnitTypeError>>()?,
                return_type: self.substitute_type(return_type, substitutions)?,
            },
            UnitTypeKind::Nominal {
                declaration,
                arguments,
            } => UnitTypeKind::Nominal {
                declaration,
                arguments: arguments
                    .into_iter()
                    .map(|argument| self.substitute_type(argument, substitutions))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            UnitTypeKind::Intrinsic {
                constructor,
                arguments,
            } => UnitTypeKind::Intrinsic {
                constructor,
                arguments: arguments
                    .into_iter()
                    .map(|argument| self.substitute_type(argument, substitutions))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            UnitTypeKind::StaticSelf(interface) => {
                UnitTypeKind::StaticSelf(self.substitute_type(interface, substitutions)?)
            }
            other => return Ok(self.types.intern(other)),
        };
        Ok(self.types.intern(substituted))
    }

    fn satisfies_interface(
        &mut self,
        actual: UnitTypeId,
        expected: UnitTypeId,
    ) -> Result<bool, CompilationUnitTypeError> {
        if actual == expected {
            return Ok(true);
        }
        match self.types.get(actual).cloned() {
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => {
                let nominal = self.nominals[&declaration].clone();
                let substitutions = nominal
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments)
                    .collect::<BTreeMap<_, _>>();
                for interface in nominal.interfaces() {
                    if self.substitute_type(*interface, &substitutions)? == expected {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Some(UnitTypeKind::TypeParameter(symbol)) => {
                let Some(UnitTypeParameterBound::Interface(bound)) = self
                    .type_parameters
                    .get(&symbol)
                    .map(|descriptor| descriptor.bound())
                else {
                    return Ok(false);
                };
                self.satisfies_interface(bound, expected)
            }
            _ => Ok(false),
        }
    }

    fn unit_symbol_span(
        &self,
        symbol: UnitSymbolId,
    ) -> Result<crate::source::Span, CompilationUnitTypeError> {
        self.names.source_units()[symbol.source_unit().index()]
            .resolution()
            .symbols()
            .get(symbol.symbol().index())
            .map(|symbol| symbol.span())
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
    }
}

const fn marker_span(marker: NameMarker) -> crate::source::Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
    }
}
