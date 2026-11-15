//! Independent source coordinates and the existing layout-to-origin comparison.
use super::*;

pub(super) fn source_spans(source: &str, unit: bool) -> Spans {
    let text = if unit {
        format!("package p\n{source}")
    } else {
        source.to_owned()
    };
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(if unit { "p/range.ko" } else { "range.ko" }, &text)
        .unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty());
    let ast = file.ast();
    let (outer, outer_condition) = only(ast.statements().iter().filter_map(|(_, node)| {
        let Statement::While { condition, .. } = node.payload() else {
            return None;
        };
        let cond_span = ast.expressions().get(*condition).unwrap().span();
        Some((node.span(), cond_span))
    }));
    let (numeric, mut range) = only(ast.statements().iter().filter_map(|(_, node)| {
        let Statement::For { source, .. } = node.payload() else {
            return None;
        };
        Some((node.span(), *source))
    }));
    while let Expression::Group { expression } = ast.expressions().get(range).unwrap().payload() {
        range = *expression;
    }
    let range = ast.expressions().get(range).unwrap().span();
    let branch = only(ast.expressions().iter().filter_map(|(_, node)| {
        matches!(node.payload(), Expression::If { .. }).then_some(node.span())
    }));
    let continued = only(ast.expressions().iter().filter_map(|(_, node)| {
        matches!(node.payload(), Expression::Continue { .. }).then_some(node.span())
    }));
    let mut reads = ast
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            matches!(node.payload(), Expression::Member { .. }).then_some(node.span())
        })
        .collect::<Vec<_>>();
    reads.sort_by_key(|span| span.start());
    let [inner, final_read] = reads.as_slice() else {
        panic!("one inner and one post-loop Member")
    };
    Spans {
        outer,
        outer_condition,
        numeric,
        range,
        branch,
        continued,
        reads: [*inner, *final_read],
    }
}

pub(super) fn source_origin(origin: &Origin, expected: Span) -> bool {
    // The independent parse has its own SourceMap; it only supplies node coordinates.
    matches!(origin, Origin::Source(span) if span.start() == expected.start() && span.end() == expected.end())
}
