mod delegation;
mod expression;
mod item;
mod members;
mod nominal;
mod type_ref;

use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    diagnostic::{Diagnostic, DiagnosticCode, Severity, codes, ordered_diagnostics},
    name_resolution::{
        ExternalSymbolId, NameResolution, Namespace, ReferenceTarget, ScopeId, ScopeKind, SymbolId,
        SymbolKind,
    },
    parser::{ClassifierKind, Item, NameMarker, ParsedFile, SyntaxAst},
    source::{SourceMap, Span},
};

use super::{
    BuiltinType, CallableDescriptor, Capability, DeferredReason, DelegationPlan,
    EnvironmentFunction, EnvironmentType, ExternalTypeBinding, FunctionParameterType,
    NominalDescriptor, NominalId, NominalKind, ParameterMode, TypeCheckingError, TypeEnvironment,
    TypeId, TypeKind, TypeParameterBound, TypeParameterDescriptor, TypeTable, TypedFile,
    TypedFileParts,
};

#[derive(Clone, Copy)]
struct ExprCheck {
    ty: TypeId,
    falls_through: bool,
}

#[derive(Clone, Copy)]
struct StatementCheck {
    ty: TypeId,
    falls_through: bool,
}

#[derive(Clone, Copy)]
struct CallableContext {
    return_type: TypeId,
    annotation_span: Option<Span>,
}

pub(super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    environment: &TypeEnvironment,
) -> Result<TypedFile, TypeCheckingError> {
    sources.source_text(parsed.source_id())?;
    Checker::new(sources, parsed, names, environment)?.run()
}

struct Checker<'a> {
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
    environment: &'a TypeEnvironment,
    types: TypeTable,
    expression_types: Vec<Option<TypeId>>,
    type_ref_types: Vec<Option<TypeId>>,
    symbol_types: Vec<Option<TypeId>>,
    references: BTreeMap<(usize, usize, u8), ReferenceTarget>,
    symbols_by_span: BTreeMap<(usize, usize), SymbolId>,
    symbol_kinds: Vec<SymbolKind>,
    symbol_spans: Vec<Span>,
    symbol_scopes: Vec<ScopeId>,
    nominal_by_symbol: BTreeMap<SymbolId, NominalId>,
    nominal_by_scope: BTreeMap<ScopeId, NominalId>,
    classifier_scope_by_span: BTreeMap<(usize, usize), ScopeId>,
    nominals: Vec<NominalDescriptor>,
    type_parameters: Vec<TypeParameterDescriptor>,
    type_parameter_by_symbol: BTreeMap<SymbolId, usize>,
    delegations: Vec<DelegationPlan>,
    invalid_delegations: Vec<(NominalId, TypeId)>,
    typed_callables: Vec<CallableDescriptor>,
    interface_edge_spans: BTreeMap<(NominalId, NominalId), Span>,
    external_types: BTreeMap<ExternalSymbolId, TypeId>,
    callables: Vec<CallableContext>,
    classifiers: Vec<TypeId>,
    diagnostics: Vec<Diagnostic>,
    builtin_arguments_code: DiagnosticCode,
    cannot_infer_code: DiagnosticCode,
    mismatch_code: DiagnosticCode,
    operands_code: DiagnosticCode,
    return_outside_code: DiagnosticCode,
    return_shape_code: DiagnosticCode,
    missing_return_code: DiagnosticCode,
    branch_type_code: DiagnosticCode,
    numeric_range_code: DiagnosticCode,
    type_argument_arity_code: DiagnosticCode,
    invalid_type_bound_code: DiagnosticCode,
    interface_runtime_value_code: DiagnosticCode,
    invalid_supertype_code: DiagnosticCode,
    interface_cycle_code: DiagnosticCode,
    type_argument_bound_code: DiagnosticCode,
    duplicate_callable_shape_code: DiagnosticCode,
    concrete_member_body_code: DiagnosticCode,
    interface_member_mismatch_code: DiagnosticCode,
    invalid_override_code: DiagnosticCode,
    missing_interface_member_code: DiagnosticCode,
    default_member_conflict_code: DiagnosticCode,
    invalid_delegation_target_code: DiagnosticCode,
    delegate_interface_mismatch_code: DiagnosticCode,
    delegation_member_conflict_code: DiagnosticCode,
}

