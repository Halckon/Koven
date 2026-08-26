use std::collections::{BTreeMap, BTreeSet};

#[path = "qualified.rs"]
mod qualified;

use crate::{
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{
        NameEnvironment, NameResolution, NameResolutionError, Namespace, ReferenceTarget, SymbolId,
        resolve_names,
    },
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
};

use super::{
    CompilationUnitIndex, CompilationUnitNameError, CompilationUnitNames, DeclarationId,
    DeclarationVisibility, PackageId, SourceUnitId, SourceUnitInput, SourceUnitNames,
    UnitNameReference, UnitReferenceTarget, UnitSymbolId, index_compilation_unit,
    ordered_unit_diagnostics,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct DeclarationBinding(Vec<DeclarationId>);
impl DeclarationBinding {
    fn target(&self) -> UnitReferenceTarget {
        if self.0.len() == 1 {
            UnitReferenceTarget::Declaration(self.0[0])
        } else {
            UnitReferenceTarget::OverloadSet(self.0.clone())
        }
    }
}

#[derive(Default)]
struct FileBindings {
    automatic: BTreeMap<(u8, String), DeclarationBinding>,
    exact: BTreeMap<(u8, String), DeclarationBinding>,
    wildcards: Vec<PackageId>,
}

struct UnitResolver<'a> {
    sources: &'a SourceMap,
    inputs: Vec<&'a ParsedFile>,
    index: CompilationUnitIndex,
    local: Vec<NameResolution>,
    package_ids: BTreeMap<Vec<String>, PackageId>,
    declarations: BTreeMap<(PackageId, u8, String), DeclarationBinding>,
    declaration_symbols: BTreeMap<DeclarationId, UnitSymbolId>,
    bindings: Vec<FileBindings>,
    references: Vec<UnitNameReference>,
    diagnostics: Vec<Diagnostic>,
    suppressed_unresolved: Vec<BTreeSet<(usize, usize)>>,
}

/// 解析一个显式 compilation unit 的 import、可见性、限定路径与普通名称。
pub fn resolve_compilation_unit_names(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    index: &CompilationUnitIndex,
    environment: &NameEnvironment,
) -> Result<CompilationUnitNames, CompilationUnitNameError> {
    let rebuilt = index_compilation_unit(sources, inputs)?;
    if &rebuilt != index {
        return Err(CompilationUnitNameError::MismatchedIndex);
    }
    let parsed = index
        .source_units()
        .iter()
        .map(|unit| {
            inputs
                .iter()
                .find(|input| input.source_id() == unit.source_id())
                .map(|input| input.parsed())
                .ok_or(CompilationUnitNameError::MismatchedIndex)
        })
        .collect::<Result<Vec<_>, _>>()?;
    UnitResolver::new(sources, parsed, index.clone(), environment)?.run()
}

impl<'a> UnitResolver<'a> {
    fn new(
        sources: &'a SourceMap,
        inputs: Vec<&'a ParsedFile>,
        index: CompilationUnitIndex,
        environment: &'a NameEnvironment,
    ) -> Result<Self, CompilationUnitNameError> {
        let local = inputs
            .iter()
            .map(|parsed| resolve_names(sources, parsed, environment))
            .collect::<Result<Vec<_>, _>>()?;
        let package_ids = index
            .packages()
            .iter()
            .map(|package| (package.name().segments().to_vec(), package.id()))
            .collect();
        let mut declarations: BTreeMap<_, DeclarationBinding> = BTreeMap::new();
        for declaration in index.declarations() {
            declarations
                .entry((
                    declaration.package(),
                    namespace_rank(declaration.namespace()),
                    declaration.name().to_owned(),
                ))
                .or_insert_with(|| DeclarationBinding(Vec::new()))
                .0
                .push(declaration.id());
        }
        let mut declaration_symbols = BTreeMap::new();
        for declaration in index.declarations() {
            let resolution = &local[declaration.source_unit().index()];
            if let Some(symbol) = resolution.symbols().iter().find(|symbol| {
                symbol.span() == declaration.name_span()
                    && symbol.namespace() == declaration.namespace()
                    && symbol.kind() == declaration.kind()
            }) {
                declaration_symbols.insert(
                    declaration.id(),
                    UnitSymbolId::new(declaration.source_unit(), symbol.id()),
                );
            }
        }
        let unit_count = inputs.len();
        Ok(Self {
            sources,
            inputs,
            index,
            local,
            package_ids,
            declarations,
            declaration_symbols,
            bindings: (0..unit_count).map(|_| FileBindings::default()).collect(),
            references: Vec::new(),
            diagnostics: Vec::new(),
            suppressed_unresolved: (0..unit_count).map(|_| BTreeSet::new()).collect(),
        })
    }

