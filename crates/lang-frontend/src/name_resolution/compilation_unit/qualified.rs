use std::collections::BTreeSet;

use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    name_resolution::{NameResolutionError, Namespace, ReferenceTarget, ScopeKind},
    parser::{
        ClassifierKind, Expression, Item, NameMarker, SyntaxAst, TypeRef, VisibilityModifier,
    },
    source::Span,
};

use super::{DeclarationBinding, UnitResolver, namespace_rank, source_unit_id, span_key};
use crate::name_resolution::{
    CompilationUnitNameError, DeclarationId, SourceUnitId, UnitNameReference, UnitReferenceTarget,
    UnitSymbolId,
};

impl UnitResolver<'_> {
    pub(super) fn resolve_qualified_paths(&mut self) -> Result<(), CompilationUnitNameError> {
        for source_index in 0..self.inputs.len() {
            self.resolve_qualified_type_paths(source_index)?;
            self.resolve_qualified_expression_paths(source_index)?;
        }
        Ok(())
    }

    fn resolve_qualified_type_paths(
        &mut self,
        source_index: usize,
    ) -> Result<(), CompilationUnitNameError> {
        let paths = self.inputs[source_index]
            .ast()
            .type_refs()
            .iter()
            .filter_map(|(_, node)| {
                let TypeRef::Qualified { segments, .. } = node.payload() else {
                    return None;
                };
                (segments.len() > 1).then(|| {
                    segments
                        .iter()
                        .map(|segment| segment.name_span)
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let source = source_unit_id(source_index);
        for spans in paths {
            if self.has_lexical_root(source, spans[0]) {
                continue;
            }
            if !self.try_relative_static_path(source, &spans, Namespace::Type)?
                && !self.has_external_root(source, spans[0])
            {
                self.try_absolute_qualified_path(source, &spans, Namespace::Type)?;
            }
        }
        Ok(())
    }

    fn resolve_qualified_expression_paths(
        &mut self,
        source_index: usize,
    ) -> Result<(), CompilationUnitNameError> {
        let ast = self.inputs[source_index].ast();
        let member_receivers = ast
            .expressions()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Expression::Member { receiver, .. } => Some(receiver.index()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let roots = ast
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                if member_receivers.contains(&id.index())
                    || !matches!(node.payload(), Expression::Member { safe: false, .. })
                {
                    return None;
                }
                flatten_member_path(ast, id)
            })
            .collect::<Vec<_>>();
        let source = source_unit_id(source_index);
        for spans in roots {
            if self.has_lexical_root(source, spans[0]) {
                self.resolve_local_constant_path(source, &spans)?;
                continue;
            }
            if !self.try_relative_static_path(source, &spans, Namespace::Value)?
                && !self.has_external_root(source, spans[0])
            {
                self.try_absolute_qualified_path(source, &spans, Namespace::Value)?;
            }
        }
        Ok(())
    }

    /// Local classifier roots already have lexical identity; only complete their constant tail.
    fn resolve_local_constant_path(
        &mut self,
        source: SourceUnitId,
        spans: &[Span],
    ) -> Result<(), CompilationUnitNameError> {
        if spans.len() != 2 {
            return Ok(());
        }
        let resolution = &self.local[source.index()];
        let Some(symbol) = resolution.references().iter().find_map(|reference| {
            if reference.span() != spans[0] {
                return None;
            }
            let ReferenceTarget::Symbol(symbol) = reference.target() else {
                return None;
            };
            resolution.symbols().get(symbol.index()).filter(|symbol| {
                matches!(
                    symbol.kind(),
                    crate::name_resolution::SymbolKind::Classifier
                        | crate::name_resolution::SymbolKind::ObjectValue
                )
            })
        }) else {
            return Ok(());
        };
        let Some(declaration) = self.index.declarations().iter().find(|declaration| {
            declaration.source_unit() == source
                && declaration.name_span() == symbol.span()
                && declaration.namespace() == Namespace::Type
        }) else {
            return Ok(());
        };
        let Some(&root) = self.declaration_symbols.get(&declaration.id()) else {
            return Ok(());
        };
        let member = self
            .sources
            .slice(spans[1])
            .map_err(NameResolutionError::from)?;
        let targets = self.static_members(
            root,
            member,
            Namespace::Value,
            self.is_object(declaration.id()),
            true,
        );
        // Local owner visibility belongs to the type checker; private functions are not missing.
        if targets.is_empty() {
            self.replace_qualified_root_with_error(source, spans, 1)?;
            return Ok(());
        }
        if let [target] = targets.as_slice()
            && self.local[target.source_unit().index()].symbols()[target.symbol().index()].kind()
                == crate::name_resolution::SymbolKind::Constant
        {
            self.references.push(UnitNameReference::new(
                source,
                spans[1],
                Some(Namespace::Value),
                UnitReferenceTarget::Symbol(*target),
            ));
            self.suppressed_unresolved[source.index()].insert(span_key(spans[1]));
        }
        Ok(())
    }

    fn has_lexical_root(&self, source: SourceUnitId, span: Span) -> bool {
        self.local[source.index()]
            .references()
            .iter()
            .any(|reference| {
                reference.span() == span
                    && matches!(
                        reference.target(),
                        ReferenceTarget::Symbol(_)
                            | ReferenceTarget::OverloadSet(_)
                            | ReferenceTarget::EnumCasePayloadCandidates(_)
                            | ReferenceTarget::LaterLocal(_)
                    )
            })
    }

    fn has_external_root(&self, source: SourceUnitId, span: Span) -> bool {
        self.local[source.index()]
            .references()
            .iter()
            .any(|reference| {
                reference.span() == span
                    && matches!(
                        reference.target(),
                        ReferenceTarget::External(_) | ReferenceTarget::ExternalOverloadSet(_)
                    )
            })
    }

    fn try_relative_static_path(
        &mut self,
        source: SourceUnitId,
        spans: &[Span],
        namespace: Namespace,
    ) -> Result<bool, CompilationUnitNameError> {
        let root_name = self
            .sources
            .slice(spans[0])
            .map_err(NameResolutionError::from)?;
        let Some(lookup) = self.lookup_file_binding(source, Namespace::Type, root_name) else {
            return Ok(false);
        };
        let binding = match lookup {
            Ok(binding) => binding,
            Err(candidates) => {
                self.suppressed_unresolved[source.index()].insert(span_key(spans[0]));
                self.push_wildcard_ambiguity(spans[0], &candidates)?;
                return Ok(true);
            }
        };
        self.references.push(UnitNameReference::new(
            source,
            spans[0],
            Some(Namespace::Type),
            binding.target(),
        ));
        let root_declaration = binding.0.first().copied();
        let mut target = root_declaration
            .and_then(|id| self.declaration_symbols.get(&id))
            .copied();
        if !self.resolve_static_tail(source, spans, 1, namespace, root_declaration, &mut target)? {
            return Ok(true);
        }
        for span in spans {
            self.suppressed_unresolved[source.index()].insert(span_key(*span));
        }
        Ok(true)
    }

    fn try_absolute_qualified_path(
        &mut self,
        source: SourceUnitId,
        spans: &[Span],
        namespace: Namespace,
    ) -> Result<(), CompilationUnitNameError> {
        let names = spans
            .iter()
            .map(|span| self.sources.slice(*span).map(str::to_owned))
            .collect::<Result<Vec<_>, _>>()
            .map_err(NameResolutionError::from)?;
        let Some((prefix_len, package)) = (1..names.len()).rev().find_map(|length| {
            self.package_ids
                .get(&names[..length])
                .copied()
                .map(|package| (length, package))
        }) else {
            return Ok(());
        };
        let terminal = &names[prefix_len];
        let remaining = &names[prefix_len + 1..];
        let selected_namespace = if remaining.is_empty() {
            namespace
        } else {
            Namespace::Type
        };
        let Some(binding) = self
            .declarations
            .get(&(
                package,
                namespace_rank(selected_namespace),
                terminal.clone(),
            ))
            .cloned()
        else {
            self.replace_qualified_root_with_error(source, spans, prefix_len)?;
            return Ok(());
        };
        let visible = self.visible_binding(&binding, source);
        if visible.0.is_empty() {
            self.replace_qualified_root_with_invisible(source, spans, prefix_len, &binding)?;
            return Ok(());
        }
        for span in &spans[..prefix_len] {
            self.references.push(UnitNameReference::new(
                source,
                *span,
                None,
                UnitReferenceTarget::Package(package),
            ));
        }
        self.references.push(UnitNameReference::new(
            source,
            spans[prefix_len],
            Some(selected_namespace),
            visible.target(),
        ));
        let root_declaration = visible.0.first().copied();
        let mut target = root_declaration
            .and_then(|id| self.declaration_symbols.get(&id))
            .copied();
        if !self.resolve_static_tail(
            source,
            spans,
            prefix_len + 1,
            namespace,
            root_declaration,
            &mut target,
        )? {
            return Ok(());
        }
        for span in spans {
            self.suppressed_unresolved[source.index()].insert(span_key(*span));
        }
        Ok(())
    }

    fn resolve_static_tail(
        &mut self,
        source: SourceUnitId,
        spans: &[Span],
        tail_start: usize,
        namespace: Namespace,
        root_declaration: Option<DeclarationId>,
        target: &mut Option<UnitSymbolId>,
    ) -> Result<bool, CompilationUnitNameError> {
        for (offset, span) in spans[tail_start..].iter().enumerate() {
            let index = tail_start + offset;
            let member_namespace = if index + 1 == spans.len() {
                namespace
            } else {
                Namespace::Type
            };
            let member = self
                .sources
                .slice(*span)
                .map_err(NameResolutionError::from)?;
            let allow_direct = offset == 0
                && root_declaration.is_some_and(|declaration| self.is_object(declaration));
            let resolved = target.map_or_else(Vec::new, |root| {
                self.static_members(root, member, member_namespace, allow_direct, false)
            });
            if resolved.is_empty() || (index + 1 < spans.len() && resolved.len() != 1) {
                self.replace_qualified_root_with_error(source, spans, index)?;
                return Ok(false);
            }
            let reference_target = if resolved.len() == 1 {
                UnitReferenceTarget::Symbol(resolved[0])
            } else {
                UnitReferenceTarget::Symbols(resolved.clone())
            };
            self.references.push(UnitNameReference::new(
                source,
                *span,
                Some(member_namespace),
                reference_target,
            ));
            *target = (resolved.len() == 1).then_some(resolved[0]);
        }
        Ok(true)
    }

    fn static_members(
        &self,
        root: UnitSymbolId,
        name: &str,
        namespace: Namespace,
        allow_direct: bool,
        include_private: bool,
    ) -> Vec<UnitSymbolId> {
        let resolution = &self.local[root.source_unit().index()];
        if let Some(case) = resolution.enum_cases().iter().find(|case| {
            if case.root() != root.symbol() {
                return false;
            }
            let symbol = match namespace {
                Namespace::Type => case.type_symbol(),
                Namespace::Value => case.value_symbol(),
            };
            resolution
                .symbols()
                .get(symbol.index())
                .is_some_and(|symbol| symbol.name() == name)
        }) {
            let symbol = match namespace {
                Namespace::Type => case.type_symbol(),
                Namespace::Value => case.value_symbol(),
            };
            return vec![UnitSymbolId::new(root.source_unit(), symbol)];
        }
        let Some(root_symbol) = resolution.symbols().get(root.symbol().index()) else {
            return Vec::new();
        };
        let Some(classifier_scope) = resolution.scopes().iter().find(|scope| {
            scope.kind() == ScopeKind::Classifier
                && scope.span().is_some_and(|span| {
                    span.start() <= root_symbol.span().start()
                        && span.end() >= root_symbol.span().end()
                })
        }) else {
            return Vec::new();
        };
        let mut scopes = Vec::new();
        if allow_direct {
            scopes.push(classifier_scope.id());
        }
        scopes.extend(
            resolution
                .scopes()
                .iter()
                .filter(|scope| {
                    scope.kind() == ScopeKind::Companion
                        && scope.span().is_some_and(|span| {
                            classifier_scope.span().is_some_and(|classifier_span| {
                                classifier_span.start() <= span.start()
                                    && classifier_span.end() >= span.end()
                            })
                        })
                })
                .map(|scope| scope.id()),
        );
        resolution
            .symbols()
            .iter()
            .filter(|symbol| {
                scopes.contains(&symbol.scope())
                    && symbol.namespace() == namespace
                    && symbol.name() == name
            })
            .map(|symbol| UnitSymbolId::new(root.source_unit(), symbol.id()))
            .filter(|symbol| include_private || self.static_symbol_visible(*symbol))
            .collect()
    }

    fn static_symbol_visible(&self, symbol: UnitSymbolId) -> bool {
        let source_unit = symbol.source_unit();
        let resolution = &self.local[source_unit.index()];
        let Some(symbol) = resolution.symbols().get(symbol.symbol().index()) else {
            return false;
        };
        // Constants are selected here, but their classifier visibility belongs to Phase 2 (L0154).
        if symbol.kind() == crate::name_resolution::SymbolKind::Constant {
            return true;
        }
        let ast = self.inputs[source_unit.index()].ast();
        for (_, node) in ast.items().iter() {
            let Item::Modified {
                modifiers,
                declaration,
            } = node.payload()
            else {
                continue;
            };
            let Ok(item) = ast.items().get(*declaration) else {
                continue;
            };
            if item_name_span(item.payload()) == Some(symbol.span()) {
                return !matches!(modifiers.visibility, Some(VisibilityModifier::Private(_)));
            }
        }
        true
    }

    fn is_object(&self, declaration: DeclarationId) -> bool {
        let declaration = &self.index.declarations()[declaration.index()];
        let ast = self.inputs[declaration.source_unit().index()].ast();
        let Ok(mut item) = ast
            .items()
            .get(declaration.root())
            .map(|node| node.payload())
        else {
            return false;
        };
        while let Item::Modified {
            declaration: inner, ..
        } = item
        {
            let Ok(node) = ast.items().get(*inner) else {
                return false;
            };
            item = node.payload();
        }
        matches!(
            item,
            Item::Classifier(classifier)
                if matches!(classifier.kind, ClassifierKind::Object { .. })
        )
    }

    fn replace_qualified_root_with_invisible(
        &mut self,
        source: SourceUnitId,
        spans: &[Span],
        failed: usize,
        binding: &DeclarationBinding,
    ) -> Result<(), CompilationUnitNameError> {
        self.suppressed_unresolved[source.index()].insert(span_key(spans[0]));
        self.references.push(UnitNameReference::new(
            source,
            spans[failed],
            None,
            UnitReferenceTarget::Unresolved,
        ));
        let mut diagnostic = self.new_diagnostic(
            codes::INVISIBLE_IMPORT_TARGET,
            "qualified target is not visible",
            spans[failed],
        )?;
        for declaration in &binding.0 {
            diagnostic.add_label(
                self.sources,
                self.index.declarations()[declaration.index()].name_span(),
                "private declaration is here",
            )?;
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn replace_qualified_root_with_error(
        &mut self,
        source: SourceUnitId,
        spans: &[Span],
        failed: usize,
    ) -> Result<(), CompilationUnitNameError> {
        self.suppressed_unresolved[source.index()].insert(span_key(spans[0]));
        self.references.push(UnitNameReference::new(
            source,
            spans[failed],
            None,
            UnitReferenceTarget::Unresolved,
        ));
        self.push_diagnostic(
            codes::UNRESOLVED_NAME,
            "unresolved qualified name",
            spans[failed],
        )
    }
}

fn item_name_span(item: &Item) -> Option<Span> {
    let marker = match item {
        Item::Variable { name, .. } | Item::Constant { name, .. } | Item::Function { name, .. } => {
            *name
        }
        Item::Classifier(classifier) => classifier.name,
        Item::Error | Item::Companion(_) | Item::Modified { .. } => return None,
    };
    match marker {
        NameMarker::Present(span) => Some(span),
        NameMarker::Missing(_) | NameMarker::Error(_) => None,
    }
}

fn flatten_member_path(ast: &SyntaxAst, mut expression: ExpressionId) -> Option<Vec<Span>> {
    let mut tail = Vec::new();
    loop {
        let node = ast.expressions().get(expression).ok()?;
        match node.payload() {
            Expression::Member {
                receiver,
                name_span,
                safe: false,
                ..
            } => {
                tail.push(*name_span);
                expression = *receiver;
            }
            Expression::Name => {
                tail.push(node.span());
                tail.reverse();
                return Some(tail);
            }
            _ => return None,
        }
    }
}
