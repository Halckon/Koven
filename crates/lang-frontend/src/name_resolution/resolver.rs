use std::collections::BTreeMap;

mod constant_context;

use crate::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    diagnostic::{Diagnostic, DiagnosticCode, Severity, codes, ordered_diagnostics},
    parser::*,
    source::{SourceMap, Span},
};

use super::{
    EnumCase, EnumCaseId, ExternalBinding, NameEnvironment, NameReference, NameResolution,
    NameResolutionError, Namespace, ReferenceTarget, Scope, ScopeId, ScopeKind, Symbol, SymbolId,
    SymbolKind,
};

#[derive(Clone, Debug)]
enum Binding {
    Single(SymbolId),
    Functions(Vec<SymbolId>),
}

struct ScopeState {
    public: Scope,
    types: BTreeMap<String, Binding>,
    values: BTreeMap<String, Binding>,
    later_locals: Vec<(String, Span)>,
}

pub(super) fn resolve(
    sources: &SourceMap,
    parsed: &ParsedFile,
    environment: &NameEnvironment,
) -> Result<NameResolution, NameResolutionError> {
    // Validate the owner identity before any AST traversal or diagnostic construction.
    sources.source_text(parsed.source_id())?;
    Resolver::new(sources, parsed, environment)?.run()
}

struct Resolver<'a> {
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
    environment: &'a NameEnvironment,
    scopes: Vec<ScopeState>,
    symbols: Vec<Symbol>,
    enum_cases: Vec<EnumCase>,
    enum_case_by_root_name: BTreeMap<(SymbolId, String), EnumCaseId>,
    enum_case_by_span: BTreeMap<(usize, usize), EnumCaseId>,
    payload_candidates: BTreeMap<ScopeId, BTreeMap<String, Vec<SymbolId>>>,
    references: Vec<NameReference>,
    diagnostics: Vec<Diagnostic>,
    duplicate_code: DiagnosticCode,
    unresolved_code: DiagnosticCode,
    before_local_code: DiagnosticCode,
    constant_context_code: DiagnosticCode,
    companion_owners: BTreeMap<ScopeId, ScopeId>,
    constant_owner: Option<ScopeId>,
}

impl<'a> Resolver<'a> {
    fn new(
        sources: &'a SourceMap,
        parsed: &'a ParsedFile,
        environment: &'a NameEnvironment,
    ) -> Result<Self, NameResolutionError> {
        let catalog = codes::catalog()?;
        Ok(Self {
            sources,
            parsed,
            environment,
            scopes: Vec::new(),
            symbols: Vec::new(),
            enum_cases: Vec::new(),
            enum_case_by_root_name: BTreeMap::new(),
            enum_case_by_span: BTreeMap::new(),
            payload_candidates: BTreeMap::new(),
            references: Vec::new(),
            diagnostics: Vec::new(),
            duplicate_code: catalog.resolve(codes::DUPLICATE_NAME)?,
            unresolved_code: catalog.resolve(codes::UNRESOLVED_NAME)?,
            constant_context_code: catalog.resolve(codes::INVALID_CONSTANT_CONTEXT)?,
            companion_owners: BTreeMap::new(),
            constant_owner: None,
            before_local_code: catalog.resolve(codes::NAME_USED_BEFORE_LOCAL)?,
        })
    }

    fn run(mut self) -> Result<NameResolution, NameResolutionError> {
        let file = self.add_scope(None, ScopeKind::File, None);
        for &item in self.parsed.roots() {
            self.predeclare_item(item, file)?;
        }
        for &item in self.parsed.roots() {
            self.resolve_item(item, file)?;
        }
        let ordered = ordered_diagnostics(self.sources, &self.diagnostics)?
            .into_iter()
            .cloned()
            .collect();
        Ok(NameResolution::new(
            self.parsed.source_id(),
            self.environment.owner(),
            self.scopes.into_iter().map(|scope| scope.public).collect(),
            self.symbols,
            self.enum_cases,
            self.references,
            ordered,
        ))
    }

    fn ast(&self) -> &SyntaxAst {
        self.parsed.ast()
    }

    fn add_scope(
        &mut self,
        parent: Option<ScopeId>,
        kind: ScopeKind,
        span: Option<Span>,
    ) -> ScopeId {
        let id = ScopeId(self.scopes.len());
        self.scopes.push(ScopeState {
            public: Scope::new(id, parent, kind, span),
            types: BTreeMap::new(),
            values: BTreeMap::new(),
            later_locals: Vec::new(),
        });
        id
    }

