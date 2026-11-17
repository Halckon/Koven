//! Immutable loop-span scan cache; not a control-flow or admission proof.
use crate::{
    parser::{ParsedFile, Statement},
    source::Span,
};
use std::cell::OnceCell;

#[derive(Default)]
pub(super) struct LoopSpans {
    spans: OnceCell<Vec<Span>>,
    #[cfg(test)]
    visits: std::cell::Cell<usize>,
}

impl LoopSpans {
    pub(super) fn contains(&self, parsed: &ParsedFile, marker: Span) -> bool {
        let spans = self.spans.get_or_init(|| {
            parsed
                .ast()
                .statements()
                .iter()
                .filter_map(|(_, node)| {
                    self.visit();
                    matches!(
                        node.payload(),
                        Statement::While { .. } | Statement::For { .. } | Statement::Loop { .. }
                    )
                    .then_some(node.span())
                })
                .collect()
        });
        spans.iter().any(|span| {
            self.visit();
            // Preserve the original offset-only predicate and arena iteration order.
            span.start() <= marker.start() && marker.end() <= span.end()
        })
    }

    #[inline]
    fn visit(&self) {
        #[cfg(test)]
        self.visits.set(self.visits.get() + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lexer::lex,
        parser::{Item, VariableKind, parse_file},
        source::SourceMap,
    };

    fn parse(text: &str) -> ParsedFile {
        let mut sources = SourceMap::new();
        let source = sources.add_source("loops.ko", text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        parsed
    }

    fn borrow_markers(parsed: &ParsedFile) -> Vec<Span> {
        parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Item::Variable {
                    kind: VariableKind::BorrowVal(marker),
                    ..
                } => Some(*marker),
                _ => None,
            })
            .collect()
    }

    fn oracle(parsed: &ParsedFile, marker: Span) -> bool {
        parsed.ast().statements().iter().any(|(_, node)| {
            matches!(
                node.payload(),
                Statement::While { .. } | Statement::For { .. } | Statement::Loop { .. }
            ) && node.span().start() <= marker.start()
                && marker.end() <= node.span().end()
        })
    }

    #[test]
    fn nested_adjacent_and_loop_free_markers_match_original_predicate() {
        for body in [
            "borrow val a = source; println(a)",
            "while (false) { borrow val a = source }; borrow val b = source",
            "for (item in source) { borrow val a = source }; loop { break }",
            "loop { while (false) { for (item in source) { borrow val a = source } }; break }; borrow val b = source",
        ] {
            let parsed = parse(&format!("fun run(source:String) {{ {body} }}"));
            let cache = LoopSpans::default();
            // Check all node spans, including loop boundaries and non-binding markers.
            for marker in parsed
                .ast()
                .statements()
                .iter()
                .map(|(_, n)| n.span())
                .chain(parsed.ast().expressions().iter().map(|(_, n)| n.span()))
                .chain(borrow_markers(&parsed))
            {
                assert_eq!(
                    cache.contains(&parsed, marker),
                    oracle(&parsed, marker),
                    "{body}: {marker:?}"
                );
            }
        }
    }

    #[test]
    fn loop_free_borrow_bindings_scan_statements_only_once() {
        for count in [8, 32, 128] {
            let body = (0..count)
                .map(|i| format!("{{ borrow val item{i} = source; println(item{i}) }};"))
                .collect::<String>();
            let parsed = parse(&format!("fun run(source:String) {{ {body} }}"));
            let cache = LoopSpans::default();
            assert!(cache.spans.get().is_none());
            let markers = borrow_markers(&parsed);
            assert_eq!(markers.len(), count);
            let statements = parsed.ast().statements().iter().count();
            for marker in markers {
                assert!(!cache.contains(&parsed, marker));
                assert!(!oracle(&parsed, marker));
            }
            assert_eq!(cache.visits.get(), statements);
            assert!(count * statements > cache.visits.get());
            assert!(cache.spans.get().unwrap().is_empty());
        }
    }
}