impl<'a> Checker<'a> {
    fn new(
        sources: &'a SourceMap,
        parsed: &'a ParsedFile,
        names: &'a NameResolution,
        environment: &'a TypeEnvironment,
    ) -> Result<Self, TypeCheckingError> {
        let catalog = codes::catalog()?;
        let mut references = BTreeMap::new();
        for reference in names.references() {
            references.insert(
                (
                    reference.span().start(),
                    reference.span().end(),
                    namespace_key(reference.namespace()),
                ),
                reference.target().clone(),
            );
        }
        let symbols_by_span = names
            .symbols()
            .iter()
            .map(|symbol| ((symbol.span().start(), symbol.span().end()), symbol.id()))
            .collect();
        let symbol_kinds = names.symbols().iter().map(|symbol| symbol.kind()).collect();
        let symbol_spans = names.symbols().iter().map(|symbol| symbol.span()).collect();
        let symbol_scopes = names
            .symbols()
            .iter()
            .map(|symbol| symbol.scope())
            .collect();
        let classifier_scope_by_span = names
            .scopes()
            .iter()
            .filter_map(|scope| {
                if scope.kind() == ScopeKind::Classifier {
                    scope
                        .span()
                        .map(|span| ((span.start(), span.end()), scope.id()))
                } else {
                    None
                }
            })
            .collect();
        Ok(Self {
            sources,
            parsed,
            environment,
            types: TypeTable::new(),
            expression_types: vec![None; parsed.ast().expressions().len()],
            type_ref_types: vec![None; parsed.ast().type_refs().len()],
            symbol_types: vec![None; names.symbols().len()],
            references,
            symbols_by_span,
            symbol_kinds,
            symbol_spans,
            symbol_scopes,
            nominal_by_symbol: BTreeMap::new(),
            nominal_by_scope: BTreeMap::new(),
            classifier_scope_by_span,
            nominals: Vec::new(),
            type_parameters: Vec::new(),
            type_parameter_by_symbol: BTreeMap::new(),
            delegations: Vec::new(),
            invalid_delegations: Vec::new(),
            typed_callables: Vec::new(),
            interface_edge_spans: BTreeMap::new(),
            external_types: BTreeMap::new(),
            callables: Vec::new(),
            classifiers: Vec::new(),
            diagnostics: Vec::new(),
            builtin_arguments_code: catalog.resolve(codes::BUILTIN_TYPE_ARGUMENTS)?,
            cannot_infer_code: catalog.resolve(codes::CANNOT_INFER_TYPE)?,
            mismatch_code: catalog.resolve(codes::TYPE_MISMATCH)?,
            operands_code: catalog.resolve(codes::INVALID_OPERAND_TYPES)?,
            return_outside_code: catalog.resolve(codes::RETURN_OUTSIDE_CALLABLE)?,
            return_shape_code: catalog.resolve(codes::RETURN_SHAPE_MISMATCH)?,
            missing_return_code: catalog.resolve(codes::MISSING_RETURN)?,
            branch_type_code: catalog.resolve(codes::NO_COMMON_BRANCH_TYPE)?,
            numeric_range_code: catalog.resolve(codes::NUMERIC_LITERAL_OUT_OF_RANGE)?,
            type_argument_arity_code: catalog.resolve(codes::TYPE_ARGUMENT_ARITY)?,
            invalid_type_bound_code: catalog.resolve(codes::INVALID_TYPE_BOUND)?,
            interface_runtime_value_code: catalog.resolve(codes::INTERFACE_RUNTIME_VALUE)?,
            invalid_supertype_code: catalog.resolve(codes::INVALID_SUPERTYPE)?,
            interface_cycle_code: catalog.resolve(codes::INTERFACE_CYCLE)?,
            type_argument_bound_code: catalog.resolve(codes::TYPE_ARGUMENT_BOUND)?,
            duplicate_callable_shape_code: catalog.resolve(codes::DUPLICATE_CALLABLE_SHAPE)?,
            concrete_member_body_code: catalog.resolve(codes::CONCRETE_MEMBER_BODY)?,
            interface_member_mismatch_code: catalog.resolve(codes::INTERFACE_MEMBER_MISMATCH)?,
            invalid_override_code: catalog.resolve(codes::INVALID_OVERRIDE)?,
            missing_interface_member_code: catalog.resolve(codes::MISSING_INTERFACE_MEMBER)?,
            default_member_conflict_code: catalog.resolve(codes::DEFAULT_MEMBER_CONFLICT)?,
            invalid_delegation_target_code: catalog.resolve(codes::INVALID_DELEGATION_TARGET)?,
            delegate_interface_mismatch_code: catalog
                .resolve(codes::DELEGATE_INTERFACE_MISMATCH)?,
            delegation_member_conflict_code: catalog.resolve(codes::DELEGATION_MEMBER_CONFLICT)?,
        })
    }