    fn marker(&self, marker: NameMarker) -> Result<Option<(String, Span)>, NameResolutionError> {
        match marker {
            NameMarker::Present(span) => Ok(Some((self.sources.slice(span)?.to_owned(), span))),
            NameMarker::Missing(_) | NameMarker::Error(_) => Ok(None),
        }
    }

    fn insert_marker(
        &mut self,
        scope: ScopeId,
        marker: NameMarker,
        namespace: Namespace,
        kind: SymbolKind,
    ) -> Result<Option<SymbolId>, NameResolutionError> {
        let Some((name, span)) = self.marker(marker)? else {
            return Ok(None);
        };
        self.insert(scope, name, span, namespace, kind).map(Some)
    }

    fn insert_span(
        &mut self,
        scope: ScopeId,
        span: Span,
        namespace: Namespace,
        kind: SymbolKind,
    ) -> Result<Option<SymbolId>, NameResolutionError> {
        let name = self.sources.slice(span)?.to_owned();
        self.insert(scope, name, span, namespace, kind).map(Some)
    }

    fn insert(
        &mut self,
        scope: ScopeId,
        name: String,
        span: Span,
        namespace: Namespace,
        kind: SymbolKind,
    ) -> Result<SymbolId, NameResolutionError> {
        let id = SymbolId(self.symbols.len());
        self.symbols
            .push(Symbol::new(id, name.clone(), span, scope, namespace, kind));
        let is_function = kind == SymbolKind::Function;
        let table = match namespace {
            Namespace::Type => &mut self.scopes[scope.0].types,
            Namespace::Value => &mut self.scopes[scope.0].values,
        };
        match table.get_mut(&name) {
            None => {
                let binding = if is_function {
                    Binding::Functions(vec![id])
                } else {
                    Binding::Single(id)
                };
                table.insert(name, binding);
            }
            Some(Binding::Functions(ids)) if is_function => ids.push(id),
            Some(existing) => {
                let first = match existing {
                    Binding::Single(first) => *first,
                    Binding::Functions(ids) => ids[0],
                };
                if self.symbols[first.0].is_synthetic() && name == "it" && !is_function {
                    *existing = Binding::Single(id);
                    return Ok(id);
                }
                let first_span = self.symbols[first.0].span();
                self.push_duplicate(span, first_span)?;
            }
        }
        Ok(id)
    }

    fn insert_implicit_it(
        &mut self,
        scope: ScopeId,
        anchor: Span,
    ) -> Result<SymbolId, NameResolutionError> {
        let name = "it".to_owned();
        let id = SymbolId(self.symbols.len());
        self.symbols.push(Symbol::synthetic_lambda_parameter(
            id,
            name.clone(),
            anchor,
            scope,
        ));
        let previous = self.scopes[scope.0]
            .values
            .insert(name, Binding::Single(id));
        assert!(previous.is_none(), "fresh lambda scope must be empty");
        Ok(id)
    }

    fn push_duplicate(&mut self, span: Span, first: Span) -> Result<(), NameResolutionError> {
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.duplicate_code,
            "duplicate name in scope",
            span,
        )?;
        diagnostic.add_label(self.sources, first, "first declaration with this name")?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn predeclare_item(
        &mut self,
        item_id: ItemId,
        scope: ScopeId,
    ) -> Result<(), NameResolutionError> {
        let item = self.ast().items().get(item_id)?.payload().clone();
        match item {
            Item::Modified { declaration, .. } => self.predeclare_item(declaration, scope),
            Item::Variable { name, .. } => {
                self.insert_marker(scope, name, Namespace::Value, SymbolKind::Variable)?;
                Ok(())
            }
            Item::Constant { name, .. } => {
                self.insert_marker(scope, name, Namespace::Value, SymbolKind::Constant)?;
                Ok(())
            }
            Item::Function { name, .. } => {
                self.insert_marker(scope, name, Namespace::Value, SymbolKind::Function)?;
                Ok(())
            }
            Item::Classifier(classifier) => {
                self.insert_marker(
                    scope,
                    classifier.name,
                    Namespace::Type,
                    SymbolKind::Classifier,
                )?;
                if matches!(classifier.kind, ClassifierKind::Object { .. }) {
                    self.insert_marker(
                        scope,
                        classifier.name,
                        Namespace::Value,
                        SymbolKind::ObjectValue,
                    )?;
                }
                Ok(())
            }
            Item::Error | Item::Companion(_) => Ok(()),
        }
    }

