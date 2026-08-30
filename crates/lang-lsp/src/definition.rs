//! 既有名称/类型事实到 source-local definition target 的确定性索引。

use std::{collections::BTreeMap, error::Error, fmt};

use lang_frontend::{
    ast::{AstError, ExpressionId},
    name_resolution::{
        CompilationUnitNames, DeclarationId, NameReference, NameResolution, ReferenceTarget,
        SourceUnitId, SymbolId, UnitNameReference, UnitReferenceTarget, UnitSymbolId,
    },
    parser::{Expression, ParsedFile, SyntaxAst},
    source::{SourceId, Span},
    type_checking::{
        AggregateProjectionKind, CallableTarget, CompilationUnitTypes, TypedFile,
        UnitAggregateProjectionKind, UnitCallTarget,
    },
};

pub(crate) struct DefinitionIndex {
    source_id: SourceId,
    entries: Vec<DefinitionEntry>,
}

struct DefinitionEntry {
    reference: Span,
    targets: Vec<Span>,
}

/// compilation-unit reference facts 到跨 source definition target 的确定性索引。
pub(crate) struct UnitDefinitionIndex {
    entries: Vec<UnitDefinitionEntry>,
}

struct UnitDefinitionEntry {
    source_unit: SourceUnitId,
    reference: Span,
    targets: Vec<UnitDefinitionTarget>,
}

/// 一个带 source-unit identity 的跨文件 definition target。
#[derive(Clone, Copy)]
pub(crate) struct UnitDefinitionTarget {
    pub(crate) source_unit: SourceUnitId,
    pub(crate) span: Span,
}

impl DefinitionIndex {
    pub(crate) fn build(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
    ) -> Result<Self, DefinitionIndexError> {
        let mut entries = BTreeMap::<(usize, usize), (Span, Vec<Span>)>::new();

        for symbol in names.symbols() {
            merge_targets(&mut entries, symbol.span(), [symbol.span()]);
        }
        for reference in names.references() {
            merge_targets(
                &mut entries,
                reference.span(),
                reference_targets(names, reference)?,
            );
        }

        for call in typed.calls() {
            let Some(target) = callable_target_symbol(call.target()) else {
                continue;
            };
            let Some(reference) = call_reference_span(parsed.ast(), call.expression())? else {
                continue;
            };
            replace_targets(&mut entries, reference, [symbol_span(names, target)?]);
        }
        for projection in typed.aggregate_projections() {
            let reference = match projection.kind() {
                AggregateProjectionKind::Field => {
                    member_name_span(parsed.ast(), projection.expression())?
                }
                AggregateProjectionKind::StructuralComponent => {
                    call_reference_span(parsed.ast(), projection.expression())?
                }
            };
            if let Some(reference) = reference {
                replace_targets(
                    &mut entries,
                    reference,
                    [symbol_span(names, projection.field())?],
                );
            }
        }

        let entries = entries
            .into_values()
            .filter_map(|(reference, mut targets)| {
                targets.sort_by_key(|span| (span.start(), span.end()));
                targets.dedup_by_key(|span| (span.start(), span.end()));
                (!targets.is_empty()).then_some(DefinitionEntry { reference, targets })
            })
            .collect();
        Ok(Self {
            source_id: parsed.source_id(),
            entries,
        })
    }

    pub(crate) const fn source_id(&self) -> SourceId {
        self.source_id
    }

    pub(crate) fn targets_at(&self, offset: usize) -> &[Span] {
        self.entries
            .iter()
            .filter(|entry| {
                entry.reference.start() <= offset
                    && offset < entry.reference.end()
                    && !entry.reference.is_empty()
            })
            .min_by_key(|entry| entry.reference.len())
            .map_or(&[], |entry| entry.targets.as_slice())
    }
}

