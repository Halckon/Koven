//! Assign entered typed calls to their physical declaration scope without scanning all templates.
use super::*;

#[derive(Default)]
struct CallScopes(BTreeMap<SourceUnitId, Vec<Scope>>);

struct Scope {
    slot: usize,
    span: Span,
    max_end: usize,
}

impl CallScopes {
    fn new(scopes: impl Iterator<Item = (SourceUnitId, Span)>) -> Self {
        let mut result = Self::default();
        for (slot, (source, span)) in scopes.enumerate() {
            result.0.entry(source).or_default().push(Scope {
                slot,
                span,
                max_end: 0,
            });
        }
        for bucket in result.0.values_mut() {
            bucket.sort_unstable_by_key(|row| (row.span.start(), row.slot));
            let mut max_end = 0;
            for row in bucket {
                max_end = max_end.max(row.span.end());
                row.max_end = max_end;
            }
        }
        result
    }

    fn candidates(&self, source: SourceUnitId, call: Span) -> &[Scope] {
        let Some(scopes) = self.0.get(&source) else {
            return &[];
        };
        let end = scopes.partition_point(|row| row.span.start() <= call.start());
        let start = scopes[..end].partition_point(|row| row.max_end < call.end());
        &scopes[start..end]
    }

    fn owner(&self, source: SourceUnitId, call: Span) -> Result<Option<usize>, LoweringError> {
        let mut matching = self
            .candidates(source, call)
            .iter()
            .filter(|row| span_contains(row.span, call));
        let owner = matching.next().map(|row| row.slot);
        if matching.next().is_some() {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, call));
        }
        Ok(owner)
    }
}

pub(super) fn index_calls(
    typed: &CompilationUnitTypes,
    owned: &dyn UnitLoweringFacts,
    parsed_by_source: &[&ParsedFile],
    templates: &[UnitFunctionTemplate],
) -> Result<Vec<Vec<(usize, Span)>>, LoweringError> {
    let mut calls = vec![Vec::new(); templates.len()];
    let scopes = CallScopes::new(
        templates
            .iter()
            .map(|template| (template.source_unit, template.span)),
    );
    for (call_index, call) in typed.calls().iter().enumerate() {
        let expression = call.expression();
        let span = parsed_by_source
            .get(expression.source_unit().index())
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        if !owned.call_site_was_entered(expression) {
            continue;
        }
        if let Some(owner) = scopes.owner(expression.source_unit(), span)? {
            calls[owner].push((call_index, span));
        }
    }
    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_frontend::{
        lexer::lex,
        name_resolution::{SourceUnitInput, index_compilation_unit},
        parser::parse_file,
        source::{SourceId, SourceMap},
    };

    fn source_ids<const N: usize>(
        sources: &SourceMap,
        files: [(&str, SourceId); N],
    ) -> [SourceUnitId; N] {
        let parsed =
            files.map(|(_, source)| parse_file(sources, &lex(sources, source).unwrap()).unwrap());
        let inputs = std::array::from_fn::<_, N, _>(|i| {
            let (name, source) = files[i];
            SourceUnitInput::new("root", name, source, &parsed[i])
        });
        let index = index_compilation_unit(sources, &inputs).unwrap();
        files.map(|(_, source)| {
            index
                .source_units()
                .iter()
                .find(|unit| unit.source_id() == source)
                .unwrap()
                .id()
        })
    }

    #[test]
    fn disjoint_callable_scopes_select_one_candidate_per_entered_call() {
        for scale in [8, 16] {
            let mut sources = SourceMap::new();
            let id = sources.add_source("a.ko", " ".repeat(8 * scale)).unwrap();
            let other = sources.add_source("b.ko", " ".repeat(8 * scale)).unwrap();
            let [a, b] = source_ids(&sources, [("a.ko", id), ("b.ko", other)]);
            let rows = (0..scale)
                .flat_map(|i| {
                    [
                        (a, sources.span(id, 8 * i, 8 * i + 6).unwrap()),
                        (b, sources.span(other, 8 * i, 8 * i + 6).unwrap()),
                    ]
                })
                .collect::<Vec<_>>();
            // Reverse declaration order; indices still point into the original template table.
            let index = CallScopes::new(rows.into_iter().rev());
            for i in 0..scale {
                for (source, physical, offset) in [(a, id, 1), (b, other, 0)] {
                    let call = sources.span(physical, 8 * i + 2, 8 * i + 4).unwrap();
                    assert_eq!(index.candidates(source, call).len(), 1);
                    assert_eq!(
                        index.owner(source, call).unwrap(),
                        Some(2 * (scale - 1 - i) + offset)
                    );
                }
            }
        }
    }

    #[test]
    fn overlapping_scopes_fail_only_for_calls_covered_twice_by_the_same_physical_source() {
        let mut sources = SourceMap::new();
        let id = sources.add_source("a.ko", " ".repeat(16)).unwrap();
        let mut other = SourceMap::new();
        let foreign = other.add_source("a.ko", " ".repeat(16)).unwrap();
        let [source] = source_ids(&sources, [("a.ko", id)]);
        let at = |a, b| sources.span(id, a, b).unwrap();
        let index = CallScopes::new(
            [
                (source, other.span(foreign, 1, 11).unwrap()),
                (source, at(1, 11)),
                (source, at(3, 5)),
            ]
            .into_iter(),
        );
        assert_eq!(index.owner(source, at(8, 9)).unwrap(), Some(1));
        assert_eq!(index.owner(source, at(15, 16)).unwrap(), None);
        let overlap = at(3, 5);
        let error = index.owner(source, overlap).unwrap_err();
        assert_eq!(
            (error.kind, error.span),
            (LoweringErrorKind::InvalidModel, Some(overlap))
        );
    }
}