    fn resolve_item(&mut self, item_id: ItemId, scope: ScopeId) -> Result<(), NameResolutionError> {
        let span = self.ast().items().get(item_id)?.span();
        let item = self.ast().items().get(item_id)?.payload().clone();
        let is_constant = matches!(&item, Item::Constant { .. });
        match item {
            Item::Error => Ok(()),
            Item::Modified { declaration, .. } => self.resolve_item(declaration, scope),
            Item::Variable {
                type_ref,
                initializer,
                ..
            }
            | Item::Constant {
                type_ref,
                initializer,
                ..
            } => {
                let previous = self.constant_owner;
                if is_constant {
                    self.constant_owner = self.companion_owners.get(&scope).copied();
                }
                let result = (|| {
                    if let Some(type_ref) = type_ref {
                        self.resolve_type(type_ref, scope)?;
                    }
                    self.resolve_expression(initializer, scope)
                })();
                self.constant_owner = previous;
                result
            }
            Item::Function {
                type_parameters,
                parameters,
                form,
                ..
            } => self.resolve_function(span, scope, &type_parameters, &parameters, form),
            Item::Classifier(classifier) => self.resolve_classifier(span, scope, &classifier),
            Item::Companion(companion) => self.resolve_companion(scope, &companion),
        }
    }

    fn resolve_function(
        &mut self,
        span: Span,
        parent: ScopeId,
        type_parameters: &[TypeParameter],
        parameters: &[ValueParameter],
        form: FunctionForm,
    ) -> Result<(), NameResolutionError> {
        let scope = self.add_scope(Some(parent), ScopeKind::Function, Some(span));
        self.resolve_type_parameters(type_parameters, scope)?;
        for parameter in parameters {
            self.resolve_type(parameter.type_ref, scope)?;
            self.insert_marker(
                scope,
                parameter.name,
                Namespace::Value,
                SymbolKind::ValueParameter,
            )?;
        }
        match form {
            FunctionForm::ImplicitUnitAbsent => Ok(()),
            FunctionForm::ImplicitUnitBlock(body) => self.resolve_statement(body, scope),
            FunctionForm::Explicit { type_ref, body, .. } => {
                self.resolve_type(type_ref, scope)?;
                match body {
                    FunctionBody::Absent => Ok(()),
                    FunctionBody::Expression { expression, .. } => {
                        self.resolve_expression(expression, scope)
                    }
                    FunctionBody::Block(body) => self.resolve_statement(body, scope),
                }
            }
        }
    }

    fn resolve_type_parameters(
        &mut self,
        parameters: &[TypeParameter],
        scope: ScopeId,
    ) -> Result<(), NameResolutionError> {
        for parameter in parameters {
            self.insert_marker(
                scope,
                parameter.name,
                Namespace::Type,
                SymbolKind::TypeParameter,
            )?;
            if let Some(bound) = parameter.bound {
                self.resolve_type(bound, scope)?;
            }
        }
        Ok(())
    }