impl UnitDefinitionIndex {
    pub(crate) fn build(
        parsed: &[ParsedFile],
        names: &CompilationUnitNames,
        typed: Option<&CompilationUnitTypes>,
    ) -> Result<Self, DefinitionIndexError> {
        let mut entries =
            BTreeMap::<(SourceUnitId, usize, usize), (Span, Vec<UnitDefinitionTarget>)>::new();

        for declaration in names.index().declarations() {
            merge_unit_targets(
                &mut entries,
                declaration.source_unit(),
                declaration.name_span(),
                [UnitDefinitionTarget {
                    source_unit: declaration.source_unit(),
                    span: declaration.name_span(),
                }],
            );
        }
        for source in names.source_units() {
            for symbol in source.resolution().symbols() {
                merge_unit_targets(
                    &mut entries,
                    source.source_unit(),
                    symbol.span(),
                    [UnitDefinitionTarget {
                        source_unit: source.source_unit(),
                        span: symbol.span(),
                    }],
                );
            }
        }
        for reference in names.references() {
            merge_unit_targets(
                &mut entries,
                reference.source_unit(),
                reference.span(),
                unit_reference_targets(names, reference)?,
            );
        }

        if let Some(typed) = typed {
            for call in typed.calls() {
                let Some(target) = unit_call_target(names, call.target())? else {
                    continue;
                };
                let expression = call.expression();
                let source = parsed_for_source(parsed, names, expression.source_unit())?;
                let Some(reference) = call_reference_span(source.ast(), expression.expression())?
                else {
                    continue;
                };
                replace_unit_targets(&mut entries, expression.source_unit(), reference, [target]);
            }
            for projection in typed.aggregate_projections() {
                let expression = projection.expression();
                let source = parsed_for_source(parsed, names, expression.source_unit())?;
                let reference = match projection.kind() {
                    UnitAggregateProjectionKind::Field => {
                        member_name_span(source.ast(), expression.expression())?
                    }
                    UnitAggregateProjectionKind::StructuralComponent => {
                        call_reference_span(source.ast(), expression.expression())?
                    }
                };
                if let Some(reference) = reference {
                    replace_unit_targets(
                        &mut entries,
                        expression.source_unit(),
                        reference,
                        [unit_symbol_target(names, projection.field())?],
                    );
                }
            }
        }

        let entries = entries
            .into_iter()
            .filter_map(|((source_unit, _, _), (reference, mut targets))| {
                targets.sort_by_key(|target| {
                    (target.source_unit, target.span.start(), target.span.end())
                });
                targets.dedup_by_key(|target| {
                    (target.source_unit, target.span.start(), target.span.end())
                });
                (!targets.is_empty()).then_some(UnitDefinitionEntry {
                    source_unit,
                    reference,
                    targets,
                })
            })
            .collect();
        Ok(Self { entries })
    }

    pub(crate) fn targets_at(
        &self,
        source_unit: SourceUnitId,
        offset: usize,
    ) -> &[UnitDefinitionTarget] {
        self.entries
            .iter()
            .filter(|entry| {
                entry.source_unit == source_unit
                    && entry.reference.start() <= offset
                    && offset < entry.reference.end()
                    && !entry.reference.is_empty()
            })
            .min_by_key(|entry| entry.reference.len())
            .map_or(&[], |entry| entry.targets.as_slice())
    }
}

fn reference_targets(
    names: &NameResolution,
    reference: &NameReference,
) -> Result<Vec<Span>, DefinitionIndexError> {
    let symbols: &[SymbolId] = match reference.target() {
        ReferenceTarget::Symbol(symbol) => std::slice::from_ref(symbol),
        ReferenceTarget::OverloadSet(symbols)
        | ReferenceTarget::EnumCasePayloadCandidates(symbols) => symbols,
        ReferenceTarget::LaterLocal(span) => return Ok(vec![*span]),
        ReferenceTarget::External(_)
        | ReferenceTarget::ExternalOverloadSet(_)
        | ReferenceTarget::Unresolved => return Ok(Vec::new()),
    };
    symbols
        .iter()
        .map(|symbol| symbol_span(names, *symbol))
        .collect()
}