    fn run(mut self) -> Result<CompilationUnitNames, CompilationUnitNameError> {
        self.build_automatic_bindings();
        self.resolve_imports()?;
        self.resolve_qualified_paths()?;
        self.resolve_file_references()?;

        let mut all_diagnostics = self.index.diagnostics().to_vec();
        let mut source_units = Vec::with_capacity(self.local.len());
        for (index, resolution) in self.local.into_iter().enumerate() {
            let source_unit = SourceUnitId(index);
            let filtered = resolution
                .diagnostics()
                .iter()
                .filter(|diagnostic| {
                    diagnostic.code().to_string() != codes::UNRESOLVED_NAME
                        || !self.suppressed_unresolved[index]
                            .contains(&span_key(diagnostic.primary_span()))
                })
                .cloned()
                .collect::<Vec<_>>();
            all_diagnostics.extend(filtered.iter().cloned());
            let resolution = NameResolution::new(
                resolution.source_id(),
                resolution.environment_owner().clone(),
                resolution.scopes().to_vec(),
                resolution.symbols().to_vec(),
                resolution.enum_cases().to_vec(),
                resolution.references().to_vec(),
                filtered,
            );
            source_units.push(SourceUnitNames::new(source_unit, resolution));
        }
        all_diagnostics.extend(self.diagnostics);
        let diagnostics =
            ordered_unit_diagnostics(self.sources, self.index.source_units(), &all_diagnostics)?
                .into_iter()
                .cloned()
                .collect();
        self.references.sort_by_key(|reference| {
            let unit = &self.index.source_units()[reference.source_unit().index()];
            (
                unit.key().clone(),
                reference.span().start(),
                reference.span().end(),
                reference.namespace().map(namespace_rank),
            )
        });
        Ok(CompilationUnitNames::new(
            self.index,
            source_units,
            self.references,
            diagnostics,
        ))
    }

    fn build_automatic_bindings(&mut self) {
        for source in self.index.source_units() {
            for ((package, namespace, name), binding) in &self.declarations {
                if *package != source.package() {
                    continue;
                }
                let visible = self.visible_binding(binding, source.id());
                if !visible.0.is_empty() {
                    self.bindings[source.id().index()]
                        .automatic
                        .insert((*namespace, name.clone()), visible);
                }
            }
        }
    }

    fn visible_binding(
        &self,
        binding: &DeclarationBinding,
        source: SourceUnitId,
    ) -> DeclarationBinding {
        DeclarationBinding(
            binding
                .0
                .iter()
                .copied()
                .filter(|id| {
                    let declaration = &self.index.declarations()[id.index()];
                    declaration.visibility() != DeclarationVisibility::Private
                        || declaration.source_unit() == source
                })
                .collect(),
        )
    }

    fn resolve_imports(&mut self) -> Result<(), CompilationUnitNameError> {
        for source_index in 0..self.inputs.len() {
            let source = SourceUnitId(source_index);
            for import in self.inputs[source_index].imports() {
                if import.segments.is_empty() {
                    continue;
                }
                let segments = import
                    .segments
                    .iter()
                    .map(|segment| self.sources.slice(segment.span).map(str::to_owned))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(NameResolutionError::from)?;
                if import.wildcard_span.is_some() {
                    self.resolve_wildcard_import(source, import, &segments)?;
                } else {
                    self.resolve_exact_import(source, import, &segments)?;
                }
            }
        }
        Ok(())
    }

    fn resolve_wildcard_import(
        &mut self,
        source: SourceUnitId,
        import: &crate::parser::ImportDirective,
        segments: &[String],
    ) -> Result<(), CompilationUnitNameError> {
        let Some(package) = self.package_ids.get(segments).copied() else {
            self.push_diagnostic(
                codes::UNRESOLVED_IMPORT_TARGET,
                "unresolved wildcard import package",
                import.segments.last().expect("non-empty import").span,
            )?;
            return Ok(());
        };
        for segment in &import.segments {
            self.references.push(UnitNameReference::new(
                source,
                segment.span,
                None,
                UnitReferenceTarget::Package(package),
            ));
        }
        if !self.bindings[source.index()].wildcards.contains(&package) {
            self.bindings[source.index()].wildcards.push(package);
        }
        Ok(())
    }

