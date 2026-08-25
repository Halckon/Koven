//! 既有名称/类型事实到 source-local definition target 的确定性索引。

use std::{collections::BTreeMap, error::Error, fmt};

use lang_frontend::{
    ast::{AstError, ExpressionId},
    name_resolution::{NameReference, NameResolution, ReferenceTarget, SymbolId},
    parser::{Expression, ParsedFile, SyntaxAst},
    source::{SourceId, Span},
    type_checking::{AggregateProjectionKind, CallableTarget, TypedFile},
};

pub(crate) struct DefinitionIndex {
    source_id: SourceId,
    entries: Vec<DefinitionEntry>,
}

struct DefinitionEntry {
    reference: Span,
    targets: Vec<Span>,
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

#[derive(Debug)]
pub(crate) enum DefinitionIndexError {
    Ast(AstError),
    InvalidSymbol(SymbolId),
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
    use crate::analysis::analyze;

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

    fn occurrence(source: &str, needle: &str, index: usize) -> usize {
        source
            .match_indices(needle)
            .nth(index)
            .map(|(offset, _)| offset)
            .unwrap_or_else(|| panic!("missing {needle}[{index}]"))
    }
}