fn unit_reference_targets(
    names: &CompilationUnitNames,
    reference: &UnitNameReference,
) -> Result<Vec<UnitDefinitionTarget>, DefinitionIndexError> {
    Ok(match reference.target() {
        UnitReferenceTarget::Declaration(declaration) => {
            vec![unit_declaration_target(names, *declaration)?]
        }
        UnitReferenceTarget::OverloadSet(declarations) => declarations
            .iter()
            .map(|declaration| unit_declaration_target(names, *declaration))
            .collect::<Result<_, _>>()?,
        UnitReferenceTarget::Symbol(symbol) => vec![unit_symbol_target(names, *symbol)?],
        UnitReferenceTarget::Symbols(symbols) => symbols
            .iter()
            .map(|symbol| unit_symbol_target(names, *symbol))
            .collect::<Result<_, _>>()?,
        UnitReferenceTarget::LaterLocal(span) => vec![UnitDefinitionTarget {
            source_unit: reference.source_unit(),
            span: *span,
        }],
        UnitReferenceTarget::Package(_)
        | UnitReferenceTarget::External(_)
        | UnitReferenceTarget::ExternalOverloadSet(_)
        | UnitReferenceTarget::Unresolved => Vec::new(),
    })
}

fn unit_call_target(
    names: &CompilationUnitNames,
    target: UnitCallTarget,
) -> Result<Option<UnitDefinitionTarget>, DefinitionIndexError> {
    Ok(match target {
        UnitCallTarget::Declaration(declaration) => {
            Some(unit_declaration_target(names, declaration)?)
        }
        UnitCallTarget::Symbol(symbol) | UnitCallTarget::StructuralComponent(symbol) => {
            Some(unit_symbol_target(names, symbol)?)
        }
        UnitCallTarget::External(_) | UnitCallTarget::FunctionValue => None,
    })
}

fn unit_declaration_target(
    names: &CompilationUnitNames,
    declaration: DeclarationId,
) -> Result<UnitDefinitionTarget, DefinitionIndexError> {
    let declaration = names
        .index()
        .declarations()
        .get(declaration.index())
        .filter(|candidate| candidate.id() == declaration)
        .ok_or(DefinitionIndexError::InvalidDeclaration(declaration))?;
    Ok(UnitDefinitionTarget {
        source_unit: declaration.source_unit(),
        span: declaration.name_span(),
    })
}

fn unit_symbol_target(
    names: &CompilationUnitNames,
    symbol: UnitSymbolId,
) -> Result<UnitDefinitionTarget, DefinitionIndexError> {
    let source = names
        .source_units()
        .get(symbol.source_unit().index())
        .filter(|source| source.source_unit() == symbol.source_unit())
        .ok_or(DefinitionIndexError::InvalidUnitSymbol(symbol))?;
    let local = source
        .resolution()
        .symbols()
        .get(symbol.symbol().index())
        .filter(|candidate| candidate.id() == symbol.symbol())
        .ok_or(DefinitionIndexError::InvalidUnitSymbol(symbol))?;
    Ok(UnitDefinitionTarget {
        source_unit: symbol.source_unit(),
        span: local.span(),
    })
}

fn parsed_for_source<'a>(
    parsed: &'a [ParsedFile],
    names: &CompilationUnitNames,
    source_unit: SourceUnitId,
) -> Result<&'a ParsedFile, DefinitionIndexError> {
    let indexed = names
        .index()
        .source_units()
        .get(source_unit.index())
        .filter(|source| source.id() == source_unit)
        .ok_or(DefinitionIndexError::InvalidSourceUnit(source_unit))?;
    parsed
        .get(source_unit.index())
        .filter(|source| source.source_id() == indexed.source_id())
        .ok_or(DefinitionIndexError::InvalidSourceUnit(source_unit))
}