    fn resolve_exact_import(
        &mut self,
        source: SourceUnitId,
        import: &crate::parser::ImportDirective,
        segments: &[String],
    ) -> Result<(), CompilationUnitNameError> {
        if import.alias.is_some_and(|alias| alias.name_span.is_empty()) {
            return Ok(());
        }
        let (terminal, package_segments) = segments.split_last().expect("non-empty import");
        let Some(package) = self.package_ids.get(package_segments).copied() else {
            self.push_diagnostic(
                codes::UNRESOLVED_IMPORT_TARGET,
                "unresolved exact import target",
                import.segments.last().expect("terminal").span,
            )?;
            return Ok(());
        };
        for segment in &import.segments[..import.segments.len() - 1] {
            self.references.push(UnitNameReference::new(
                source,
                segment.span,
                None,
                UnitReferenceTarget::Package(package),
            ));
        }
        let local_name = match import.alias {
            Some(alias) => self
                .sources
                .slice(alias.name_span)
                .map_err(NameResolutionError::from)?,
            None => terminal,
        };
        let binding_span = import
            .alias
            .map_or(import.segments.last().expect("terminal").span, |alias| {
                alias.name_span
            });
        let terminal_span = import.segments.last().expect("terminal").span;
        let mut any_declared = false;
        let mut any_visible = false;
        for namespace in [Namespace::Type, Namespace::Value] {
            let key = (package, namespace_rank(namespace), terminal.clone());
            let Some(binding) = self.declarations.get(&key).cloned() else {
                continue;
            };
            any_declared = true;
            let visible = self.visible_binding(&binding, source);
            if visible.0.is_empty() {
                continue;
            }
            any_visible = true;
            self.references.push(UnitNameReference::new(
                source,
                terminal_span,
                Some(namespace),
                visible.target(),
            ));
            if let Some(alias) = import.alias {
                self.references.push(UnitNameReference::new(
                    source,
                    alias.name_span,
                    Some(namespace),
                    visible.target(),
                ));
            }
            self.insert_exact_binding(source, namespace, local_name, binding_span, visible)?;
        }
        if !any_declared {
            self.push_diagnostic(
                codes::UNRESOLVED_IMPORT_TARGET,
                "unresolved exact import target",
                terminal_span,
            )?;
        } else if !any_visible {
            let mut diagnostic = self.new_diagnostic(
                codes::INVISIBLE_IMPORT_TARGET,
                "import target is not visible",
                terminal_span,
            )?;
            for namespace in [Namespace::Type, Namespace::Value] {
                if let Some(binding) =
                    self.declarations
                        .get(&(package, namespace_rank(namespace), terminal.clone()))
                {
                    for id in &binding.0 {
                        diagnostic
                            .add_label(
                                self.sources,
                                self.index.declarations()[id.index()].name_span(),
                                "private declaration is here",
                            )
                            .map_err(CompilationUnitNameError::from)?;
                    }
                }
            }
            self.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    fn insert_exact_binding(
        &mut self,
        source: SourceUnitId,
        namespace: Namespace,
        local_name: &str,
        span: Span,
        binding: DeclarationBinding,
    ) -> Result<(), CompilationUnitNameError> {
        let key = (namespace_rank(namespace), local_name.to_owned());
        if let Some(existing) = self.bindings[source.index()].automatic.get(&key).cloned() {
            self.push_binding_conflict(span, &existing)?;
            return Ok(());
        }
        if let Some(existing) = self.bindings[source.index()].exact.get(&key).cloned() {
            if existing != binding {
                self.push_binding_conflict(span, &existing)?;
            }
            return Ok(());
        }
        self.bindings[source.index()].exact.insert(key, binding);
        Ok(())
    }

    fn push_binding_conflict(
        &mut self,
        span: Span,
        existing: &DeclarationBinding,
    ) -> Result<(), CompilationUnitNameError> {
        let mut diagnostic = self.new_diagnostic(
            codes::EXACT_IMPORT_BINDING_CONFLICT,
            "exact import binding conflicts with an existing file binding",
            span,
        )?;
        for id in &existing.0 {
            diagnostic
                .add_label(
                    self.sources,
                    self.index.declarations()[id.index()].name_span(),
                    "existing binding is declared here",
                )
                .map_err(CompilationUnitNameError::from)?;
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn resolve_file_references(&mut self) -> Result<(), CompilationUnitNameError> {
        for source_index in 0..self.local.len() {
            let source = SourceUnitId(source_index);
            let fallback = self.type_fallback_spans(source_index);
            let references = self.local[source_index].references().to_vec();
            for reference in references {
                if self.suppressed_unresolved[source_index].contains(&span_key(reference.span())) {
                    continue;
                }
                let target = match reference.target() {
                    ReferenceTarget::Symbol(symbol) => self.local_symbol_target(source, *symbol),
                    ReferenceTarget::OverloadSet(symbols) => {
                        self.local_symbols_target(source, symbols)
                    }
                    ReferenceTarget::External(id) => self
                        .file_binding_override(source, &reference)?
                        .unwrap_or(UnitReferenceTarget::External(*id)),
                    ReferenceTarget::ExternalOverloadSet(ids) => self
                        .file_binding_override(source, &reference)?
                        .unwrap_or_else(|| UnitReferenceTarget::ExternalOverloadSet(ids.clone())),
                    ReferenceTarget::EnumCasePayloadCandidates(ids) => {
                        UnitReferenceTarget::Symbols(
                            ids.iter()
                                .map(|id| UnitSymbolId::new(source, *id))
                                .collect(),
                        )
                    }
                    ReferenceTarget::LaterLocal(span) => UnitReferenceTarget::LaterLocal(*span),
                    ReferenceTarget::Unresolved => {
                        let name = self
                            .sources
                            .slice(reference.span())
                            .map_err(NameResolutionError::from)?;
                        let mut namespace = reference.namespace();
                        let mut lookup = self.lookup_file_binding(source, namespace, name);
                        if lookup.is_none()
                            && namespace == Namespace::Value
                            && fallback.contains(&span_key(reference.span()))
                        {
                            namespace = Namespace::Type;
                            lookup = self.lookup_file_binding(source, namespace, name);
                        }
                        match lookup {
                            Some(Ok(binding)) => {
                                self.suppressed_unresolved[source_index]
                                    .insert(span_key(reference.span()));
                                self.references.push(UnitNameReference::new(
                                    source,
                                    reference.span(),
                                    Some(namespace),
                                    binding.target(),
                                ));
                                continue;
                            }
                            Some(Err(candidates)) => {
                                self.suppressed_unresolved[source_index]
                                    .insert(span_key(reference.span()));
                                self.push_wildcard_ambiguity(reference.span(), &candidates)?;
                                UnitReferenceTarget::Unresolved
                            }
                            None => UnitReferenceTarget::Unresolved,
                        }
                    }
                };
                self.references.push(UnitNameReference::new(
                    source,
                    reference.span(),
                    Some(reference.namespace()),
                    target,
                ));
            }
        }
        Ok(())
    }

    fn lookup_file_binding(
        &self,
        source: SourceUnitId,
        namespace: Namespace,
        name: &str,
    ) -> Option<Result<DeclarationBinding, Vec<DeclarationBinding>>> {
        let key = (namespace_rank(namespace), name.to_owned());
        let file = &self.bindings[source.index()];
        if let Some(binding) = file.automatic.get(&key).or_else(|| file.exact.get(&key)) {
            return Some(Ok(binding.clone()));
        }
        let mut candidates = Vec::new();
        for package in &file.wildcards {
            if let Some(binding) = self.declarations.get(&(*package, key.0, name.to_owned())) {
                let visible = self.visible_binding(binding, source);
                if !visible.0.is_empty() && !candidates.contains(&visible) {
                    candidates.push(visible);
                }
            }
        }
        match candidates.len() {
            0 => None,
            1 => Some(Ok(candidates.remove(0))),
            _ => Some(Err(candidates)),
        }
    }

    fn file_binding_override(
        &mut self,
        source: SourceUnitId,
        reference: &crate::name_resolution::NameReference,
    ) -> Result<Option<UnitReferenceTarget>, CompilationUnitNameError> {
        let name = self
            .sources
            .slice(reference.span())
            .map_err(NameResolutionError::from)?;
        match self.lookup_file_binding(source, reference.namespace(), name) {
            Some(Ok(binding)) => Ok(Some(binding.target())),
            Some(Err(candidates)) => {
                self.push_wildcard_ambiguity(reference.span(), &candidates)?;
                Ok(Some(UnitReferenceTarget::Unresolved))
            }
            None => Ok(None),
        }
    }

    fn local_symbol_target(&self, source: SourceUnitId, symbol: SymbolId) -> UnitReferenceTarget {
        self.declaration_symbols
            .iter()
            .find_map(|(declaration, unit_symbol)| {
                (*unit_symbol == UnitSymbolId::new(source, symbol))
                    .then_some(UnitReferenceTarget::Declaration(*declaration))
            })
            .unwrap_or(UnitReferenceTarget::Symbol(UnitSymbolId::new(
                source, symbol,
            )))
    }

    fn local_symbols_target(
        &self,
        source: SourceUnitId,
        symbols: &[SymbolId],
    ) -> UnitReferenceTarget {
        let targets = symbols
            .iter()
            .map(|symbol| self.local_symbol_target(source, *symbol))
            .collect::<Vec<_>>();
        if targets
            .iter()
            .all(|target| matches!(target, UnitReferenceTarget::Declaration(_)))
        {
            let first = match targets.first() {
                Some(UnitReferenceTarget::Declaration(id)) => {
                    &self.index.declarations()[id.index()]
                }
                _ => unreachable!("all targets were checked as declarations"),
            };
            self.bindings[source.index()]
                .automatic
                .get(&(namespace_rank(first.namespace()), first.name().to_owned()))
                .map_or_else(
                    || {
                        UnitReferenceTarget::OverloadSet(
                            targets
                                .into_iter()
                                .map(|target| match target {
                                    UnitReferenceTarget::Declaration(id) => id,
                                    _ => unreachable!("all targets were checked as declarations"),
                                })
                                .collect(),
                        )
                    },
                    DeclarationBinding::target,
                )
        } else {
            UnitReferenceTarget::Symbols(
                symbols
                    .iter()
                    .map(|symbol| UnitSymbolId::new(source, *symbol))
                    .collect(),
            )
        }
    }

    fn type_fallback_spans(&self, source: usize) -> BTreeSet<(usize, usize)> {
        let ast = self.inputs[source].ast();
        let mut spans = BTreeSet::new();
        for (_, node) in ast.expressions().iter() {
            let candidate = match node.payload() {
                Expression::Call { callee, .. } => ast.expressions().get(*callee).ok(),
                Expression::Member { receiver, .. } => ast.expressions().get(*receiver).ok(),
                _ => None,
            };
            if let Some(candidate) =
                candidate.filter(|node| matches!(node.payload(), Expression::Name))
            {
                spans.insert(span_key(candidate.span()));
            }
        }
        spans
    }

    fn push_wildcard_ambiguity(
        &mut self,
        span: Span,
        candidates: &[DeclarationBinding],
    ) -> Result<(), CompilationUnitNameError> {
        let mut diagnostic = self.new_diagnostic(
            codes::AMBIGUOUS_WILDCARD_IMPORT,
            "ambiguous wildcard import",
            span,
        )?;
        for binding in candidates {
            for id in &binding.0 {
                diagnostic
                    .add_label(
                        self.sources,
                        self.index.declarations()[id.index()].name_span(),
                        "wildcard candidate is declared here",
                    )
                    .map_err(CompilationUnitNameError::from)?;
            }
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn new_diagnostic(
        &self,
        code: &str,
        message: &str,
        span: Span,
    ) -> Result<Diagnostic, CompilationUnitNameError> {
        let code = codes::catalog()?.resolve(code)?;
        Diagnostic::new(self.sources, Severity::Error, code, message, span)
            .map_err(CompilationUnitNameError::from)
    }
    fn push_diagnostic(
        &mut self,
        code: &str,
        message: &str,
        span: Span,
    ) -> Result<(), CompilationUnitNameError> {
        let diagnostic = self.new_diagnostic(code, message, span)?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }
}

const fn namespace_rank(namespace: Namespace) -> u8 {
    match namespace {
        Namespace::Type => 0,
        Namespace::Value => 1,
    }
}

pub(super) const fn source_unit_id(index: usize) -> SourceUnitId {
    SourceUnitId(index)
}

const fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}
