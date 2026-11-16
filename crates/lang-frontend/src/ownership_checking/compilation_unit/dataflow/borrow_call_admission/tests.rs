use super::*;
use crate::{lexer::lex, parser::parse_file, source::SourceMap};

fn parse(sources: &mut SourceMap, path: &str, text: &str) -> ParsedFile {
    let source = sources.add_source(path, text).unwrap();
    let lexed = lex(sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    parsed
}

fn oracle(parsed: &ParsedFile, call: ExpressionId, nothing: Option<ExpressionId>) -> bool {
    crate::ownership_checking::borrow_result::call_use_is_straight_line(parsed, call)
        && !parsed.ast().expressions().iter().any(|(id, node)| {
            let span = parsed.ast().expressions().get(call).unwrap().span();
            span.start() <= node.span().start()
                && node.span().end() <= span.end()
                && Some(id) == nothing
        })
}

#[test]
fn every_call_matches_original_predicate_and_nothing_overlay() {
    let mut sources = SourceMap::new();
    for (i, body) in [
        "count(view(source))",
        "count((view(source)))",
        "count(view(source)); count(view(source))",
        "if (true) { count(view(source)) } else { count(view(source)) }",
        "when (true) { true -> count(view(source)); else -> count(view(source)) }",
        "return count(view(source))",
        "count(view(source)) + 1",
        "count(-view(source))",
        "count(view(source)!!)",
        "while (false) { count(view(source)) }",
        "loop { count(view(source)); break }",
        "for (item in source) { count(view(item)) }",
        "val nested = { if (true) { count(view(source)) } else { 1 } }; count(view(source))",
        "val closure = { count(view(source)) }; count(view(source))",
        "return 1; count(view(source))",
    ]
    .iter()
    .enumerate()
    {
        let text = format!("fun run(source:String):Int {{ {body} }}\nfun other():Int=1");
        let parsed = parse(&mut sources, &format!("case{i}.ko"), &text);
        // Every possible Nothing placement includes outside-call and nested-call controls.
        for nothing in
            std::iter::once(None).chain(parsed.ast().expressions().iter().map(|(id, _)| Some(id)))
        {
            let index = AdmissionIndex::new(&parsed, |id| Some(id) == nothing);
            for (id, node) in parsed.ast().expressions().iter() {
                if matches!(node.payload(), Expression::Call { .. }) {
                    assert_eq!(
                        index.allows(&parsed, id),
                        oracle(&parsed, id, nothing),
                        "{text}, {id:?}, {nothing:?}"
                    );
                }
            }
        }
    }
}

fn visits_for_calls(n: usize) -> (usize, usize) {
    let mut sources = SourceMap::new();
    let text = format!(
        "fun run(source:String):Unit {{ {} }}",
        "count(view(source));".repeat(n)
    );
    let parsed = parse(&mut sources, "visits.ko", &text);
    let index = AdmissionIndex::new(&parsed, |_| false);
    let ast_nodes = parsed.ast().items().iter().count()
        + parsed.ast().expressions().iter().count()
        + parsed.ast().statements().iter().count();
    assert_eq!(index.visits.get(), ast_nodes, "one build traversal");
    let mut calls = 0;
    for (id, node) in parsed.ast().expressions().iter() {
        if matches!(node.payload(), Expression::Call { .. }) {
            assert!(index.allows(&parsed, id));
            assert!(oracle(&parsed, id, None));
            calls += 1;
        }
    }
    assert_eq!(calls, 2 * n);
    assert_eq!(
        index.visits.get(),
        ast_nodes + calls,
        "one candidate function per query, no full AST re-scan"
    );
    // Original positive path visits all items/expressions/statements plus Nothing scan per call.
    let old = calls * (ast_nodes + parsed.ast().expressions().iter().count());
    (index.visits.get(), old)
}

#[test]
fn repeated_straight_line_calls_visit_linear_candidates() {
    let (small, old_small) = visits_for_calls(32);
    let (large, old_large) = visits_for_calls(64);
    assert!(large <= 2 * small);
    assert!(old_large > 3 * old_small);
    assert!(large < old_large);
}
