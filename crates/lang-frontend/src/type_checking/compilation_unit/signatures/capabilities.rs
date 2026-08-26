use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::TypeRefId,
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{DeclarationId, SourceUnitId, UnitSymbolId},
    parser::{Item, TypeRef},
    source::Span,
    type_checking::{
        BuiltinType, Capability, DeferredReason, IntrinsicTypeConstructor, NominalKind,
        TypeCheckingError, UnitTypeRefId,
    },
};

use super::super::{
    CompilationUnitTypeError, SignatureCollector, UnitTypeId, UnitTypeKind, UnitTypeParameterBound,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum CapabilityState {
    Yes,
    No,
    Unknown,
    Error,
}

#[derive(Clone)]
struct InlineTarget {
    declaration: DeclarationId,
    substitutions: BTreeMap<UnitSymbolId, UnitTypeId>,
}

#[derive(Clone, Copy)]
struct InlineEdge {
    from: DeclarationId,
    to: DeclarationId,
    span: Span,
}

enum LayoutWalk {
    Finite,
    ReachesInvalid,
    Cycle {
        edges: Vec<InlineEdge>,
        affected: Vec<DeclarationId>,
    },
}

impl SignatureCollector<'_> {
    pub(super) fn validate_capabilities_and_layout(
        &mut self,
    ) -> Result<(), CompilationUnitTypeError> {
        self.check_interface_runtime_positions()?;
        let invalid_inline = self.check_inline_layouts()?;
        self.check_capability_bounds(&invalid_inline)
    }

    fn check_interface_runtime_positions(&mut self) -> Result<(), CompilationUnitTypeError> {
        let static_refs = self.static_type_refs()?;
        let mut errors = Vec::new();
        for source_index in 0..self.inputs.len() {
            let source = self.names.source_units()[source_index].source_unit();
            for (id, node) in self.inputs[source_index].ast().type_refs().iter() {
                if static_refs.contains(&(source, id.index())) {
                    continue;
                }
                let Some(ty) = self
                    .type_ref_types
                    .get(&UnitTypeRefId::new(source, id))
                    .copied()
                else {
                    continue;
                };
                let ty = match self.types.get(ty) {
                    Some(UnitTypeKind::Nullable(inner)) => *inner,
                    _ => ty,
                };
                let Some(declaration) = self.nominal_declaration(ty) else {
                    continue;
                };
                if self.nominals[&declaration].kind() == NominalKind::Interface {
                    errors.push((node.span(), declaration));
                }
            }
        }
        for (primary, declaration) in errors {
            self.emit_with_label(
                codes::INTERFACE_RUNTIME_VALUE,
                "interface cannot be used as a runtime value type without dyn",
                primary,
                self.names.index().declarations()[declaration.index()].name_span(),
                "interface declared here",
            )?;
        }
        Ok(())
    }

    fn static_type_refs(
        &self,
    ) -> Result<BTreeSet<(SourceUnitId, usize)>, CompilationUnitTypeError> {
        let mut result = BTreeSet::new();
        for source_index in 0..self.inputs.len() {
            let source = self.names.source_units()[source_index].source_unit();
            for (_, node) in self.inputs[source_index].ast().items().iter() {
                match node.payload() {
                    Item::Function {
                        type_parameters, ..
                    } => {
                        for parameter in type_parameters {
                            if let Some(bound) = parameter.bound {
                                self.mark_type_ref_tree(source, bound, &mut result)?;
                            }
                        }
                    }
                    Item::Classifier(classifier) => {
                        for parameter in &classifier.type_parameters {
                            if let Some(bound) = parameter.bound {
                                self.mark_type_ref_tree(source, bound, &mut result)?;
                            }
                        }
                        for supertype in &classifier.supertypes {
                            self.mark_type_ref_tree(source, supertype.type_ref, &mut result)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(result)
    }

    fn mark_type_ref_tree(
        &self,
        source: SourceUnitId,
        id: TypeRefId,
        output: &mut BTreeSet<(SourceUnitId, usize)>,
    ) -> Result<(), CompilationUnitTypeError> {
        if !output.insert((source, id.index())) {
            return Ok(());
        }
        match self.inputs[source.index()]
            .ast()
            .type_refs()
            .get(id)
            .map_err(TypeCheckingError::from)?
            .payload()
        {
            TypeRef::Function {
                parameters,
                return_type,
                ..
            } => {
                for parameter in parameters {
                    self.mark_type_ref_tree(source, parameter.type_ref, output)?;
                }
                self.mark_type_ref_tree(source, *return_type, output)?;
            }
            TypeRef::Qualified { segments, .. } => {
                for argument in segments.iter().flat_map(|segment| &segment.arguments) {
                    self.mark_type_ref_tree(source, *argument, output)?;
                }
            }
            TypeRef::Error => {}
        }
        Ok(())
    }

    fn check_inline_layouts(
        &mut self,
    ) -> Result<BTreeSet<DeclarationId>, CompilationUnitTypeError> {
        let roots = self
            .nominals
            .iter()
            .filter(|(_, nominal)| {
                matches!(
                    nominal.kind(),
                    NominalKind::ValueClass | NominalKind::EnumClass
                )
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        let mut invalid = BTreeSet::new();
        for root in roots {
            if invalid.contains(&root) {
                continue;
            }
            let substitutions = self.identity_substitutions(root)?;
            match self.walk_inline_layout(
                root,
                &substitutions,
                &mut Vec::new(),
                &mut Vec::new(),
                &invalid,
            )? {
                LayoutWalk::Finite => {}
                LayoutWalk::ReachesInvalid => {
                    invalid.insert(root);
                }
                LayoutWalk::Cycle { edges, affected } => {
                    invalid.extend(affected);
                    self.emit_inline_cycle(&edges)?;
                }
            }
        }
        Ok(invalid)
    }

    fn identity_substitutions(
        &self,
        nominal: DeclarationId,
    ) -> Result<BTreeMap<UnitSymbolId, UnitTypeId>, CompilationUnitTypeError> {
        self.nominals[&nominal]
            .type_parameters()
            .iter()
            .map(|parameter| {
                self.symbol_types
                    .get(parameter)
                    .copied()
                    .map(|ty| (*parameter, ty))
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
            })
            .collect()
    }

    fn walk_inline_layout(
        &self,
        nominal: DeclarationId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        path: &mut Vec<DeclarationId>,
        edges: &mut Vec<InlineEdge>,
        invalid: &BTreeSet<DeclarationId>,
    ) -> Result<LayoutWalk, CompilationUnitTypeError> {
        path.push(nominal);
        for (component, span) in self.inline_components(nominal)? {
            let Some(target) = self.inline_target(component, substitutions) else {
                continue;
            };
            let edge = InlineEdge {
                from: nominal,
                to: target.declaration,
                span,
            };
            if let Some(position) = path
                .iter()
                .position(|candidate| *candidate == target.declaration)
            {
                let mut cycle_edges = edges[position..].to_vec();
                cycle_edges.push(edge);
                let affected = path.clone();
                path.pop();
                return Ok(LayoutWalk::Cycle {
                    edges: cycle_edges,
                    affected,
                });
            }
            if invalid.contains(&target.declaration) {
                path.pop();
                return Ok(LayoutWalk::ReachesInvalid);
            }
            edges.push(edge);
            let result = self.walk_inline_layout(
                target.declaration,
                &target.substitutions,
                path,
                edges,
                invalid,
            )?;
            edges.pop();
            if !matches!(result, LayoutWalk::Finite) {
                path.pop();
                return Ok(result);
            }
        }
        path.pop();
        Ok(LayoutWalk::Finite)
    }

    fn inline_components(
        &self,
        nominal: DeclarationId,
    ) -> Result<Vec<(UnitTypeId, Span)>, CompilationUnitTypeError> {
        let declaration = &self.names.index().declarations()[nominal.index()];
        let source = declaration.source_unit();
        let item = self.inputs[source.index()]
            .ast()
            .items()
            .get(declaration.root())
            .map_err(TypeCheckingError::from)?;
        let Item::Classifier(classifier) = item.payload() else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        let mut components = Vec::new();
        if self.nominals[&nominal].kind() == NominalKind::ValueClass {
            if let Some(constructor) = &classifier.primary_constructor {
                for field in &constructor.fields {
                    let ty = self
                        .type_ref_types
                        .get(&UnitTypeRefId::new(source, field.type_ref))
                        .copied()
                        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                    let span = self.inputs[source.index()]
                        .ast()
                        .type_refs()
                        .get(field.type_ref)
                        .map_err(TypeCheckingError::from)?
                        .span();
                    components.push((ty, span));
                }
            }
        } else if let Some(body) = &classifier.body {
            for variant in &body.variants {
                for parameter in &variant.parameters {
                    let ty = self
                        .type_ref_types
                        .get(&UnitTypeRefId::new(source, parameter.type_ref))
                        .copied()
                        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                    let span = self.inputs[source.index()]
                        .ast()
                        .type_refs()
                        .get(parameter.type_ref)
                        .map_err(TypeCheckingError::from)?
                        .span();
                    components.push((ty, span));
                }
            }
        }
        Ok(components)
    }

    fn inline_target(
        &self,
        mut ty: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    ) -> Option<InlineTarget> {
        let mut seen_parameters = BTreeSet::new();
        loop {
            match self.types.get(ty)? {
                UnitTypeKind::TypeParameter(symbol) => {
                    let actual = substitutions.get(symbol).copied()?;
                    if actual == ty || !seen_parameters.insert(*symbol) {
                        return None;
                    }
                    ty = actual;
                }
                UnitTypeKind::Nullable(inner) => ty = *inner,
                UnitTypeKind::Nominal {
                    declaration,
                    arguments,
                } => {
                    let descriptor = self.nominals.get(declaration)?;
                    if !matches!(
                        descriptor.kind(),
                        NominalKind::ValueClass | NominalKind::EnumClass
                    ) {
                        return None;
                    }
                    let mut child = substitutions.clone();
                    child.extend(
                        descriptor
                            .type_parameters()
                            .iter()
                            .copied()
                            .zip(arguments.iter().copied()),
                    );
                    return Some(InlineTarget {
                        declaration: *declaration,
                        substitutions: child,
                    });
                }
                _ => return None,
            }
        }
    }

    fn emit_inline_cycle(&mut self, edges: &[InlineEdge]) -> Result<(), CompilationUnitTypeError> {
        let Some((closing, preceding)) = edges.split_last() else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        let code = codes::catalog()?.resolve(codes::INFINITE_INLINE_LAYOUT)?;
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            "inline value layout forms an infinite recursive cycle",
            closing.span,
        )?;
        for edge in preceding {
            diagnostic.add_label(
                self.sources,
                edge.span,
                format!(
                    "inline edge declaration#{} -> declaration#{}",
                    edge.from.index(),
                    edge.to.index()
                ),
            )?;
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn check_capability_bounds(
        &mut self,
        invalid_inline: &BTreeSet<DeclarationId>,
    ) -> Result<(), CompilationUnitTypeError> {
        let mut failures = Vec::new();
        for source_index in 0..self.inputs.len() {
            let source = self.names.source_units()[source_index].source_unit();
            for (id, node) in self.inputs[source_index].ast().type_refs().iter() {
                let TypeRef::Qualified { segments, .. } = node.payload() else {
                    continue;
                };
                let Some(segment) = segments.last() else {
                    continue;
                };
                let Some(mut ty) = self
                    .type_ref_types
                    .get(&UnitTypeRefId::new(source, id))
                    .copied()
                else {
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
                for (index, (&parameter, &argument)) in
                    nominal.type_parameters().iter().zip(&arguments).enumerate()
                {
                    let bound = self.type_parameters[&parameter].bound();
                    let state = match bound {
                        UnitTypeParameterBound::Capability(Capability::Copyable) => self
                            .copyability(
                                argument,
                                &BTreeMap::new(),
                                &mut BTreeSet::new(),
                                invalid_inline,
                            ),
                        UnitTypeParameterBound::Capability(Capability::Transferable) => self
                            .transferability(
                                argument,
                                &BTreeMap::new(),
                                &mut BTreeSet::new(),
                                invalid_inline,
                            ),
                        _ => continue,
                    };
                    if state == CapabilityState::No {
                        let primary = self.inputs[source_index]
                            .ast()
                            .type_refs()
                            .get(segment.arguments[index])
                            .map_err(TypeCheckingError::from)?
                            .span();
                        failures.push((primary, parameter, bound));
                    }
                }
            }
        }
        for (primary, parameter, bound) in failures {
            let (code, message, label) = match bound {
                UnitTypeParameterBound::Capability(Capability::Copyable) => (
                    codes::COPYABLE_TYPE_ARGUMENT_BOUND,
                    "type argument does not satisfy its Copyable bound",
                    "Copyable bound declared here",
                ),
                UnitTypeParameterBound::Capability(Capability::Transferable) => (
                    codes::TRANSFERABLE_TYPE_ARGUMENT_BOUND,
                    "type argument does not satisfy its Transferable bound",
                    "Transferable bound declared here",
                ),
                _ => continue,
            };
            self.emit_with_label(
                code,
                message,
                primary,
                self.unit_symbol_span(parameter)?,
                label,
            )?;
        }
        Ok(())
    }

    fn copyability(
        &self,
        ty: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<DeclarationId>,
        invalid_inline: &BTreeSet<DeclarationId>,
    ) -> CapabilityState {
        match self.types.get(ty) {
            Some(UnitTypeKind::Builtin(
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
                | BuiltinType::Unit
                | BuiltinType::Nothing,
            )) => CapabilityState::Yes,
            Some(UnitTypeKind::Builtin(BuiltinType::String | BuiltinType::Any))
            | Some(UnitTypeKind::Function { .. })
            | Some(UnitTypeKind::Intrinsic { .. }) => CapabilityState::No,
            Some(UnitTypeKind::Nullable(inner)) => {
                self.copyability(*inner, substitutions, active, invalid_inline)
            }
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => self.nominal_capability(
                *declaration,
                arguments,
                substitutions,
                active,
                invalid_inline,
                false,
            ),
            Some(UnitTypeKind::EnumCase { root, .. }) => {
                self.copyability(*root, substitutions, active, invalid_inline)
            }
            Some(UnitTypeKind::TypeParameter(symbol)) => {
                if let Some(actual) = substitutions.get(symbol).copied()
                    && actual != ty
                {
                    return self.copyability(actual, substitutions, active, invalid_inline);
                }
                match self
                    .type_parameters
                    .get(symbol)
                    .map(|parameter| parameter.bound())
                {
                    Some(UnitTypeParameterBound::Capability(Capability::Copyable)) => {
                        CapabilityState::Yes
                    }
                    Some(UnitTypeParameterBound::Error) => CapabilityState::Error,
                    _ => CapabilityState::No,
                }
            }
            Some(UnitTypeKind::Deferred(DeferredReason::AnyValueRepresentation)) => {
                CapabilityState::No
            }
            Some(UnitTypeKind::Deferred(_)) => CapabilityState::Unknown,
            _ => CapabilityState::Error,
        }
    }

    fn transferability(
        &self,
        ty: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<DeclarationId>,
        invalid_inline: &BTreeSet<DeclarationId>,
    ) -> CapabilityState {
        match self.types.get(ty) {
            Some(UnitTypeKind::Builtin(
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
            )) => CapabilityState::Yes,
            Some(UnitTypeKind::Builtin(BuiltinType::Any)) | Some(UnitTypeKind::Function { .. }) => {
                CapabilityState::No
            }
            Some(UnitTypeKind::Nullable(inner)) => {
                self.transferability(*inner, substitutions, active, invalid_inline)
            }
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Rc,
                ..
            }) => CapabilityState::No,
            Some(UnitTypeKind::Intrinsic { arguments, .. }) => {
                arguments
                    .iter()
                    .fold(CapabilityState::Yes, |state, argument| {
                        combine(
                            state,
                            self.transferability(*argument, substitutions, active, invalid_inline),
                        )
                    })
            }
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => self.nominal_capability(
                *declaration,
                arguments,
                substitutions,
                active,
                invalid_inline,
                true,
            ),
            Some(UnitTypeKind::EnumCase { root, .. }) => {
                self.transferability(*root, substitutions, active, invalid_inline)
            }
            Some(UnitTypeKind::TypeParameter(symbol)) => {
                if let Some(actual) = substitutions.get(symbol).copied()
                    && actual != ty
                {
                    return self.transferability(actual, substitutions, active, invalid_inline);
                }
                match self
                    .type_parameters
                    .get(symbol)
                    .map(|parameter| parameter.bound())
                {
                    Some(UnitTypeParameterBound::Capability(Capability::Transferable)) => {
                        CapabilityState::Yes
                    }
                    Some(UnitTypeParameterBound::Error) => CapabilityState::Error,
                    _ => CapabilityState::No,
                }
            }
            Some(UnitTypeKind::Deferred(DeferredReason::AnyValueRepresentation)) => {
                CapabilityState::No
            }
            Some(UnitTypeKind::Deferred(_)) => CapabilityState::Unknown,
            _ => CapabilityState::Error,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn nominal_capability(
        &self,
        nominal: DeclarationId,
        arguments: &[UnitTypeId],
        outer: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<DeclarationId>,
        invalid_inline: &BTreeSet<DeclarationId>,
        transferable: bool,
    ) -> CapabilityState {
        if invalid_inline.contains(&nominal) {
            return CapabilityState::Error;
        }
        if !active.insert(nominal) {
            return if transferable {
                CapabilityState::Yes
            } else {
                CapabilityState::Error
            };
        }
        let descriptor = &self.nominals[&nominal];
        let result = match descriptor.kind() {
            NominalKind::Class if !transferable => CapabilityState::No,
            NominalKind::Object => CapabilityState::No,
            NominalKind::Interface => CapabilityState::Error,
            NominalKind::Class | NominalKind::ValueClass | NominalKind::EnumClass => {
                let mut substitutions = outer.clone();
                substitutions.extend(
                    descriptor
                        .type_parameters()
                        .iter()
                        .copied()
                        .zip(arguments.iter().copied()),
                );
                let components = if descriptor.kind() == NominalKind::EnumClass {
                    descriptor
                        .enum_cases()
                        .iter()
                        .flat_map(|case| case.payloads().iter().map(|payload| payload.ty()))
                        .collect::<Vec<_>>()
                } else {
                    descriptor
                        .fields()
                        .iter()
                        .map(|field| field.ty())
                        .collect::<Vec<_>>()
                };
                components
                    .into_iter()
                    .fold(CapabilityState::Yes, |state, component| {
                        combine(
                            state,
                            if transferable {
                                self.transferability(
                                    component,
                                    &substitutions,
                                    active,
                                    invalid_inline,
                                )
                            } else {
                                self.copyability(component, &substitutions, active, invalid_inline)
                            },
                        )
                    })
            }
        };
        active.remove(&nominal);
        result
    }
}

fn combine(left: CapabilityState, right: CapabilityState) -> CapabilityState {
    match (left, right) {
        (CapabilityState::Error, _) | (_, CapabilityState::Error) => CapabilityState::Error,
        (CapabilityState::Unknown, _) | (_, CapabilityState::Unknown) => CapabilityState::Unknown,
        (CapabilityState::No, _) | (_, CapabilityState::No) => CapabilityState::No,
        (CapabilityState::Yes, CapabilityState::Yes) => CapabilityState::Yes,
    }
}
