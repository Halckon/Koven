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
        missing_block_closers: Vec::new(),
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

#[test]
fn block_dispatch_legal_error_and_nested_families_stay_linear() {
    use super::tests::block_dispatch_metrics;

    for (make, diagnostics_per_element) in [
        (|count| format!("{{ {} }}", "val x = 1; ".repeat(count)), 0),
        (|count| format!("{{ {} }}", "return\n".repeat(count)), 0),
        (|count| format!("{{ {} }}", "@ ".repeat(count)), 1),
        (
            |count| format!("{{ {}{}", "block {".repeat(count - 1), "}".repeat(count)),
            0,
        ),
    ] as [(fn(usize) -> String, usize); 4]
    {
        let (small_raw, small_iterations, small_visits, small_diagnostics) =
            block_dispatch_metrics(make(32));
        let (large_raw, large_iterations, large_visits, large_diagnostics) =
            block_dispatch_metrics(make(64));
        assert_eq!(small_diagnostics, diagnostics_per_element * 32);
        assert_eq!(large_diagnostics, diagnostics_per_element * 64);
        assert!(small_iterations <= small_raw);
        assert!(large_iterations <= large_raw);
        assert!(
            small_visits <= small_raw * 32,
            "{small_visits} > {small_raw} * 32"
        );
        assert!(
            large_visits <= large_raw * 32,
            "{large_visits} > {large_raw} * 32"
        );
        assert!(large_visits <= small_visits * 2 + 64);
    }
}

#[test]
fn paired_poisoned_expression_groups_keep_linear_significant_visits() {
    let metrics = |count| {
        let groups = "(x async /* trivia */ (a, b)) + ".repeat(count);
        let text = format!("val value = {groups}tail\nval sentinel = 1");
        angle_separator_metrics(&text, 0).0
    };
    let small = metrics(32);
    let large = metrics(64);
    assert!(large <= small * 2 + 64, "{small} -> {large}");
}

#[test]
fn poison_before_else_keeps_linear_significant_visits() {
    let metrics = |count| {
        let poison = "β /* trivia */ ".repeat(count);
        let text = format!(
            "val value = if (outer) (if (inner) {{ value }} {poison}else a) else b\nval sentinel = 1"
        );
        let visits = angle_separator_metrics(&text, 0).0;
        assert!(visits >= count, "the metric must include the poison scan");
        visits
    };
    let small = metrics(32);
    let large = metrics(64);
    assert!(large <= small * 2 + 64, "{small} -> {large}");
}

#[test]
fn rejected_else_poison_runs_are_not_rescanned_from_each_statement() {
    let metrics = |count| {
        let text = format!(
            "{{ if (outer) when {{ true -> value\n{} }} else fallback }}",
            "β /* trivia */ \n".repeat(count)
        );
        super::tests::block_dispatch_metrics(text).2
    };
    let small = metrics(32);
    let large = metrics(64);
    assert!(large <= small * 2 + 64, "{small} -> {large}");
}

#[test]
fn paired_malformed_expression_groups_keep_linear_significant_visits() {
    let metrics = |count| {
        let groups = "f((x, /* trivia */ other), next) + ".repeat(count);
        let text = format!("val value = {groups}tail\nval sentinel = 1");
        angle_separator_metrics(&text, count).0
    };
    let small = metrics(32);
    let large = metrics(64);
    assert!(large <= small * 2 + 64, "{small} -> {large}");
}

#[test]
fn paired_index_recovery_keeps_linear_significant_visits() {
    let metrics = |count| {
        let groups = "xs[key, nested[item]] + ".repeat(count);
        let text = format!("val value = {groups}tail\nval sentinel = 1");
        angle_separator_metrics(&text, count).0
    };
    let small = metrics(32);
    let large = metrics(64);
    assert!(large <= small * 2 + 64, "{small} -> {large}");
}

#[test]
fn an_unclosed_interpolation_group_cannot_take_an_outer_closer() {
    use super::*;
    use crate::lexer::lex;

    let text = r#"("${(value β}tail")"#;
    let mut sources = SourceMap::new();
    let id = sources.add_source("group-scopes.ko", text).unwrap();
    let lexed = lex(&sources, id).unwrap();
    let recovery = LexicalRecoveryIndex::new(text, &lexed).unwrap();
    let raw_at = |offset| {
        lexed
            .lexemes()
            .iter()
            .position(|lexeme| lexeme.span().start() == offset)
            .unwrap()
    };
    let inner = raw_at(text.find("${(").unwrap() + 2);
    let outer_closer = raw_at(text.rfind(')').unwrap());
    assert_eq!(recovery.group_closer(&lexed, 0), Some(outer_closer));
    assert_eq!(recovery.group_closer(&lexed, inner), None);
}

#[test]
fn unpaired_indexes_do_not_rescan_a_rejected_poison_run() {
    let metrics = |count| {
        let text = format!(
            "{{ {}key {}+ tail }}",
            "xs[".repeat(count),
            "β ".repeat(count)
        );
        super::tests::block_dispatch_metrics(text).2
    };
    let small = metrics(32);
    let large = metrics(64);
    assert!(large <= small * 2 + 64, "{small} -> {large}");
}

fn new_test_parser<'a>(
    sources: &'a super::SourceMap,
    source_id: crate::source::SourceId,
    text: &'a str,
    lexed: &'a super::LexedFile,
) -> super::Parser<'a> {
    use super::*;

    let lexical_recoveries = LexicalRecoveryIndex::new(text, lexed).unwrap();
    let strict_trials = StrictCallTrialIndex::new(sources, lexed).unwrap();
    let lambda_headers =
        LambdaHeaderIndex::new(lexed, &lexical_recoveries.terminal_owner_events).unwrap();
    Parser {
        sources,
        lexed,
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
        missing_block_closers: Vec::new(),
        declaration_recovery_raw_visits: 0,
        declaration_recovery_event_queries_and_applications: 0,
        block_dispatch_iterations: 0,
        lambda_body_dispatch_iterations: 0,
        significant_raw_visits: Cell::new(0),
    }
}

