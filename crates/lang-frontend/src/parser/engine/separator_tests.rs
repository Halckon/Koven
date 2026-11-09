//! Count parser work for valid and malformed adjacent-element separators.

#[test]
fn missing_separators_and_long_trivia_keep_dispatch_linear() {
    for gap in [" ".to_owned(), format!("/*{}*/", " ".repeat(1024))] {
        let make = |count| format!("{{ {} }}", format!("x{gap}").repeat(count));
        let (small_raw, small_iterations, small_visits, small_errors) =
            super::tests::block_dispatch_metrics(make(32));
        let (large_raw, large_iterations, large_visits, large_errors) =
            super::tests::block_dispatch_metrics(make(64));
        assert_eq!((small_errors, large_errors), (31, 63));
        assert!(small_iterations <= small_raw && large_iterations <= large_raw);
        assert!(large_visits <= small_visits * 2 + 64);
    }
}

fn angle_separator_metrics(text: &str, diagnostic_count: usize) -> (usize, usize) {
    use super::*;
    use crate::lexer::lex;

    let mut sources = SourceMap::new();
    let source_id = sources.add_source("angle-separator.ko", text).unwrap();
    let lexed = lex(&sources, source_id).unwrap();
    let lexical_recoveries = LexicalRecoveryIndex::new(text, &lexed).unwrap();
    let strict_trials = StrictCallTrialIndex::new(&sources, &lexed).unwrap();
    let lambda_headers =
        LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events).unwrap();
    let mut parser = Parser {
        sources: &sources,
        lexed: &lexed,
        lexical_recoveries,
        file_angle: None,
        strict_trials,
        lambda_headers,
        index: 0,
        next_terminal_recovery_event: 0,
        recursion_depth: 0,
        file_mode: true,
        ast: ExpressionAst::new(source_id),
        diagnostics: Vec::new(),
        declaration_recovery_raw_visits: 0,
        declaration_recovery_event_queries_and_applications: 0,
        block_dispatch_iterations: 0,
        lambda_body_dispatch_iterations: 0,
        significant_raw_visits: Cell::new(0),
    };
    let roots = parser.parse_file_roots().unwrap();
    assert_eq!(roots.len(), 2);
    assert_eq!(parser.diagnostics.len(), diagnostic_count);
    (
        parser.significant_raw_visits.get(),
        parser.declaration_recovery_raw_visits,
    )
}

#[test]
fn unfinished_nested_types_do_not_rescan_the_file_separator_suffix() {
    let metrics = |separator, depth, trivia| {
        let text = format!(
            "interface C<T: {}Int{separator}{} public val sentinel = 2 > 1",
            "A<".repeat(depth),
            "/* comment */ ".repeat(trivia)
        );
        angle_separator_metrics(&text, depth + 1).0
    };
    for separator in [";", "\n"] {
        let small = metrics(separator, 16, 64);
        let large = metrics(separator, 32, 128);
        assert!(
            large <= small * 2 + 64,
            "{separator:?} raw token visits: {small} -> {large}"
        );
    }
}

#[test]
fn recovery_scans_paired_angle_groups_once() {
    let metrics = |count| {
        let text = format!(
            "interface C<T break {}>\nclass Sentinel",
            "f([2 > 1) ".repeat(count)
        );
        angle_separator_metrics(&text, 1)
    };
    let (small_significant, small_recovery) = metrics(32);
    let (large_significant, large_recovery) = metrics(64);
    assert!(small_recovery > 32);
    assert!(large_significant <= small_significant * 2 + 64);
    assert!(large_recovery <= small_recovery * 2 + 16);
}