    fn resolve_classifier(
        &mut self,
        span: Span,
        parent: ScopeId,
        classifier: &ClassifierDeclaration,
    ) -> Result<(), NameResolutionError> {
        let scope = self.add_scope(Some(parent), ScopeKind::Classifier, Some(span));
        let root = match classifier.name {
            NameMarker::Present(name_span) => {
                self.declaration_symbol(name_span, Namespace::Type, SymbolKind::Classifier)
            }
            _ => None,
        };
        self.resolve_type_parameters(&classifier.type_parameters, scope)?;
        if let Some(constructor) = &classifier.primary_constructor {
            for field in &constructor.fields {
                self.insert_marker(scope, field.name, Namespace::Value, SymbolKind::Field)?;
            }
        }
        if let Some(body) = &classifier.body {
            for variant in &body.variants {
                let (Some(root), NameMarker::Present(name_span)) = (root, variant.name) else {
                    continue;
                };
                let Some(type_symbol) = self.insert_marker(
                    scope,
                    variant.name,
                    Namespace::Type,
                    SymbolKind::EnumCaseType,
                )?
                else {
                    continue;
                };
                let Some(value_symbol) = self.insert_marker(
                    scope,
                    variant.name,
                    Namespace::Value,
                    SymbolKind::EnumVariant,
                )?
                else {
                    continue;
                };
                let id = EnumCaseId(self.enum_cases.len());
                let name = self.sources.slice(name_span)?.to_owned();
                self.enum_case_by_root_name.insert((root, name), id);
                self.enum_case_by_span
                    .insert((variant.span.start(), variant.span.end()), id);
                self.enum_cases.push(EnumCase::new(
                    id,
                    root,
                    value_symbol,
                    type_symbol,
                    variant.span,
                ));
            }
            for &member in &body.members {
                self.predeclare_item(member, scope)?;
            }
        }
        for supertype in &classifier.supertypes {
            self.resolve_type(supertype.type_ref, scope)?;
            if let Some(delegation) = supertype.delegation {
                self.resolve_marker_reference(delegation.target, scope, Namespace::Value)?;
            }
        }
        if let Some(constructor) = &classifier.primary_constructor {
            for field in &constructor.fields {
                self.resolve_type(field.type_ref, scope)?;
            }
        }
        if let Some(body) = &classifier.body {
            for variant in &body.variants {
                let variant_scope =
                    self.add_scope(Some(scope), ScopeKind::EnumVariant, Some(variant.span));
                let mut payloads = Vec::new();
                for parameter in &variant.parameters {
                    self.resolve_type(parameter.type_ref, variant_scope)?;
                    if let Some(symbol) = self.insert_marker(
                        variant_scope,
                        parameter.name,
                        Namespace::Value,
                        SymbolKind::ValueParameter,
                    )? {
                        payloads.push(symbol);
                        if let NameMarker::Present(name_span) = parameter.name {
                            let name = self.sources.slice(name_span)?.to_owned();
                            self.payload_candidates
                                .entry(scope)
                                .or_default()
                                .entry(name)
                                .or_default()
                                .push(symbol);
                        }
                    }
                }
                if let Some(id) = self
                    .enum_case_by_span
                    .get(&(variant.span.start(), variant.span.end()))
                    .copied()
                {
                    self.enum_cases[id.index()].set_payloads(payloads);
                }
            }
            for &member in &body.members {
                self.resolve_item(member, scope)?;
            }
        }
        Ok(())
    }

    fn resolve_companion(
        &mut self,
        classifier_scope: ScopeId,
        companion: &CompanionObject,
    ) -> Result<(), NameResolutionError> {
        // Companion is a type-level namespace: it skips the instance-member scope when looking outwards.
        let parent = self.scopes[classifier_scope.0].public.parent();
        let scope = self.add_scope(
            parent,
            ScopeKind::Companion,
            Some(companion.body.left_brace_span),
        );
        self.companion_owners.insert(scope, classifier_scope);
        for &member in &companion.body.members {
            self.predeclare_item(member, scope)?;
        }
        for &member in &companion.body.members {
            self.resolve_item(member, scope)?;
        }
        Ok(())
    }

    fn resolve_statement(
        &mut self,
        statement_id: StatementId,
        parent: ScopeId,
    ) -> Result<(), NameResolutionError> {
        let node = self.ast().statements().get(statement_id)?;
        let span = node.span();
        let statement = node.payload().clone();
        match statement {
            Statement::Error => Ok(()),
            Statement::Block { elements } => {
                let scope = self.add_scope(Some(parent), ScopeKind::Block, Some(span));
                self.resolve_sequence(&elements, scope)
            }
            Statement::LambdaBody { elements } => self.resolve_sequence(&elements, parent),
            Statement::ControlBody { elements } => {
                let scope = self.add_scope(Some(parent), ScopeKind::ControlBody, Some(span));
                self.resolve_sequence(&elements, scope)
            }
            Statement::LocalVariable { declaration } => {
                self.resolve_local_variable(declaration, parent)
            }
            Statement::LocalDestructuring {
                bindings,
                initializer,
                ..
            } => {
                self.resolve_expression(initializer, parent)?;
                for binding in bindings {
                    self.insert_marker(
                        parent,
                        binding,
                        Namespace::Value,
                        SymbolKind::DestructuringBinding,
                    )?;
                }
                Ok(())
            }
            Statement::While {
                condition, body, ..
            } => {
                self.resolve_expression(condition, parent)?;
                let scope = self.add_scope(Some(parent), ScopeKind::Loop, Some(span));
                self.resolve_statement(body, scope)
            }
            Statement::For {
                binding,
                source,
                body,
                ..
            } => {
                self.resolve_expression(source, parent)?;
                let scope = self.add_scope(Some(parent), ScopeKind::Loop, Some(span));
                self.insert_for_binding(&binding, scope)?;
                self.resolve_statement(body, scope)
            }
            Statement::Loop { body, .. } => {
                let scope = self.add_scope(Some(parent), ScopeKind::Loop, Some(span));
                self.resolve_statement(body, scope)
            }
            Statement::Expression { expression } => self.resolve_expression(expression, parent),
        }
    }