    fn run(mut self) -> Result<TypedFile, TypeCheckingError> {
        self.collect_nominals()?;
        self.check_type_parameter_bounds()?;
        self.check_direct_interfaces()?;
        self.check_interface_cycles()?;
        self.compute_interface_closures()?;
        self.predeclare_signatures()?;
        self.check_delegations()?;
        self.check_callable_shapes_and_bodies()?;
        for &root in self.parsed.roots() {
            self.check_item(root)?;
        }
        self.validate_type_argument_bounds()?;
        let error = self.error_type();
        let expression_types = self
            .expression_types
            .into_iter()
            .map(|ty| ty.unwrap_or(error))
            .collect();
        let type_ref_types = self
            .type_ref_types
            .into_iter()
            .map(|ty| ty.unwrap_or(error))
            .collect();
        let symbol_types = self
            .symbol_types
            .into_iter()
            .map(|ty| ty.unwrap_or(error))
            .collect();
        let diagnostics = ordered_diagnostics(self.sources, &self.diagnostics)?
            .into_iter()
            .cloned()
            .collect();
        Ok(TypedFile::new(
            self.parsed.source_id(),
            self.types,
            TypedFileParts {
                expression_types,
                type_ref_types,
                symbol_types,
                nominals: self.nominals,
                type_parameters: self.type_parameters,
                delegations: self.delegations,
                callables: self.typed_callables,
            },
            diagnostics,
        ))
    }