#[test]
fn f2a_confirmed_closer_depends_on_enclosing_pairing_and_caller_stops() {
    use super::*;
    use crate::lexer::lex;

    let probe = |text: &str, opener_offset: usize, cursor_offset: usize, outer_stops: Stops| {
        let mut sources = SourceMap::new();
        let id = sources.add_source("f2a-probe.ko", text).unwrap();
        let lexed = lex(&sources, id).unwrap();
        let raw_at = |offset| {
            lexed
                .lexemes()
                .iter()
                .position(|lexeme| lexeme.span().start() == offset)
                .unwrap()
        };
        let opener_raw = raw_at(opener_offset);
        let cursor_raw = raw_at(cursor_offset);
        let mut parser = new_test_parser(&sources, id, text, &lexed);
        parser.index = cursor_raw;
        let skipped = parser
            .skip_to_confirmed_group_closer(opener_raw, outer_stops)
            .unwrap();
        (skipped.is_some(), parser.index, cursor_raw)
    };

    // Counterexample 1: confirmed local paired `(value, other)` in `f(..., next)`.
    let local_paired = "f((value, other), next)";
    let (jumped, after, before) = probe(
        local_paired,
        local_paired.find("(value").unwrap(),
        local_paired.find(',').unwrap(),
        Stops::ROOT.with(Stops::COMMA | Stops::RIGHT_PAREN),
    );
    assert!(jumped && after > before);

    // Counterexample 2: sole `)` in `f((value, next)` may belong to the caller.
    let ambiguous = "f((value, next)";
    let (jumped, after, before) = probe(
        ambiguous,
        ambiguous.find("(value").unwrap(),
        ambiguous.find(',').unwrap(),
        Stops::ROOT.with(Stops::COMMA | Stops::RIGHT_PAREN),
    );
    assert!(!jumped && after == before);

    // Counterexample 3: physically paired group with nested `[ else next ]` depends on caller `ELSE`.
    let nested_else = "if (flag) f((value junk [ else next ], rest), suffix)";
    let opener = nested_else.find("(value").unwrap();
    let cursor = nested_else.find("junk").unwrap();
    let (jumped_with_else, after_with_else, before_with_else) = probe(
        nested_else,
        opener,
        cursor,
        Stops::ROOT.with(Stops::ELSE | Stops::COMMA | Stops::RIGHT_PAREN),
    );
    assert!(!jumped_with_else && after_with_else == before_with_else);
    let (jumped_without_else, after_without_else, before_without_else) = probe(
        nested_else,
        opener,
        cursor,
        Stops::ROOT.with(Stops::COMMA | Stops::RIGHT_PAREN),
    );
    assert!(jumped_without_else && after_without_else > before_without_else);
}

#[test]
fn f2a_multi_group_unwind_and_long_poison_keep_linear_visits_and_events() {
    use super::*;
    use crate::lexer::lex;

    let file_metrics = |text: &str| {
        let mut sources = SourceMap::new();
        let id = sources.add_source("f2a-unwind.ko", text).unwrap();
        let lexed = lex(&sources, id).unwrap();
        let mut parser = new_test_parser(&sources, id, text, &lexed);
        let roots = parser.parse_file_roots().unwrap();
        let last = parser.ast.items().get(*roots.last().unwrap()).unwrap();
        assert_eq!(
            sources.slice(last.span()),
            Ok("val sentinel = 1"),
            "sentinel declaration must survive: {text}"
        );
        (
            parser.significant_raw_visits.get(),
            parser.declaration_recovery_raw_visits,
            parser.declaration_recovery_event_queries_and_applications,
        )
    };

    // Counterexample 4a: multi-group unwind with long lexical poison to a shared caller boundary.
    let poison_case = |depth, poison| {
        format!(
            "val broken = {}value {}\nval sentinel = 1",
            "(".repeat(depth),
            "β ".repeat(poison),
        )
    };
    let (small_sig, small_rec, small_ev) = file_metrics(&poison_case(16, 32));
    let (large_sig, large_rec, large_ev) = file_metrics(&poison_case(32, 64));
    assert!(
        large_sig <= small_sig * 2 + 64,
        "significant visits: {small_sig} -> {large_sig}"
    );
    assert!(
        large_rec <= small_rec * 2 + 64,
        "recovery raw visits: {small_rec} -> {large_rec}"
    );
    assert!(
        large_ev <= small_ev * 2 + 64,
        "event queries/apps: {small_ev} -> {large_ev}"
    );

    // Counterexample 4b: multi-group unwind inside a call to a shared caller comma.
    let call_case = |depth| {
        format!(
            "val broken = f({}value, next)\nval sentinel = 1",
            "(".repeat(depth),
        )
    };
    let (small_sig, small_rec, small_ev) = file_metrics(&call_case(16));
    let (large_sig, large_rec, large_ev) = file_metrics(&call_case(32));
    assert!(
        large_sig <= small_sig * 2 + 64,
        "call unwind significant visits: {small_sig} -> {large_sig}"
    );
    assert!(
        large_rec <= small_rec * 2 + 64,
        "call unwind recovery raw visits: {small_rec} -> {large_rec}"
    );
    assert!(
        large_ev <= small_ev * 2 + 64,
        "call unwind event queries/apps: {small_ev} -> {large_ev}"
    );
}