fn callable_target_symbol(target: CallableTarget) -> Option<SymbolId> {
    match target {
        CallableTarget::Source(symbol) | CallableTarget::StructuralComponent(symbol) => {
            Some(symbol)
        }
        CallableTarget::External(_) | CallableTarget::FunctionValue => None,
    }
}

fn symbol_span(names: &NameResolution, symbol: SymbolId) -> Result<Span, DefinitionIndexError> {
    names
        .symbols()
        .get(symbol.index())
        .filter(|candidate| candidate.id() == symbol)
        .map(|candidate| candidate.span())
        .ok_or(DefinitionIndexError::InvalidSymbol(symbol))
}

fn call_reference_span(
    ast: &SyntaxAst,
    expression: ExpressionId,
) -> Result<Option<Span>, AstError> {
    let Expression::Call { callee, .. } = ast.expressions().get(expression)?.payload() else {
        return Ok(None);
    };
    identifier_span(ast, *callee)
}

fn identifier_span(
    ast: &SyntaxAst,
    mut expression: ExpressionId,
) -> Result<Option<Span>, AstError> {
    loop {
        let node = ast.expressions().get(expression)?;
        match node.payload() {
            Expression::Name => return Ok(Some(node.span())),
            Expression::Member { name_span, .. }
            | Expression::SuperMember { name_span, .. }
            | Expression::CallableReference { name_span, .. } => return Ok(Some(*name_span)),
            Expression::Group { expression: inner } => expression = *inner,
            _ => return Ok(None),
        }
    }
}

fn member_name_span(ast: &SyntaxAst, expression: ExpressionId) -> Result<Option<Span>, AstError> {
    let node = ast.expressions().get(expression)?;
    Ok(match node.payload() {
        Expression::Member { name_span, .. } => Some(*name_span),
        _ => None,
    })
}

fn merge_targets(
    entries: &mut BTreeMap<(usize, usize), (Span, Vec<Span>)>,
    reference: Span,
    targets: impl IntoIterator<Item = Span>,
) {
    entries
        .entry((reference.start(), reference.end()))
        .or_insert_with(|| (reference, Vec::new()))
        .1
        .extend(targets);
}

fn replace_targets(
    entries: &mut BTreeMap<(usize, usize), (Span, Vec<Span>)>,
    reference: Span,
    targets: impl IntoIterator<Item = Span>,
) {
    entries.insert(
        (reference.start(), reference.end()),
        (reference, targets.into_iter().collect()),
    );
}

fn merge_unit_targets(
    entries: &mut BTreeMap<(SourceUnitId, usize, usize), (Span, Vec<UnitDefinitionTarget>)>,
    source_unit: SourceUnitId,
    reference: Span,
    targets: impl IntoIterator<Item = UnitDefinitionTarget>,
) {
    entries
        .entry((source_unit, reference.start(), reference.end()))
        .or_insert_with(|| (reference, Vec::new()))
        .1
        .extend(targets);
}

fn replace_unit_targets(
    entries: &mut BTreeMap<(SourceUnitId, usize, usize), (Span, Vec<UnitDefinitionTarget>)>,
    source_unit: SourceUnitId,
    reference: Span,
    targets: impl IntoIterator<Item = UnitDefinitionTarget>,
) {
    entries.insert(
        (source_unit, reference.start(), reference.end()),
        (reference, targets.into_iter().collect()),
    );
}

#[derive(Debug)]
pub(crate) enum DefinitionIndexError {
    Ast(AstError),
    InvalidSymbol(SymbolId),
    InvalidDeclaration(DeclarationId),
    InvalidSourceUnit(SourceUnitId),
    InvalidUnitSymbol(UnitSymbolId),
}

impl fmt::Display for DefinitionIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ast(error) => write!(formatter, "definition index AST mismatch: {error}"),
            Self::InvalidSymbol(symbol) => {
                write!(
                    formatter,
                    "definition index references unknown symbol {symbol:?}"
                )
            }
            Self::InvalidDeclaration(declaration) => {
                write!(
                    formatter,
                    "definition index references unknown declaration {declaration:?}"
                )
            }
            Self::InvalidSourceUnit(source) => {
                write!(
                    formatter,
                    "definition index references unknown source unit {source:?}"
                )
            }
            Self::InvalidUnitSymbol(symbol) => {
                write!(
                    formatter,
                    "definition index references unknown unit symbol {symbol:?}"
                )
            }
        }
    }
}