    fn check_direct_interfaces(&mut self) -> Result<(), TypeCheckingError> {
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

    fn check_interface_cycles(&mut self) -> Result<(), TypeCheckingError> {
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

    fn check_type_parameter_bounds(&mut self) -> Result<(), TypeCheckingError> {
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

    fn collect_nominals(&mut self) -> Result<(), TypeCheckingError> {
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

    fn ast(&self) -> &SyntaxAst {
        self.parsed.ast()
    }

    fn builtin(&mut self, builtin: BuiltinType) -> TypeId {
        self.types.intern(TypeKind::Builtin(builtin))
    }

    fn error_type(&mut self) -> TypeId {
        self.types.intern(TypeKind::Error)
    }

    fn deferred(&mut self, reason: DeferredReason) -> TypeId {
        self.types.intern(TypeKind::Deferred(reason))
    }

    fn kind(&self, id: TypeId) -> &TypeKind {
        self.types
            .get(id)
            .expect("TypeId is always allocated by this checker")
    }

    fn set_expression(&mut self, id: ExpressionId, ty: TypeId) {
        self.expression_types[id.index()] = Some(ty);
    }

    fn set_type_ref(&mut self, id: TypeRefId, ty: TypeId) {
        self.type_ref_types[id.index()] = Some(ty);
    }

    fn set_symbol(&mut self, id: SymbolId, ty: TypeId) {
        self.symbol_types[id.index()] = Some(ty);
    }

    fn symbol_type(&self, id: SymbolId) -> Option<TypeId> {
        self.symbol_types.get(id.index()).copied().flatten()
    }

    fn symbol_at(&self, span: Span) -> Option<SymbolId> {
        self.symbols_by_span
            .get(&(span.start(), span.end()))
            .copied()
    }

    fn reference(&self, span: Span, namespace: Namespace) -> Option<&ReferenceTarget> {
        self.references
            .get(&(span.start(), span.end(), namespace_key(namespace)))
    }

    fn external_type(&mut self, id: ExternalSymbolId) -> Result<TypeId, TypeCheckingError> {
        if let Some(ty) = self.external_types.get(&id).copied() {
            return Ok(ty);
        }
        let ty = match self.environment.binding(id).cloned() {
            Some(ExternalTypeBinding::Builtin(builtin)) => self.builtin(builtin),
            Some(ExternalTypeBinding::Capability(capability)) => {
                self.types.intern(TypeKind::Capability(capability))
            }
            Some(ExternalTypeBinding::Value(ty)) => self.normalize_environment_type(&ty),
            Some(ExternalTypeBinding::Function(signature)) => {
                self.normalize_environment_function(&signature)
            }
            None => self.deferred(DeferredReason::UnboundExternalType),
        };
        self.external_types.insert(id, ty);
        Ok(ty)
    }

    fn normalize_environment_function(&mut self, signature: &EnvironmentFunction) -> TypeId {
        let parameters = signature
            .parameters
            .iter()
            .map(|parameter| FunctionParameterType {
                mode: parameter.mode,
                ty: self.normalize_environment_type(&parameter.ty),
            })
            .collect();
        let return_type = self.normalize_environment_type(&signature.return_type);
        self.types.intern(TypeKind::Function {
            move_only: false,
            parameters,
            return_type,
        })
    }

    fn normalize_environment_type(&mut self, ty: &EnvironmentType) -> TypeId {
        match ty {
            EnvironmentType::Builtin(builtin) => self.builtin(*builtin),
            EnvironmentType::Nullable(inner) => {
                let inner = self.normalize_environment_type(inner);
                self.types.intern(TypeKind::Nullable(inner))
            }
            EnvironmentType::Function {
                move_only,
                parameters,
                return_type,
            } => {
                let parameters = parameters
                    .iter()
                    .map(|parameter| FunctionParameterType {
                        mode: parameter.mode,
                        ty: self.normalize_environment_type(&parameter.ty),
                    })
                    .collect();
                let return_type = self.normalize_environment_type(return_type);
                self.types.intern(TypeKind::Function {
                    move_only: *move_only,
                    parameters,
                    return_type,
                })
            }
        }
    }

    fn is_error(&self, ty: TypeId) -> bool {
        matches!(self.kind(ty), TypeKind::Error)
    }

    fn is_deferred(&self, ty: TypeId) -> bool {
        matches!(self.kind(ty), TypeKind::Deferred(_))
    }

    fn assignable(&self, actual: TypeId, expected: TypeId) -> bool {
        if actual == expected || self.is_error(actual) || self.is_error(expected) {
            return true;
        }
        if matches!(self.kind(actual), TypeKind::Builtin(BuiltinType::Nothing)) {
            return true;
        }
        match (self.kind(actual), self.kind(expected)) {
            (TypeKind::Builtin(BuiltinType::Nothing), _) => true,
            (TypeKind::Nullable(inner), TypeKind::Nullable(expected))
                if matches!(self.kind(*inner), TypeKind::Builtin(BuiltinType::Nothing)) =>
            {
                !self.is_deferred(*expected)
            }
            (TypeKind::Nullable(actual), TypeKind::Nullable(expected)) => actual == expected,
            (TypeKind::Builtin(_), TypeKind::Nullable(inner)) => actual == *inner,
            _ => false,
        }
    }

    fn join(&mut self, left: TypeId, right: TypeId) -> Option<TypeId> {
        if left == right {
            return Some(left);
        }
        if matches!(self.kind(left), TypeKind::Builtin(BuiltinType::Nothing)) {
            return Some(right);
        }
        if matches!(self.kind(right), TypeKind::Builtin(BuiltinType::Nothing)) {
            return Some(left);
        }
        match (self.kind(left).clone(), self.kind(right).clone()) {
            (TypeKind::Nullable(inner), _) if inner == right => Some(left),
            (_, TypeKind::Nullable(inner)) if inner == left => Some(right),
            (TypeKind::Error, _) => Some(right),
            (_, TypeKind::Error) => Some(left),
            _ => None,
        }
    }

    fn type_name(&self, ty: TypeId) -> String {
        match self.kind(ty) {
            TypeKind::Builtin(builtin) => builtin.name().to_owned(),
            TypeKind::Nullable(inner) => format!("{}?", self.type_name(*inner)),
            TypeKind::Function { .. } => "function type".to_owned(),
            TypeKind::Nominal { nominal, .. } => format!("nominal#{}", nominal.symbol().index()),
            TypeKind::TypeParameter(symbol) => format!("type-parameter#{}", symbol.index()),
            TypeKind::StaticSelf(interface) => format!("Self<{}>", self.type_name(*interface)),
            TypeKind::Capability(Capability::Copyable) => "Copyable".to_owned(),
            TypeKind::Capability(Capability::Transferable) => "Transferable".to_owned(),
            TypeKind::IntegerLiteral(_) => "integer literal".to_owned(),
            TypeKind::Error => "<error>".to_owned(),
            TypeKind::Deferred(reason) => format!("<deferred:{reason:?}>"),
        }
    }

    fn emit(
        &mut self,
        code: DiagnosticCode,
        message: &str,
        primary: Span,
    ) -> Result<(), TypeCheckingError> {
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
        code: DiagnosticCode,
        message: &str,
        primary: Span,
        label: Span,
        label_message: impl Into<String>,
    ) -> Result<(), TypeCheckingError> {
        let mut diagnostic =
            Diagnostic::new(self.sources, Severity::Error, code, message, primary)?;
        diagnostic.add_label(self.sources, label, label_message)?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn mismatch(
        &mut self,
        primary: Span,
        expected_span: Option<Span>,
        actual: TypeId,
        expected: TypeId,
    ) -> Result<(), TypeCheckingError> {
        if self.is_error(actual)
            || self.is_deferred(actual)
            || self.is_error(expected)
            || self.is_deferred(expected)
        {
            return Ok(());
        }
        let message = "expression type does not match the expected type";
        if let Some(label) = expected_span {
            self.emit_with_label(
                self.mismatch_code,
                message,
                primary,
                label,
                format!(
                    "expected {}, found {}",
                    self.type_name(expected),
                    self.type_name(actual)
                ),
            )
        } else {
            self.emit(self.mismatch_code, message, primary)
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

fn namespace_key(namespace: Namespace) -> u8 {
    match namespace {
        Namespace::Type => 0,
        Namespace::Value => 1,
    }
}
