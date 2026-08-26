use std::{collections::BTreeMap, sync::Arc};

use crate::{
    ast::{ItemId, TypeRefId},
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SourceUnitInput, SymbolId, SymbolKind,
        UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames, index_compilation_unit,
        ordered_unit_diagnostics,
    },
    parser::{
        ClassifierBody, ClassifierDeclaration, ClassifierKind, FunctionForm, Item, NameMarker,
        ParameterModeMarker, ParsedFile, SyntaxAst, TypePathSegment, TypeRef, ValueParameter,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, DeferredReason, EnvironmentType, ExternalTypeBinding, NominalKind,
        ParameterMode, TypeCheckingError, TypeEnvironment,
    },
};

use super::{
    CompilationUnitSignatures, CompilationUnitTypeError, UnitCallableParameter,
    UnitCallableSignature, UnitCallableTarget, UnitDeclarationSignature, UnitEnumCaseSignature,
    UnitFieldSignature, UnitFunctionParameterType, UnitNominalSignature, UnitTypeId, UnitTypeKind,
    UnitTypeParameterBound, UnitTypeParameterDescriptor, UnitTypeTable,
    shapes::{duplicate_member_shapes, duplicate_top_level_shapes},
};

mod graph;

/// 收集 canonical compilation unit 的全部顶层、nominal 与 callable signatures。
///
/// 本入口只消费无名称错误的 validated product。它不会逐文件构造互不兼容的 `TypeTable`，
/// 也不会把已知 source declaration 伪装成 compiler external symbol。
pub fn collect_compilation_unit_signatures(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
) -> Result<CompilationUnitSignatures, CompilationUnitTypeError> {
    let unit_names = names.names();
    let rebuilt = index_compilation_unit(sources, inputs)
        .map_err(|_| CompilationUnitTypeError::MismatchedInputs)?;
    if &rebuilt != unit_names.index() {
        return Err(CompilationUnitTypeError::MismatchedInputs);
    }
    let parsed = unit_names
        .index()
        .source_units()
        .iter()
        .map(|source| {
            inputs
                .iter()
                .find(|input| input.source_id() == source.source_id())
                .map(|input| input.parsed())
                .ok_or(CompilationUnitTypeError::MismatchedInputs)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if unit_names.source_units().len() != parsed.len()
        || unit_names.source_units().iter().any(|source| {
            source.resolution().source_id()
                != unit_names.index().source_units()[source.source_unit().index()].source_id()
                || !Arc::ptr_eq(source.resolution().environment_owner(), environment.owner())
        })
    {
        return Err(CompilationUnitTypeError::MismatchedNameEnvironment);
    }
    SignatureCollector::new(sources, parsed, unit_names, environment)?.run()
}

struct SignatureCollector<'a> {
    sources: &'a SourceMap,
    inputs: Vec<&'a ParsedFile>,
    names: &'a crate::name_resolution::CompilationUnitNames,
    environment: &'a TypeEnvironment,
    types: UnitTypeTable,
    references: BTreeMap<(SourceUnitId, usize, usize, u8), UnitReferenceTarget>,
    symbols_by_span: Vec<BTreeMap<(usize, usize), SymbolId>>,
    symbol_types: BTreeMap<UnitSymbolId, UnitTypeId>,
    nominal_types: BTreeMap<DeclarationId, UnitTypeId>,
    nominal_by_root: BTreeMap<(SourceUnitId, usize), DeclarationId>,
    nominals: BTreeMap<DeclarationId, UnitNominalSignature>,
    type_parameters: BTreeMap<UnitSymbolId, UnitTypeParameterDescriptor>,
    interface_edge_spans: BTreeMap<(DeclarationId, DeclarationId), Span>,
    type_ref_types: BTreeMap<(SourceUnitId, usize), UnitTypeId>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> SignatureCollector<'a> {
    fn new(
        sources: &'a SourceMap,
        inputs: Vec<&'a ParsedFile>,
        names: &'a crate::name_resolution::CompilationUnitNames,
        environment: &'a TypeEnvironment,
    ) -> Result<Self, CompilationUnitTypeError> {
        let mut references = BTreeMap::new();
        for reference in names.references() {
            let Some(namespace) = reference.namespace() else {
                continue;
            };
            let key = (
                reference.source_unit(),
                reference.span().start(),
                reference.span().end(),
                namespace_rank(namespace),
            );
            if let Some(previous) = references.insert(key, reference.target().clone())
                && previous != *reference.target()
            {
                return Err(CompilationUnitTypeError::MismatchedInputs);
            }
        }
        let symbols_by_span = names
            .source_units()
            .iter()
            .map(|source| {
                source
                    .resolution()
                    .symbols()
                    .iter()
                    .map(|symbol| ((symbol.span().start(), symbol.span().end()), symbol.id()))
                    .collect()
            })
            .collect();
        Ok(Self {
            sources,
            inputs,
            names,
            environment,
            types: UnitTypeTable::new(),
            references,
            symbols_by_span,
            symbol_types: BTreeMap::new(),
            nominal_types: BTreeMap::new(),
            nominal_by_root: BTreeMap::new(),
            nominals: BTreeMap::new(),
            type_parameters: BTreeMap::new(),
            interface_edge_spans: BTreeMap::new(),
            type_ref_types: BTreeMap::new(),
            diagnostics: Vec::new(),
        })
    }

    fn run(mut self) -> Result<CompilationUnitSignatures, CompilationUnitTypeError> {
        self.predeclare_type_parameters();
        self.predeclare_nominals()?;
        self.collect_nominal_signatures()?;
        self.collect_type_parameter_bounds()?;
        self.check_interface_cycles()?;
        self.compute_interface_closures()?;
        let declarations = self.collect_declarations()?;
        self.validate_type_argument_bounds()?;
        self.check_duplicate_top_level_shapes(&declarations)?;
        let diagnostics = ordered_unit_diagnostics(
            self.sources,
            self.names.index().source_units(),
            &self.diagnostics,
        )?
        .into_iter()
        .cloned()
        .collect();
        Ok(CompilationUnitSignatures::new(
            self.types,
            declarations,
            self.symbol_types,
            self.type_parameters,
            diagnostics,
        ))
    }

    fn predeclare_type_parameters(&mut self) {
        for source in self.names.source_units() {
            for symbol in source.resolution().symbols() {
                if symbol.kind() != SymbolKind::TypeParameter {
                    continue;
                }
                let unit_symbol = UnitSymbolId::new(source.source_unit(), symbol.id());
                let ty = self.types.intern(UnitTypeKind::TypeParameter(unit_symbol));
                self.symbol_types.insert(unit_symbol, ty);
                self.type_parameters
                    .insert(unit_symbol, UnitTypeParameterDescriptor::new(unit_symbol));
            }
        }
    }

    fn predeclare_nominals(&mut self) -> Result<(), CompilationUnitTypeError> {
        for declaration in self.names.index().declarations() {
            if declaration.namespace() != Namespace::Type
                || declaration.kind() != SymbolKind::Classifier
            {
                continue;
            }
            let source = declaration.source_unit();
            let item = unwrapped_item(self.inputs[source.index()].ast(), declaration.root())?;
            let Item::Classifier(classifier) = item else {
                return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
            };
            let symbol = self
                .names
                .declaration_symbol(declaration.id())
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            let type_parameters = classifier
                .type_parameters
                .iter()
                .filter_map(|parameter| self.marker_symbol(source, parameter.name))
                .collect::<Vec<_>>();
            let arguments = type_parameters
                .iter()
                .map(|symbol| {
                    self.symbol_types
                        .get(symbol)
                        .copied()
                        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let ty = self.types.intern(UnitTypeKind::Nominal {
                declaration: declaration.id(),
                arguments,
            });
            self.symbol_types.insert(symbol, ty);
            self.nominal_types.insert(declaration.id(), ty);
            self.nominal_by_root
                .insert((source, declaration.root().index()), declaration.id());
            self.nominals.insert(
                declaration.id(),
                UnitNominalSignature::new(
                    declaration.id(),
                    symbol,
                    nominal_kind(&classifier.kind),
                    ty,
                    type_parameters,
                ),
            );
        }
        Ok(())
    }

    fn collect_nominal_signatures(&mut self) -> Result<(), CompilationUnitTypeError> {
        let ids = self.nominals.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let declaration = &self.names.index().declarations()[id.index()];
            let source = declaration.source_unit();
            let item = unwrapped_item(self.inputs[source.index()].ast(), declaration.root())?;
            let Item::Classifier(classifier) = item else {
                return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
            };
            let interfaces = self.collect_direct_interfaces(source, classifier, id)?;
            let mut fields = Vec::new();
            if let Some(constructor) = &classifier.primary_constructor {
                for field in &constructor.fields {
                    let Some(symbol) = self.marker_symbol(source, field.name) else {
                        continue;
                    };
                    let ty = self.resolve_type_ref(source, field.type_ref)?;
                    self.symbol_types.insert(symbol, ty);
                    fields.push(UnitFieldSignature::new(
                        symbol,
                        self.marker_text(field.name)?.unwrap_or_default(),
                        ty,
                        marker_span(field.name),
                    ));
                }
            }
            let enum_cases = self.collect_enum_cases(source, classifier, id)?;
            let mut members = Vec::new();
            if let Some(body) = &classifier.body {
                self.collect_member_callables(source, body, &mut members)?;
            }
            self.check_duplicate_member_shapes(&members)?;
            let nominal = self
                .nominals
                .get_mut(&id)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            nominal.set_direct_interfaces(interfaces);
            nominal.set_fields(fields);
            nominal.set_enum_cases(enum_cases);
            nominal.set_members(members);
        }
        Ok(())
    }

    fn collect_enum_cases(
        &mut self,
        source: SourceUnitId,
        classifier: &ClassifierDeclaration,
        nominal: DeclarationId,
    ) -> Result<Vec<UnitEnumCaseSignature>, CompilationUnitTypeError> {
        let Some(body) = &classifier.body else {
            return Ok(Vec::new());
        };
        let root = self
            .names
            .declaration_symbol(nominal)
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        let root_type = self.nominal_types[&nominal];
        let cases = self.names.source_units()[source.index()]
            .resolution()
            .enum_cases()
            .iter()
            .filter(|case| case.root() == root.symbol())
            .cloned()
            .collect::<Vec<_>>();
        let mut result = Vec::new();
        for case in cases {
            let variant = body
                .variants
                .iter()
                .find(|variant| variant.span == case.span())
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            let mut payloads = Vec::new();
            for (parameter, symbol) in variant.parameters.iter().zip(case.payloads()) {
                let unit_symbol = UnitSymbolId::new(source, *symbol);
                let ty = self.resolve_type_ref(source, parameter.type_ref)?;
                self.symbol_types.insert(unit_symbol, ty);
                payloads.push(UnitFieldSignature::new(
                    unit_symbol,
                    self.marker_text(parameter.name)?.unwrap_or_default(),
                    ty,
                    marker_span(parameter.name),
                ));
            }
            let value_symbol = UnitSymbolId::new(source, case.value_symbol());
            let type_symbol = UnitSymbolId::new(source, case.type_symbol());
            let case_type = self.types.intern(UnitTypeKind::EnumCase {
                case: type_symbol,
                root: root_type,
            });
            let value_type = if payloads.is_empty() {
                root_type
            } else {
                self.types.intern(UnitTypeKind::Function {
                    move_only: false,
                    parameters: payloads
                        .iter()
                        .map(|payload| {
                            UnitFunctionParameterType::new(ParameterMode::Value, payload.ty())
                        })
                        .collect(),
                    return_type: root_type,
                })
            };
            self.symbol_types.insert(type_symbol, case_type);
            self.symbol_types.insert(value_symbol, value_type);
            result.push(UnitEnumCaseSignature::new(
                value_symbol,
                type_symbol,
                self.marker_text(variant.name)?.unwrap_or_default(),
                marker_span(variant.name),
                case_type,
                value_type,
                payloads,
            ));
        }
        Ok(result)
    }

    fn collect_member_callables(
        &mut self,
        source: SourceUnitId,
        body: &ClassifierBody,
        output: &mut Vec<UnitCallableSignature>,
    ) -> Result<(), CompilationUnitTypeError> {
        for item in &body.members {
            match unwrapped_item(self.inputs[source.index()].ast(), *item)? {
                Item::Function { .. } => output.push(
                    self.callable_signature(
                        source,
                        *item,
                        UnitCallableTarget::Symbol(
                            self.item_symbol(source, *item)?
                                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?,
                        ),
                    )?,
                ),
                Item::Companion(companion) => {
                    self.collect_member_callables(source, &companion.body, output)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn collect_declarations(
        &mut self,
    ) -> Result<Vec<UnitDeclarationSignature>, CompilationUnitTypeError> {
        let mut signatures = Vec::with_capacity(self.names.index().declarations().len());
        for declaration in self.names.index().declarations() {
            let source = declaration.source_unit();
            let symbol = self
                .names
                .declaration_symbol(declaration.id())
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            let item = unwrapped_item(self.inputs[source.index()].ast(), declaration.root())?;
            let (ty, callable, nominal) = match item {
                Item::Function { .. } => {
                    let callable = self.callable_signature(
                        source,
                        declaration.root(),
                        UnitCallableTarget::Declaration(declaration.id()),
                    )?;
                    (callable.callable_type(), Some(callable), None)
                }
                Item::Variable { type_ref, .. } | Item::Constant { type_ref, .. } => {
                    let ty = match type_ref {
                        Some(type_ref) => self.resolve_type_ref(source, *type_ref)?,
                        None => self
                            .types
                            .intern(UnitTypeKind::Deferred(DeferredReason::ForwardValueType)),
                    };
                    (ty, None, None)
                }
                Item::Classifier(_) if declaration.namespace() == Namespace::Type => {
                    let nominal = self
                        .nominals
                        .get(&declaration.id())
                        .cloned()
                        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                    (nominal.ty(), None, Some(nominal))
                }
                Item::Classifier(_) => {
                    let nominal = self
                        .nominal_by_root
                        .get(&(source, declaration.root().index()))
                        .and_then(|id| self.nominal_types.get(id))
                        .copied()
                        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                    (nominal, None, None)
                }
                _ => return Err(CompilationUnitTypeError::MissingDeclarationSymbol),
            };
            self.symbol_types.insert(symbol, ty);
            signatures.push(UnitDeclarationSignature::new(
                declaration.id(),
                symbol,
                declaration.package(),
                ty,
                callable,
                nominal,
            ));
        }
        Ok(signatures)
    }

    fn callable_signature(
        &mut self,
        source: SourceUnitId,
        item: ItemId,
        target: UnitCallableTarget,
    ) -> Result<UnitCallableSignature, CompilationUnitTypeError> {
        let item = unwrapped_item(self.inputs[source.index()].ast(), item)?;
        let Item::Function {
            name,
            type_parameters,
            parameters,
            form,
            ..
        } = item
        else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        let name_span = marker_span(*name);
        let type_parameters = type_parameters
            .iter()
            .filter_map(|parameter| self.marker_symbol(source, parameter.name))
            .collect::<Vec<_>>();
        let parameters = parameters
            .iter()
            .map(|parameter| self.callable_parameter(source, parameter))
            .collect::<Result<Vec<_>, _>>()?;
        let return_type = match form {
            FunctionForm::ImplicitUnitAbsent | FunctionForm::ImplicitUnitBlock(_) => {
                self.builtin(BuiltinType::Unit)
            }
            FunctionForm::Explicit { type_ref, .. } => self.resolve_type_ref(source, *type_ref)?,
        };
        let callable_type = self.types.intern(UnitTypeKind::Function {
            move_only: false,
            parameters: parameters
                .iter()
                .map(|parameter| UnitFunctionParameterType::new(parameter.mode(), parameter.ty()))
                .collect(),
            return_type,
        });
        if let UnitCallableTarget::Symbol(symbol) = target {
            self.symbol_types.insert(symbol, callable_type);
        }
        Ok(UnitCallableSignature::new(
            target,
            self.marker_text(*name)?.unwrap_or_default(),
            name_span,
            type_parameters,
            parameters,
            return_type,
            callable_type,
        ))
    }

    fn callable_parameter(
        &mut self,
        source: SourceUnitId,
        parameter: &ValueParameter,
    ) -> Result<UnitCallableParameter, CompilationUnitTypeError> {
        let symbol = self.marker_symbol(source, parameter.name);
        let ty = self.resolve_type_ref(source, parameter.type_ref)?;
        if let Some(symbol) = symbol {
            self.symbol_types.insert(symbol, ty);
        }
        Ok(UnitCallableParameter::new(
            symbol,
            self.marker_text(parameter.name)?,
            parameter_mode(parameter.mode_marker),
            ty,
            parameter.span,
        ))
    }

    fn resolve_type_ref(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        if let Some(ty) = self.type_ref_types.get(&(source, id.index())).copied() {
            return Ok(ty);
        }
        let payload = self.inputs[source.index()]
            .ast()
            .type_refs()
            .get(id)
            .map_err(TypeCheckingError::from)?
            .payload()
            .clone();
        let ty = match payload {
            TypeRef::Error => self.error_type(),
            TypeRef::Function {
                move_span,
                parameters,
                return_type,
                ..
            } => {
                let parameters = parameters
                    .into_iter()
                    .map(|parameter| {
                        Ok(UnitFunctionParameterType::new(
                            parameter_mode(parameter.mode_marker),
                            self.resolve_type_ref(source, parameter.type_ref)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompilationUnitTypeError>>()?;
                let return_type = self.resolve_type_ref(source, return_type)?;
                self.types.intern(UnitTypeKind::Function {
                    move_only: move_span.is_some(),
                    parameters,
                    return_type,
                })
            }
            TypeRef::Qualified {
                segments,
                nullable_span,
            } => {
                let mut base = self.resolve_named_type(source, &segments)?;
                if nullable_span.is_some() && !self.is_error(base) {
                    base = self.types.intern(UnitTypeKind::Nullable(base));
                }
                base
            }
        };
        self.type_ref_types.insert((source, id.index()), ty);
        Ok(ty)
    }

    fn resolve_named_type(
        &mut self,
        source: SourceUnitId,
        segments: &[TypePathSegment],
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(segment) = segments.last() else {
            return Ok(self.error_type());
        };
        let arguments = segment
            .arguments
            .iter()
            .map(|argument| self.resolve_type_ref(source, *argument))
            .collect::<Result<Vec<_>, _>>()?;
        let target = self
            .reference(source, segment.name_span, Namespace::Type)
            .cloned();
        match target {
            Some(UnitReferenceTarget::Declaration(declaration)) => {
                self.instantiate_nominal(declaration, segment, arguments)
            }
            Some(UnitReferenceTarget::Symbol(symbol)) => {
                self.instantiate_unit_symbol(symbol, segment, arguments)
            }
            Some(UnitReferenceTarget::Symbols(symbols)) if symbols.len() == 1 => {
                self.instantiate_unit_symbol(symbols[0], segment, arguments)
            }
            Some(UnitReferenceTarget::External(external)) => {
                self.instantiate_external(source, external, segment, arguments)
            }
            _ => Ok(self.error_type()),
        }
    }

    fn instantiate_nominal(
        &mut self,
        declaration: DeclarationId,
        segment: &TypePathSegment,
        arguments: Vec<UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(nominal) = self.nominals.get(&declaration) else {
            return Ok(self.error_type());
        };
        if arguments.len() != nominal.type_parameters().len() {
            self.emit_arity(
                self.names.index().declarations()[declaration.index()].source_unit(),
                segment,
                self.names.index().declarations()[declaration.index()].name_span(),
                "nominal type has the wrong number of type arguments",
            )?;
            return Ok(self.error_type());
        }
        Ok(self.types.intern(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }))
    }

    fn instantiate_unit_symbol(
        &mut self,
        symbol: UnitSymbolId,
        segment: &TypePathSegment,
        arguments: Vec<UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(ty) = self.symbol_types.get(&symbol).copied() else {
            return Ok(self.error_type());
        };
        if !arguments.is_empty() {
            let label = self.names.source_units()[symbol.source_unit().index()]
                .resolution()
                .symbols()
                .get(symbol.symbol().index())
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
                .span();
            self.emit_arity(
                symbol.source_unit(),
                segment,
                label,
                "type parameter does not accept type arguments",
            )?;
            return Ok(self.error_type());
        }
        Ok(ty)
    }

    fn instantiate_external(
        &mut self,
        source: SourceUnitId,
        external: crate::name_resolution::ExternalSymbolId,
        segment: &TypePathSegment,
        arguments: Vec<UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        match self.environment.binding(external).cloned() {
            Some(ExternalTypeBinding::Builtin(builtin)) => {
                if arguments.is_empty() {
                    Ok(self.builtin(builtin))
                } else {
                    self.emit(
                        codes::BUILTIN_TYPE_ARGUMENTS,
                        "builtin type does not accept type arguments",
                        self.argument_primary(source, segment),
                    )?;
                    Ok(self.error_type())
                }
            }
            Some(ExternalTypeBinding::Capability(capability)) => {
                if arguments.is_empty() {
                    Ok(self.types.intern(UnitTypeKind::Capability(capability)))
                } else {
                    self.emit(
                        codes::TYPE_ARGUMENT_ARITY,
                        "compiler capability does not accept type arguments",
                        self.argument_primary(source, segment),
                    )?;
                    Ok(self.error_type())
                }
            }
            Some(ExternalTypeBinding::Intrinsic(constructor)) => {
                if arguments.len() == 1 {
                    Ok(self.types.intern(UnitTypeKind::Intrinsic {
                        constructor,
                        arguments,
                    }))
                } else {
                    self.emit(
                        codes::TYPE_ARGUMENT_ARITY,
                        "intrinsic type has the wrong number of type arguments",
                        segment.name_span,
                    )?;
                    Ok(self.error_type())
                }
            }
            Some(ExternalTypeBinding::Value(ty)) => {
                if arguments.is_empty() {
                    Ok(self.normalize_environment_type(&ty))
                } else {
                    self.emit(
                        codes::BUILTIN_TYPE_ARGUMENTS,
                        "external type does not accept type arguments",
                        self.argument_primary(source, segment),
                    )?;
                    Ok(self.error_type())
                }
            }
            _ => Ok(self.error_type()),
        }
    }

    fn normalize_environment_type(&mut self, ty: &EnvironmentType) -> UnitTypeId {
        match ty {
            EnvironmentType::Builtin(builtin) => self.builtin(*builtin),
            EnvironmentType::Nullable(inner) => {
                let inner = self.normalize_environment_type(inner);
                self.types.intern(UnitTypeKind::Nullable(inner))
            }
            EnvironmentType::Function {
                move_only,
                parameters,
                return_type,
            } => {
                let parameters = parameters
                    .iter()
                    .map(|parameter| {
                        let ty = self.normalize_environment_type(&parameter.ty);
                        UnitFunctionParameterType::new(parameter.mode, ty)
                    })
                    .collect();
                let return_type = self.normalize_environment_type(return_type);
                self.types.intern(UnitTypeKind::Function {
                    move_only: *move_only,
                    parameters,
                    return_type,
                })
            }
        }
    }

    fn check_duplicate_top_level_shapes(
        &mut self,
        declarations: &[UnitDeclarationSignature],
    ) -> Result<(), CompilationUnitTypeError> {
        for (primary, first) in duplicate_top_level_shapes(declarations, &self.types) {
            self.emit_with_label(
                codes::DUPLICATE_CALLABLE_SHAPE,
                "duplicate callable signature",
                primary,
                first,
                "first declaration with this shape is here",
            )?;
        }
        Ok(())
    }

    fn check_duplicate_member_shapes(
        &mut self,
        members: &[UnitCallableSignature],
    ) -> Result<(), CompilationUnitTypeError> {
        for (primary, first) in duplicate_member_shapes(members, &self.types) {
            self.emit_with_label(
                codes::DUPLICATE_CALLABLE_SHAPE,
                "duplicate callable signature",
                primary,
                first,
                "first declaration with this shape is here",
            )?;
        }
        Ok(())
    }

    fn item_symbol(
        &self,
        source: SourceUnitId,
        item: ItemId,
    ) -> Result<Option<UnitSymbolId>, CompilationUnitTypeError> {
        let item = unwrapped_item(self.inputs[source.index()].ast(), item)?;
        let marker = match item {
            Item::Function { name, .. }
            | Item::Variable { name, .. }
            | Item::Constant { name, .. } => Some(*name),
            Item::Classifier(classifier) => Some(classifier.name),
            _ => None,
        };
        Ok(marker.and_then(|marker| self.marker_symbol(source, marker)))
    }

    fn marker_symbol(&self, source: SourceUnitId, marker: NameMarker) -> Option<UnitSymbolId> {
        let NameMarker::Present(span) = marker else {
            return None;
        };
        self.symbols_by_span[source.index()]
            .get(&(span.start(), span.end()))
            .copied()
            .map(|symbol| UnitSymbolId::new(source, symbol))
    }

    fn marker_text(&self, marker: NameMarker) -> Result<Option<String>, CompilationUnitTypeError> {
        let NameMarker::Present(span) = marker else {
            return Ok(None);
        };
        self.sources
            .slice(span)
            .map(str::to_owned)
            .map(Some)
            .map_err(TypeCheckingError::from)
            .map_err(CompilationUnitTypeError::from)
    }

    fn reference(
        &self,
        source: SourceUnitId,
        span: Span,
        namespace: Namespace,
    ) -> Option<&UnitReferenceTarget> {
        self.references
            .get(&(source, span.start(), span.end(), namespace_rank(namespace)))
    }

    fn builtin(&mut self, builtin: BuiltinType) -> UnitTypeId {
        self.types.intern(UnitTypeKind::Builtin(builtin))
    }

    fn error_type(&mut self) -> UnitTypeId {
        self.types.intern(UnitTypeKind::Error)
    }

    fn is_error(&self, ty: UnitTypeId) -> bool {
        matches!(self.types.get(ty), Some(UnitTypeKind::Error))
    }

    fn nominal_declaration(&self, ty: UnitTypeId) -> Option<DeclarationId> {
        match self.types.get(ty) {
            Some(UnitTypeKind::Nominal { declaration, .. }) => Some(*declaration),
            _ => None,
        }
    }

    fn argument_primary(&self, source: SourceUnitId, segment: &TypePathSegment) -> Span {
        segment
            .arguments
            .first()
            .and_then(|argument| {
                self.inputs[source.index()]
                    .ast()
                    .type_refs()
                    .get(*argument)
                    .ok()
            })
            .map_or(segment.name_span, |node| node.span())
    }

    fn emit_arity(
        &mut self,
        source: SourceUnitId,
        segment: &TypePathSegment,
        label: Span,
        message: &str,
    ) -> Result<(), CompilationUnitTypeError> {
        self.emit_with_label(
            codes::TYPE_ARGUMENT_ARITY,
            message,
            self.argument_primary(source, segment),
            label,
            "type is declared here",
        )
    }

    fn emit(
        &mut self,
        code: &str,
        message: &str,
        primary: Span,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(code)?;
        self.diagnostics.push(Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            message,
            primary,
        )?);
        Ok(())
    }

    fn emit_with_label(
        &mut self,
        code: &str,
        message: &str,
        primary: Span,
        label: Span,
        label_message: &str,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(code)?;
        let mut diagnostic =
            Diagnostic::new(self.sources, Severity::Error, code, message, primary)?;
        diagnostic.add_label(self.sources, label, label_message)?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }
}

fn unwrapped_item(ast: &SyntaxAst, id: ItemId) -> Result<&Item, CompilationUnitTypeError> {
    let mut item = ast
        .items()
        .get(id)
        .map_err(TypeCheckingError::from)?
        .payload();
    while let Item::Modified { declaration, .. } = item {
        item = ast
            .items()
            .get(*declaration)
            .map_err(TypeCheckingError::from)?
            .payload();
    }
    Ok(item)
}

const fn namespace_rank(namespace: Namespace) -> u8 {
    match namespace {
        Namespace::Type => 0,
        Namespace::Value => 1,
    }
}

const fn parameter_mode(marker: Option<ParameterModeMarker>) -> ParameterMode {
    match marker {
        None | Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
        Some(ParameterModeMarker::Own(_)) => ParameterMode::Value,
        Some(ParameterModeMarker::Inout(_)) => ParameterMode::Inout,
    }
}

const fn marker_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
    }
}

const fn nominal_kind(kind: &ClassifierKind) -> NominalKind {
    match kind {
        ClassifierKind::ValueClass { .. } => NominalKind::ValueClass,
        ClassifierKind::Class { .. } => NominalKind::Class,
        ClassifierKind::Interface { .. } => NominalKind::Interface,
        ClassifierKind::EnumClass { .. } => NominalKind::EnumClass,
        ClassifierKind::Object { .. } => NominalKind::Object,
    }
}