impl Error for DefinitionIndexError {}

impl From<AstError> for DefinitionIndexError {
    fn from(error: AstError) -> Self {
        Self::Ast(error)
    }
}

#[cfg(test)]
mod tests {
    use lang_frontend::{
        lexer::lex,
        name_resolution::{
            SourceUnitId, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::{ParsedFile, parse_file},
        source::{SourceId, SourceMap},
        type_checking::{check_compilation_unit_types, standard_environments},
    };

    use crate::analysis::analyze;

    use super::UnitDefinitionIndex;

    #[test]
    fn indexes_declarations_names_types_enum_cases_later_locals_and_null_targets() {
        let source = "enum class Choice { First; }\n\
            fun choose(): Choice = Choice.First\n\
            fun later(): Unit {\n\
                val before = deferred\n\
                val deferred = 1\n\
            }\n\
            fun identity(choice: Choice): Choice = choice\n\
            fun external(value: List<Int>): Unit {}\n\
            fun missingUse(): Unit = missing";
        let analysis = analyze("file:///names.ko", source).expect("analysis");

        assert_target(source, &analysis, "Choice", 1, "Choice", 0);
        assert_target(source, &analysis, "Choice", 2, "Choice", 0);
        assert_target(source, &analysis, "First", 1, "First", 0);
        assert_target(source, &analysis, "deferred", 0, "deferred", 1);
        assert_target(source, &analysis, "choice", 1, "choice", 0);
        assert_no_target(source, &analysis, "List", 0);
        assert_no_target(source, &analysis, "missing", 1);
    }

    #[test]
    fn typed_calls_and_fields_narrow_overload_and_member_targets() {
        let source = "class Holder(val field: Int) { fun read(): Int = this.field }\n\
            fun pick(value: Int): Int = value\n\
            fun pick(value: String): String = value\n\
            fun failed(): Int = pick(true)\n\
            fun use(holder: Holder): Int = holder.read() + pick(1)";
        let analysis = analyze("file:///typed.ko", source).expect("analysis");

        assert_target(source, &analysis, "field", 1, "field", 0);
        assert_target(source, &analysis, "read", 1, "read", 0);
        assert_target(source, &analysis, "pick", 3, "pick", 0);

        let failed = occurrence(source, "pick", 2);
        let targets = analysis.definitions.targets_at(failed);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].start(), occurrence(source, "pick", 0));
        assert_eq!(targets[1].start(), occurrence(source, "pick", 1));
    }

    #[test]
    fn indexes_call_after_utf16_surrogate_content() {
        let source = "fun target(): Unit {}\nfun use(): Unit { /* 😀 */ target() }";
        let analysis = analyze("file:///unicode.ko", source).expect("analysis");

        assert_target(source, &analysis, "target", 1, "target", 0);
    }

    #[test]
    fn unit_typed_facts_narrow_overloads_and_cross_file_members() {
        let provider_source = "package lib\n\
            public class Holder(val field: Int) { fun read(): Int = this.field }\n\
            public fun pick(item: Int): Int = item\n\
            public fun pick(item: String): String = item";
        let consumer_source = "package app\nimport lib.Holder\nimport lib.pick\n\
            fun failed(): Int = pick(true)\n\
            fun use(holder: Holder): Int = holder.read() + holder.field + pick(1)";
        let mut sources = SourceMap::new();
        let (provider_id, provider) = parsed(&mut sources, "provider.ko", provider_source);
        let (consumer_id, consumer) = parsed(&mut sources, "consumer.ko", consumer_source);
        // UnitDefinitionIndex consumes ParsedFile in canonical SourceUnitId order, not caller order.
        let parsed = vec![consumer, provider];
        let inputs = [
            SourceUnitInput::new("root", "app/use.ko", consumer_id, &parsed[0]),
            SourceUnitInput::new("root", "lib/api.ko", provider_id, &parsed[1]),
        ];
        let (name_environment, type_environment) = standard_environments();
        let unit_index = index_compilation_unit(&sources, &inputs).expect("unit index");
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &unit_index, &name_environment)
                .expect("unit names");
        let validated_names = names.clone().validate().expect("validated names");
        let typed =
            check_compilation_unit_types(&sources, &inputs, &validated_names, &type_environment)
                .expect("unit types");
        let definitions =
            UnitDefinitionIndex::build(&parsed, &names, Some(&typed)).expect("definitions");
        let provider_unit = source_unit(&names, provider_id);
        let consumer_unit = source_unit(&names, consumer_id);

        assert_unit_target(
            &definitions,
            consumer_unit,
            occurrence(consumer_source, "pick", 2),
            provider_unit,
            occurrence(provider_source, "pick", 0),
        );
        assert_unit_target(
            &definitions,
            consumer_unit,
            occurrence(consumer_source, "read", 0),
            provider_unit,
            occurrence(provider_source, "read", 0),
        );
        assert_unit_target(
            &definitions,
            consumer_unit,
            occurrence(consumer_source, "field", 0),
            provider_unit,
            occurrence(provider_source, "field", 0),
        );

        let failed = definitions.targets_at(consumer_unit, occurrence(consumer_source, "pick", 1));
        assert_eq!(failed.len(), 2);
        assert_eq!(failed[0].source_unit, provider_unit);
        assert_eq!(
            failed[0].span.start(),
            occurrence(provider_source, "pick", 0)
        );
        assert_eq!(failed[1].source_unit, provider_unit);
        assert_eq!(
            failed[1].span.start(),
            occurrence(provider_source, "pick", 1)
        );
    }

    fn assert_target(
        source: &str,
        analysis: &crate::analysis::Analysis,
        reference_name: &str,
        reference_index: usize,
        target_name: &str,
        target_index: usize,
    ) {
        let reference = occurrence(source, reference_name, reference_index);
        let target = occurrence(source, target_name, target_index);
        let targets = analysis.definitions.targets_at(reference);
        assert_eq!(
            targets.len(),
            1,
            "targets for {reference_name}[{reference_index}]"
        );
        assert_eq!(targets[0].start(), target);
        assert!(
            analysis
                .definitions
                .targets_at(reference + reference_name.len())
                .is_empty()
        );
    }

    fn assert_no_target(
        source: &str,
        analysis: &crate::analysis::Analysis,
        name: &str,
        index: usize,
    ) {
        assert!(
            analysis
                .definitions
                .targets_at(occurrence(source, name, index))
                .is_empty()
        );
    }

    fn assert_unit_target(
        definitions: &UnitDefinitionIndex,
        source_unit: SourceUnitId,
        reference: usize,
        target_unit: SourceUnitId,
        target: usize,
    ) {
        let targets = definitions.targets_at(source_unit, reference);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].source_unit, target_unit);
        assert_eq!(targets[0].span.start(), target);
    }

    fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
        let source = sources.add_source(name, text).expect("source");
        let lexed = lex(sources, source).expect("lex");
        let parsed = parse_file(sources, &lexed).expect("parse");
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        (source, parsed)
    }

    fn source_unit(
        names: &lang_frontend::name_resolution::CompilationUnitNames,
        source: SourceId,
    ) -> SourceUnitId {
        names
            .index()
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == source)
            .expect("source unit")
            .id()
    }

    fn occurrence(source: &str, needle: &str, index: usize) -> usize {
        source
            .match_indices(needle)
            .nth(index)
            .map(|(offset, _)| offset)
            .unwrap_or_else(|| panic!("missing {needle}[{index}]"))
    }
}