    fn resolve_sequence(
        &mut self,
        elements: &[StatementId],
        scope: ScopeId,
    ) -> Result<(), NameResolutionError> {
        let mut later = Vec::new();
        for &element in elements {
            self.collect_direct_local_names(element, &mut later)?;
        }
        self.scopes[scope.0].later_locals = later;
        for &element in elements {
            self.resolve_statement(element, scope)?;
        }
        Ok(())
    }

    fn collect_direct_local_names(
        &self,
        statement_id: StatementId,
        out: &mut Vec<(String, Span)>,
    ) -> Result<(), NameResolutionError> {
        match self.ast().statements().get(statement_id)?.payload() {
            Statement::LocalVariable { declaration } => {
                let mut item = self.ast().items().get(*declaration)?.payload();
                while let Item::Modified { declaration, .. } = item {
                    item = self.ast().items().get(*declaration)?.payload();
                }
                if let Item::Variable {
                    name: NameMarker::Present(span),
                    ..
                } = item
                {
                    let name = self.sources.slice(*span)?;
                    out.push((name.to_owned(), *span));
                }
            }
            Statement::LocalDestructuring { bindings, .. } => {
                for marker in bindings {
                    if let NameMarker::Present(span) = marker {
                        let name = self.sources.slice(*span)?;
                        out.push((name.to_owned(), *span));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn resolve_local_variable(
        &mut self,
        item_id: ItemId,
        scope: ScopeId,
    ) -> Result<(), NameResolutionError> {
        let item = self.ast().items().get(item_id)?.payload().clone();
        match item {
            Item::Modified { declaration, .. } => self.resolve_local_variable(declaration, scope),
            Item::Variable {
                name,
                type_ref,
                initializer,
                ..
            } => {
                if let Some(type_ref) = type_ref {
                    self.resolve_type(type_ref, scope)?;
                }
                self.resolve_expression(initializer, scope)?;
                self.insert_marker(scope, name, Namespace::Value, SymbolKind::Variable)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn insert_for_binding(
        &mut self,
        binding: &ForBinding,
        scope: ScopeId,
    ) -> Result<(), NameResolutionError> {
        match binding {
            ForBinding::Name(marker) => {
                self.insert_for_binding_marker(scope, *marker)?;
            }
            ForBinding::Destructuring { names, .. } => {
                for marker in names {
                    self.insert_for_binding_marker(scope, *marker)?;
                }
            }
        }
        Ok(())
    }

    fn insert_for_binding_marker(
        &mut self,
        scope: ScopeId,
        marker: NameMarker,
    ) -> Result<(), NameResolutionError> {
        let Some((name, span)) = self.marker(marker)? else {
            return Ok(());
        };
        if name != "_" {
            self.insert(scope, name, span, Namespace::Value, SymbolKind::ForBinding)?;
        }
        Ok(())
    }

    fn resolve_expression(
        &mut self,
        expression_id: ExpressionId,
        scope: ScopeId,
    ) -> Result<(), NameResolutionError> {
        let node = self.ast().expressions().get(expression_id)?;
        let span = node.span();
        let expression = node.payload().clone();
        match expression {
            Expression::This => self.check_constant_this(span),
            Expression::Error
            | Expression::Literal(_)
            | Expression::Break { .. }
            | Expression::Continue { .. } => Ok(()),
            Expression::Name => self.resolve_reference(span, scope, Namespace::Value),
            Expression::Group { expression } => self.resolve_expression(expression, scope),
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        self.resolve_expression(expression, scope)?;
                    }
                }
                Ok(())
            }
            Expression::Lambda {
                opener_span,
                parameters,
                arrow_span,
                body,
                ..
            } => {
                let lambda = self.add_scope(Some(scope), ScopeKind::Lambda, Some(span));
                if arrow_span.is_none() {
                    self.insert_implicit_it(lambda, opener_span)?;
                }
                for parameter in parameters {
                    self.insert_span(
                        lambda,
                        parameter,
                        Namespace::Value,
                        SymbolKind::LambdaParameter,
                    )?;
                }
                self.resolve_statement(body, lambda)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.resolve_expression(condition, scope)?;
                self.resolve_statement(then_branch, scope)?;
                if let Some(branch) = else_branch {
                    self.resolve_statement(branch, scope)?;
                }
                Ok(())
            }
            Expression::When {
                subject, entries, ..
            } => {
                if let Some(subject) = subject {
                    self.resolve_expression(subject, scope)?;
                }
                for entry in entries {
                    for condition in entry.conditions {
                        match condition {
                            WhenCondition::Expression(id)
                            | WhenCondition::Contains { expression: id, .. } => {
                                self.resolve_expression(id, scope)?
                            }
                            WhenCondition::TypeTest { type_ref, .. } => {
                                self.resolve_type(type_ref, scope)?
                            }
                        }
                    }
                    self.resolve_statement(entry.body, scope)?;
                }
                Ok(())
            }
            Expression::Return { value, .. } => {
                if let Some(value) = value {
                    self.resolve_expression(value, scope)?;
                }
                Ok(())
            }
            Expression::SuperMember { interface, .. } => self.resolve_type(interface, scope),
            Expression::Prefix { operand, .. } | Expression::NonNullAssert { operand, .. } => {
                self.resolve_expression(operand, scope)
            }
            Expression::Propagate { value, .. } => self.resolve_expression(value, scope),
            Expression::Cast {
                expression,
                type_ref,
                ..
            }
            | Expression::TypeTest {
                expression,
                type_ref,
                ..
            } => {
                self.resolve_expression(expression, scope)?;
                self.resolve_type(type_ref, scope)
            }
            Expression::Binary { left, right, .. } => {
                self.resolve_expression(left, scope)?;
                self.resolve_expression(right, scope)
            }
            Expression::Assignment { target, value, .. } => {
                self.resolve_expression(target, scope)?;
                self.resolve_expression(value, scope)
            }
            Expression::Member {
                receiver,
                name_span,
                ..
            } => {
                self.resolve_expression_with_type_fallback(receiver, scope)?;
                let receiver_node = self.ast().expressions().get(receiver)?;
                if matches!(receiver_node.payload(), Expression::This) {
                    if let Some(candidates) =
                        self.lookup_payload_candidates(scope, self.sources.slice(name_span)?)
                    {
                        self.references.push(NameReference::new(
                            name_span,
                            scope,
                            Namespace::Value,
                            ReferenceTarget::EnumCasePayloadCandidates(candidates),
                        ));
                    }
                } else if matches!(receiver_node.payload(), Expression::Name)
                    && let Some(ReferenceTarget::Symbol(root)) =
                        self.reference_target(receiver_node.span(), Namespace::Type)
                {
                    let name = self.sources.slice(name_span)?.to_owned();
                    if let Some(case) = self.enum_case_by_root_name.get(&(root, name.clone())) {
                        let symbol = self.enum_cases[case.index()].value_symbol();
                        self.references.push(NameReference::new(
                            name_span,
                            scope,
                            Namespace::Value,
                            ReferenceTarget::Symbol(symbol),
                        ));
                    } else if self.enum_cases.iter().any(|case| case.root() == root)
                        && !self.has_companion_constant(root, &name)?
                    {
                        self.diagnostics.push(Diagnostic::new(
                            self.sources,
                            Severity::Error,
                            self.unresolved_code,
                            "unresolved enum case value",
                            name_span,
                        )?);
                        self.references.push(NameReference::new(
                            name_span,
                            scope,
                            Namespace::Value,
                            ReferenceTarget::Unresolved,
                        ));
                    }
                }
                Ok(())
            }
            Expression::Call {
                callee,
                type_arguments,
                arguments,
                ..
            } => {
                self.resolve_expression_with_type_fallback(callee, scope)?;
                for type_ref in type_arguments {
                    self.resolve_type(type_ref, scope)?;
                }
                for argument in arguments {
                    self.resolve_expression(argument.value, scope)?;
                }
                Ok(())
            }
            Expression::Index { receiver, index } => {
                self.resolve_expression(receiver, scope)?;
                self.resolve_expression(index, scope)
            }
            Expression::CallableReference {
                receiver,
                name_span,
                ..
            } => {
                if let Some(receiver) = receiver {
                    self.resolve_expression(receiver, scope)
                } else {
                    self.resolve_reference(name_span, scope, Namespace::Value)
                }
            }
        }
    }

    fn resolve_type(
        &mut self,
        type_id: TypeRefId,
        scope: ScopeId,
    ) -> Result<(), NameResolutionError> {
        let type_ref = self.ast().type_refs().get(type_id)?.payload().clone();
        match type_ref {
            TypeRef::Error => Ok(()),
            TypeRef::Qualified { segments, .. } => {
                if let Some(first) = segments.first() {
                    self.resolve_reference(first.name_span, scope, Namespace::Type)?;
                }
                if let [first, second] = segments.as_slice()
                    && let Some(ReferenceTarget::Symbol(root)) =
                        self.reference_target(first.name_span, Namespace::Type)
                {
                    let name = self.sources.slice(second.name_span)?.to_owned();
                    if let Some(case) = self.enum_case_by_root_name.get(&(root, name)) {
                        let symbol = self.enum_cases[case.index()].type_symbol();
                        self.references.push(NameReference::new(
                            second.name_span,
                            scope,
                            Namespace::Type,
                            ReferenceTarget::Symbol(symbol),
                        ));
                    } else if self.enum_cases.iter().any(|case| case.root() == root) {
                        self.diagnostics.push(Diagnostic::new(
                            self.sources,
                            Severity::Error,
                            self.unresolved_code,
                            "unresolved enum case type",
                            second.name_span,
                        )?);
                        self.references.push(NameReference::new(
                            second.name_span,
                            scope,
                            Namespace::Type,
                            ReferenceTarget::Unresolved,
                        ));
                    }
                }
                for segment in segments {
                    for argument in segment.arguments {
                        self.resolve_type(argument, scope)?;
                    }
                }
                Ok(())
            }
            TypeRef::Function {
                parameters,
                return_type,
                ..
            } => {
                for parameter in parameters {
                    self.resolve_type(parameter.type_ref, scope)?;
                }
                self.resolve_type(return_type, scope)
            }
        }
    }

    /// Enum member syntax can denote a companion constant; defer its selection/visibility
    /// to Phase 2 instead of publishing an erroneous unresolved-case diagnostic.
    fn has_companion_constant(
        &self,
        root: SymbolId,
        name: &str,
    ) -> Result<bool, NameResolutionError> {
        let root_span = self.symbols[root.index()].span();
        for (_, node) in self.ast().items().iter() {
            let Item::Classifier(classifier) = node.payload() else {
                continue;
            };
            if !matches!(classifier.name, NameMarker::Present(span) if span == root_span) {
                continue;
            }
            let Some(body) = &classifier.body else {
                continue;
            };
            for &member in &body.members {
                let Item::Companion(companion) = self.unwrapped_associated_item(member)? else {
                    continue;
                };
                for &constant in &companion.body.members {
                    if let Item::Constant {
                        name: NameMarker::Present(span),
                        ..
                    } = self.unwrapped_associated_item(constant)?
                        && self.sources.slice(*span)? == name
                    {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    fn unwrapped_associated_item(&self, mut id: ItemId) -> Result<&Item, NameResolutionError> {
        loop {
            let item = self.ast().items().get(id)?.payload();
            if let Item::Modified { declaration, .. } = item {
                id = *declaration;
            } else {
                return Ok(item);
            }
        }
    }

    fn resolve_expression_with_type_fallback(
        &mut self,
        expression_id: ExpressionId,
        scope: ScopeId,
    ) -> Result<(), NameResolutionError> {
        let node = self.ast().expressions().get(expression_id)?;
        if matches!(node.payload(), Expression::Name) {
            let span = node.span();
            let name = self.sources.slice(span)?;
            if self.lookup(scope, Namespace::Value, name).is_none()
                && self.environment.lookup(Namespace::Value, name).is_none()
                && (self.lookup(scope, Namespace::Type, name).is_some()
                    || self.environment.lookup(Namespace::Type, name).is_some())
            {
                return self.resolve_reference(span, scope, Namespace::Type);
            }
        }
        self.resolve_expression(expression_id, scope)
    }

    fn resolve_marker_reference(
        &mut self,
        marker: NameMarker,
        scope: ScopeId,
        namespace: Namespace,
    ) -> Result<(), NameResolutionError> {
        if let NameMarker::Present(span) = marker {
            self.resolve_reference(span, scope, namespace)?;
        }
        Ok(())
    }

    fn resolve_reference(
        &mut self,
        span: Span,
        scope: ScopeId,
        namespace: Namespace,
    ) -> Result<(), NameResolutionError> {
        let name = self.sources.slice(span)?.to_owned();
        let target = if let Some(binding) = self.lookup(scope, namespace, &name) {
            Self::target(binding)
        } else if let Some(binding) = self.environment.lookup(namespace, &name) {
            match binding {
                ExternalBinding::Single(id) => ReferenceTarget::External(*id),
                ExternalBinding::Functions(ids) => {
                    ReferenceTarget::ExternalOverloadSet(ids.clone())
                }
            }
        } else if namespace == Namespace::Value {
            if let Some(candidates) = self.lookup_payload_candidates(scope, &name) {
                ReferenceTarget::EnumCasePayloadCandidates(candidates)
            } else if let Some(later) = self.lookup_later_local(scope, &name, span.start()) {
                let mut diagnostic = Diagnostic::new(
                    self.sources,
                    Severity::Error,
                    self.before_local_code,
                    "name used before local declaration",
                    span,
                )?;
                diagnostic.add_label(self.sources, later, "local declaration appears here")?;
                self.diagnostics.push(diagnostic);
                ReferenceTarget::LaterLocal(later)
            } else {
                self.report_unresolved_reference(span, namespace, &name)?;
                ReferenceTarget::Unresolved
            }
        } else {
            self.report_unresolved_reference(span, namespace, &name)?;
            ReferenceTarget::Unresolved
        };
        self.references
            .push(NameReference::new(span, scope, namespace, target));
        Ok(())
    }

    fn lookup(&self, mut scope: ScopeId, namespace: Namespace, name: &str) -> Option<&Binding> {
        loop {
            let state = &self.scopes[scope.0];
            let found = match namespace {
                Namespace::Type => state.types.get(name),
                Namespace::Value => state.values.get(name),
            };
            if found.is_some() {
                return found;
            }
            scope = state.public.parent()?;
        }
    }

    fn lookup_later_local(
        &self,
        mut scope: ScopeId,
        name: &str,
        reference_start: usize,
    ) -> Option<Span> {
        loop {
            let state = &self.scopes[scope.0];
            if let Some((_, span)) = state
                .later_locals
                .iter()
                .find(|(candidate, span)| candidate == name && span.start() > reference_start)
            {
                return Some(*span);
            }
            scope = state.public.parent()?;
        }
    }

    fn target(binding: &Binding) -> ReferenceTarget {
        match binding {
            Binding::Single(id) => ReferenceTarget::Symbol(*id),
            Binding::Functions(ids) => ReferenceTarget::OverloadSet(ids.clone()),
        }
    }

    fn declaration_symbol(
        &self,
        span: Span,
        namespace: Namespace,
        kind: SymbolKind,
    ) -> Option<SymbolId> {
        self.symbols
            .iter()
            .find(|symbol| {
                symbol.span() == span && symbol.namespace() == namespace && symbol.kind() == kind
            })
            .map(Symbol::id)
    }

    fn reference_target(&self, span: Span, namespace: Namespace) -> Option<ReferenceTarget> {
        self.references
            .iter()
            .rev()
            .find(|reference| reference.span() == span && reference.namespace() == namespace)
            .map(|reference| reference.target().clone())
    }

    fn lookup_payload_candidates(&self, mut scope: ScopeId, name: &str) -> Option<Vec<SymbolId>> {
        loop {
            if let Some(candidates) = self
                .payload_candidates
                .get(&scope)
                .and_then(|by_name| by_name.get(name))
            {
                return Some(candidates.clone());
            }
            scope = self.scopes[scope.index()].public.parent()?;
        }
    }
}
