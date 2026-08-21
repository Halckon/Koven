use super::*;
use crate::{lexer::lex, source::SourceMap};

#[test]
fn lexical_recovery_indexes_only_strings_that_own_invalid_escapes() {
    let text = r#""valid" "bad\q" "${"nested\z"}""#;
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("invalid-string-escape-owners.ko", text)
        .expect("test source name must be unique");
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let index = LexicalRecoveryIndex::new(text, &lexed).expect("recoveries must index");
    let openers = lexed
        .lexemes()
        .iter()
        .filter(|lexeme| matches!(lexeme.kind(), LexemeKind::Token(TokenKind::StringStart)))
        .map(|lexeme| lexeme.span().start())
        .collect::<Vec<_>>();
    let closers = lexed
        .lexemes()
        .iter()
        .filter(|lexeme| matches!(lexeme.kind(), LexemeKind::Token(TokenKind::StringEnd)))
        .map(|lexeme| lexeme.span().end())
        .collect::<Vec<_>>();

    assert_eq!(openers.len(), 4);
    assert_eq!(closers.len(), 4);
    assert_eq!(index.string_owner_end(openers[0]), Some(closers[0]));
    assert_eq!(index.string_owner_end(openers[1]), Some(closers[1]));
    assert_eq!(index.string_owner_end(openers[2]), Some(closers[3]));
    assert_eq!(index.string_owner_end(openers[3]), Some(closers[2]));
    assert_eq!(index.lexical_poison_string_recovery_end(openers[0]), None);
    assert_eq!(
        index.lexical_poison_string_recovery_end(openers[1]),
        Some(closers[1])
    );
    assert_eq!(
        index.lexical_poison_string_recovery_end(openers[2]),
        Some(closers[3])
    );
    assert_eq!(
        index.lexical_poison_string_recovery_end(openers[3]),
        Some(closers[2])
    );
}

#[test]
fn function_suffix_transfers_complete_string_poison_to_the_lexer_owner() {
    for (text, expected) in [
        (r#"fun f() "a\q""#, vec!["L0006"]),
        (r#"fun f() "${"bad\q"}""#, vec!["L0006"]),
        ("fun f() \"${\"inner\n}tail\"", vec!["L0004"]),
        (r#"fun f(): "a\q""#, vec!["L0006"]),
        (r#"fun f(): "${"bad\q"}""#, vec!["L0006"]),
        ("fun f(): \"${\"inner\n}tail\"", vec!["L0004"]),
        (r#"fun f(): "ok""#, vec!["L0014"]),
    ] {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("function-string-suffix.ko", text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let parsed = parse_declaration(&sources, &lexed)
            .expect("lexical poison must remain a user diagnostic");
        let actual = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{text:?}: {:?}", parsed.diagnostics());

        let Item::Function { form, .. } = parsed
            .ast()
            .items()
            .get(parsed.root())
            .expect("function root")
            .payload()
        else {
            panic!("{text:?}: function root")
        };
        if text.starts_with("fun f():") {
            let FunctionForm::Explicit { type_ref, .. } = form else {
                panic!("{text:?}: explicit form")
            };
            assert!(matches!(
                parsed
                    .ast()
                    .type_refs()
                    .get(*type_ref)
                    .expect("return type")
                    .payload(),
                TypeRef::Error
            ));
        } else {
            assert!(matches!(form, FunctionForm::ImplicitUnitAbsent));
        }
    }
}

fn recovery_metrics(regions: usize) -> (usize, usize, usize) {
    let region = " /* dense */ ( [ \"outer ${ [ \"bad\n next ] } tail\" ] ) // trivia\n ";
    let text = format!(
        "fun f(p: T = [ {} \"terminal ${{ value",
        region.repeat(regions)
    );

    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("declaration-recovery.ko", &text)
        .expect("test source name must be unique");
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let source = sources
        .source_text(source_id)
        .expect("test source must remain available");
    validate_lexemes(&sources, &lexed, source.len()).expect("lexer output must be valid");

    let lexical_recoveries =
        LexicalRecoveryIndex::new(source, &lexed).expect("recoveries must index");
    let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
    let lambda_headers = LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
        .expect("headers must index");
    let mut parser = Parser {
        sources: &sources,
        lexed: &lexed,
        lexical_recoveries,
        strict_trials,
        lambda_headers,
        index: 0,
        next_terminal_recovery_event: 0,
        recursion_depth: 0,
        file_mode: false,
        ast: ExpressionAst::new(source_id),
        diagnostics: Vec::new(),
        declaration_recovery_raw_visits: 0,
        declaration_recovery_event_queries_and_applications: 0,
        block_dispatch_iterations: 0,
        lambda_body_dispatch_iterations: 0,
        significant_raw_visits: Cell::new(0),
    };
    parser
        .parse_declaration_root()
        .expect("recovered declaration must parse");
    let unsupported_default = codes::catalog()
        .expect("diagnostic catalog must be valid")
        .resolve(codes::UNSUPPORTED_PARAMETER_DEFAULT)
        .expect("unsupported-default code must exist");
    assert_eq!(parser.diagnostics.len(), 1);
    assert_eq!(parser.diagnostics[0].code(), unsupported_default);

    (
        lexed.lexemes().len(),
        parser.declaration_recovery_raw_visits,
        parser.declaration_recovery_event_queries_and_applications,
    )
}

#[test]
fn declaration_recovery_visits_raw_lexemes_and_terminal_events_linearly() {
    let (small_raw, small_visits, small_events) = recovery_metrics(16);
    let (large_raw, large_visits, large_events) = recovery_metrics(32);
    let small_work = small_visits + small_events;
    let large_work = large_visits + large_events;

    assert!(small_visits > 0);
    assert!(small_visits <= small_raw);
    assert!(large_visits <= large_raw);
    assert!(small_work <= small_raw * 3);
    assert!(large_work <= large_raw * 3);
    assert!(large_work <= small_work * 2 + 8);
}

fn parse_class_family_metrics(text: &str) -> (usize, usize, usize) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("class-family-linear.ko", text)
        .expect("test source name must be unique");
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let source = sources
        .source_text(source_id)
        .expect("test source must remain available");
    validate_lexemes(&sources, &lexed, source.len()).expect("lexer output must be valid");
    let lexical_recoveries =
        LexicalRecoveryIndex::new(source, &lexed).expect("recoveries must index");
    let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
    let lambda_headers = LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
        .expect("headers must index");
    let mut parser = Parser {
        sources: &sources,
        lexed: &lexed,
        lexical_recoveries,
        strict_trials,
        lambda_headers,
        index: 0,
        next_terminal_recovery_event: 0,
        recursion_depth: 0,
        file_mode: false,
        ast: ExpressionAst::new(source_id),
        diagnostics: Vec::new(),
        declaration_recovery_raw_visits: 0,
        declaration_recovery_event_queries_and_applications: 0,
        block_dispatch_iterations: 0,
        lambda_body_dispatch_iterations: 0,
        significant_raw_visits: Cell::new(0),
    };
    parser
        .parse_declaration_root()
        .expect("class family must parse");
    assert!(parser.diagnostics.is_empty(), "{:?}", parser.diagnostics);
    (
        lexed.lexemes().len(),
        parser.significant_raw_visits.get(),
        parser.ast.items().len(),
    )
}

fn class_family_metrics(members: usize) -> (usize, usize, usize) {
    let text = format!("class C {{ {} }}", "fun f(): Unit; ".repeat(members));
    parse_class_family_metrics(&text)
}

#[test]
fn class_family_member_dispatch_stays_linear_when_doubled() {
    let (small_raw, small_visits, small_items) = class_family_metrics(32);
    let (large_raw, large_visits, large_items) = class_family_metrics(64);
    assert_eq!(small_items, 33);
    assert_eq!(large_items, 65);
    assert!(small_visits <= small_raw * 24);
    assert!(large_visits <= large_raw * 24);
    assert!(large_visits <= small_visits * 2 + 64);
}

fn class_family_inline_sequence_metrics(variants: bool, elements: usize) -> (usize, usize) {
    let entries = (0..elements)
        .map(|index| {
            if variants {
                format!("V{index}")
            } else {
                format!("val f{index}: Int")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let text = if variants {
        format!("enum class E {{ {entries} }}")
    } else {
        format!("class C({entries})")
    };
    let (raw, visits, _) = parse_class_family_metrics(&text);
    (raw, visits)
}

#[test]
fn class_family_field_and_variant_sequences_stay_linear_when_doubled() {
    for variants in [false, true] {
        let (small_raw, small_visits) = class_family_inline_sequence_metrics(variants, 32);
        let (large_raw, large_visits) = class_family_inline_sequence_metrics(variants, 64);
        assert!(small_visits <= small_raw * 24);
        assert!(large_visits <= large_raw * 24);
        assert!(large_visits <= small_visits * 2 + 64);
    }
}

#[test]
fn interface_delegation_sequences_stay_linear_when_doubled() {
    let metrics = |entries| {
        let supertypes = (0..entries)
            .map(|index| format!("I{index} by delegate"))
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!("class C(val delegate: Impl): {supertypes}");
        let (raw, visits, _) = parse_class_family_metrics(&text);
        (raw, visits)
    };
    let (small_raw, small_visits) = metrics(32);
    let (large_raw, large_visits) = metrics(64);
    assert!(small_visits <= small_raw * 24);
    assert!(large_visits <= large_raw * 24);
    assert!(large_visits <= small_visits * 2 + 64);
}

fn call_recovery_metrics(regions: usize) -> (usize, usize, usize, usize, usize) {
    let text = format!("f({}tail)", "@ \"bad\n, ".repeat(regions));
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("call-recovery.ko", &text)
        .expect("test source name must be unique");
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let source = sources
        .source_text(source_id)
        .expect("test source must remain available");
    validate_lexemes(&sources, &lexed, source.len()).expect("lexer output must be valid");

    let lexical_recoveries =
        LexicalRecoveryIndex::new(source, &lexed).expect("recoveries must index");
    let terminal_events = lexical_recoveries.terminal_owner_events.len();
    let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
    let lambda_headers = LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
        .expect("headers must index");
    let mut parser = Parser {
        sources: &sources,
        lexed: &lexed,
        lexical_recoveries,
        strict_trials,
        lambda_headers,
        index: 0,
        next_terminal_recovery_event: 0,
        recursion_depth: 0,
        file_mode: false,
        ast: ExpressionAst::new(source_id),
        diagnostics: Vec::new(),
        declaration_recovery_raw_visits: 0,
        declaration_recovery_event_queries_and_applications: 0,
        block_dispatch_iterations: 0,
        lambda_body_dispatch_iterations: 0,
        significant_raw_visits: Cell::new(0),
    };
    let root = parser
        .parse_expression_bp(0, Stops::ROOT)
        .expect("recovered call must parse");
    parser
        .consume_expression_tail(root, Stops::ROOT)
        .expect("recovered call tail must parse");

    assert_eq!(terminal_events, regions);
    assert_eq!(parser.next_terminal_recovery_event, terminal_events);
    assert_eq!(parser.diagnostics.len(), regions);
    assert!(
        parser
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code().to_string() == "L0033")
    );

    (
        lexed.lexemes().len(),
        parser.declaration_recovery_raw_visits,
        parser.declaration_recovery_event_queries_and_applications,
        terminal_events,
        parser.diagnostics.len(),
    )
}

#[test]
fn call_recovery_with_many_terminal_owners_stays_linear() {
    let (small_raw, small_visits, small_event_work, small_events, small_diagnostics) =
        call_recovery_metrics(16);
    let (large_raw, large_visits, large_event_work, large_events, large_diagnostics) =
        call_recovery_metrics(32);
    let small_work = small_visits + small_event_work;
    let large_work = large_visits + large_event_work;

    assert_eq!(large_events, small_events * 2);
    assert_eq!(large_diagnostics, small_diagnostics * 2);
    assert!(small_visits <= small_raw);
    assert!(large_visits <= large_raw);
    assert!(small_work <= small_raw * 3);
    assert!(large_work <= large_raw * 3);
    assert!(large_work <= small_work * 2 + 8);
}

#[test]
fn lambda_local_recovery_preserves_inherited_hard_closers() {
    for text in [
        "f({ val + )",
        "f({ val x: A<B )",
        "f({ { val + )",
        "a[{ val + ]",
    ] {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("lambda-local-recovery.ko", text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let parsed = parse(&sources, &lexed).expect("recovery must remain a user diagnostic");
        assert!(
            matches!(
                parsed
                    .ast()
                    .expressions()
                    .get(parsed.root())
                    .expect("root expression")
                    .payload(),
                Expression::Call { .. } | Expression::Index { .. }
            ),
            "{text:?} must leave the inherited closer for its caller"
        );
        assert!(
            parsed.diagnostics().iter().all(|diagnostic| {
                !matches!(diagnostic.code().to_string().as_str(), "L0013" | "L0029")
            }),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }
}

fn postfix_propagation_metrics(questions: usize) -> (usize, usize) {
    let text = format!("result{}", "?".repeat(questions));
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("postfix-propagation.ko", &text)
        .expect("test source name must be unique");
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let lexical_recoveries =
        LexicalRecoveryIndex::new(&text, &lexed).expect("recoveries must index");
    let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
    let lambda_headers = LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
        .expect("headers must index");
    let mut parser = Parser {
        sources: &sources,
        lexed: &lexed,
        lexical_recoveries,
        strict_trials,
        lambda_headers,
        index: 0,
        next_terminal_recovery_event: 0,
        recursion_depth: 0,
        file_mode: false,
        ast: ExpressionAst::new(source_id),
        diagnostics: Vec::new(),
        declaration_recovery_raw_visits: 0,
        declaration_recovery_event_queries_and_applications: 0,
        block_dispatch_iterations: 0,
        lambda_body_dispatch_iterations: 0,
        significant_raw_visits: Cell::new(0),
    };
    let root = parser
        .parse_expression_bp(0, Stops::ROOT)
        .expect("propagation chain must parse");
    parser
        .consume_expression_tail(root, Stops::ROOT)
        .expect("propagation tail must parse");
    assert!(parser.diagnostics.is_empty());
    (lexed.lexemes().len(), parser.significant_raw_visits.get())
}

#[test]
fn postfix_propagation_significant_visits_stay_linear_when_doubled() {
    let (small_raw, small_visits) = postfix_propagation_metrics(256);
    let (large_raw, large_visits) = postfix_propagation_metrics(512);
    assert!(small_visits <= small_raw * 16);
    assert!(large_visits <= large_raw * 16);
    assert!(large_visits <= small_visits * 2 + 32);
}

fn block_dispatch_metrics(text: String) -> (usize, usize, usize, usize) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("block-dispatch.ko", &text)
        .expect("test source name must be unique");
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let lexical_recoveries =
        LexicalRecoveryIndex::new(&text, &lexed).expect("recoveries must index");
    let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
    let lambda_headers = LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
        .expect("headers must index");
    let mut parser = Parser {
        sources: &sources,
        lexed: &lexed,
        lexical_recoveries,
        strict_trials,
        lambda_headers,
        index: 0,
        next_terminal_recovery_event: 0,
        recursion_depth: 0,
        file_mode: false,
        ast: ExpressionAst::new(source_id),
        diagnostics: Vec::new(),
        declaration_recovery_raw_visits: 0,
        declaration_recovery_event_queries_and_applications: 0,
        block_dispatch_iterations: 0,
        lambda_body_dispatch_iterations: 0,
        significant_raw_visits: Cell::new(0),
    };
    parser.parse_block_root().expect("block must parse");
    (
        lexed.lexemes().len(),
        parser.block_dispatch_iterations,
        parser.significant_raw_visits.get(),
        parser.diagnostics.len(),
    )
}

#[test]
fn block_dispatch_legal_error_and_nested_families_stay_linear() {
    for (make, diagnostics_per_element) in [
        (|count| format!("{{ {} }}", "val x = 1 ".repeat(count)), 0),
        (|count| format!("{{ {} }}", "return ".repeat(count)), 0),
        (|count| format!("{{ {} }}", "@ ".repeat(count)), 1),
        (
            |count| format!("{}{}", "{".repeat(count), "}".repeat(count)),
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

fn lambda_body_dispatch_metrics(text: String) -> (usize, usize, usize, usize) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("lambda-body-dispatch.ko", &text)
        .expect("test source name must be unique");
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let lexical_diagnostics = lexed.diagnostics().len();
    let lexical_recoveries =
        LexicalRecoveryIndex::new(&text, &lexed).expect("recoveries must index");
    let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
    let lambda_headers = LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
        .expect("headers must index");
    let mut parser = Parser {
        sources: &sources,
        lexed: &lexed,
        lexical_recoveries,
        strict_trials,
        lambda_headers,
        index: 0,
        next_terminal_recovery_event: 0,
        recursion_depth: 0,
        file_mode: false,
        ast: ExpressionAst::new(source_id),
        diagnostics: Vec::new(),
        declaration_recovery_raw_visits: 0,
        declaration_recovery_event_queries_and_applications: 0,
        block_dispatch_iterations: 0,
        lambda_body_dispatch_iterations: 0,
        significant_raw_visits: Cell::new(0),
    };
    let root = parser
        .parse_expression_bp(0, Stops::ROOT)
        .expect("lambda must parse");
    parser
        .consume_expression_tail(root, Stops::ROOT)
        .expect("lambda tail must parse");

    (
        lexed.lexemes().len(),
        parser.lambda_body_dispatch_iterations,
        parser.significant_raw_visits.get(),
        lexical_diagnostics + parser.diagnostics.len(),
    )
}

#[test]
fn lambda_body_legal_unsupported_and_poison_families_stay_linear() {
    for (make, diagnostics_per_element) in [
        (|count| format!("{{ {} }}", "{} ".repeat(count)), 0),
        (|count| format!("{{ {} }}", "return ".repeat(count)), 0),
        (|count| format!("{{ {} }}", "@ ".repeat(count)), 1),
    ] as [(fn(usize) -> String, usize); 3]
    {
        let (small_raw, small_iterations, small_visits, small_diagnostics) =
            lambda_body_dispatch_metrics(make(32));
        let (large_raw, large_iterations, large_visits, large_diagnostics) =
            lambda_body_dispatch_metrics(make(64));

        assert_eq!(small_iterations, 32);
        assert_eq!(large_iterations, 64);
        assert_eq!(small_diagnostics, diagnostics_per_element * 32);
        assert_eq!(large_diagnostics, diagnostics_per_element * 64);
        assert!(
            small_visits <= small_raw * 34,
            "{small_visits} > {small_raw} * 34"
        );
        assert!(
            large_visits <= large_raw * 34,
            "{large_visits} > {large_raw} * 34"
        );
        assert!(large_visits <= small_visits * 2 + 64);
    }
}

#[test]
fn terminal_owner_events_preserve_boundary_order_and_exact_owner() {
    let text = concat!(
        "fun f(p: T = [ \"outer ${ \"bad\n next } tail\", ",
        "\"outer ${ \"bad\n next } tail\", ",
        "\"terminal ${ value"
    );
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("terminal-owner-events.ko", text)
        .expect("test source name must be unique");
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let index = LexicalRecoveryIndex::new(text, &lexed).expect("events must index");
    let bad_openers = text
        .match_indices("\"bad")
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let terminal_interpolation = text.rfind("${").expect("terminal interpolation must exist");

    assert_eq!(index.terminal_owner_events.len(), 4);
    for (event, opener) in index.terminal_owner_events[..2].iter().zip(bad_openers) {
        assert_eq!(event.kind, TerminalOwnerKind::String);
        assert_eq!(event.opener, opener);
        assert_eq!(event.offset, text[opener..].find('\n').unwrap() + opener);
    }
    assert_eq!(
        index.terminal_owner_events[2],
        TerminalOwnerEvent {
            offset: text.len(),
            kind: TerminalOwnerKind::Interpolation,
            opener: terminal_interpolation,
        }
    );
    assert_eq!(
        index.terminal_owner_events[3],
        TerminalOwnerEvent {
            offset: text.len(),
            kind: TerminalOwnerKind::String,
            opener: text[..terminal_interpolation].rfind('"').unwrap(),
        }
    );

    let nested_eof_text = "fun f(x:T = \"${\"inner";
    let mut nested_sources = SourceMap::new();
    let nested_id = nested_sources
        .add_source("nested-terminal-owners.ko", nested_eof_text)
        .expect("test source name must be unique");
    let nested_lexed = lex(&nested_sources, nested_id).expect("test source must lex");
    let nested_index = LexicalRecoveryIndex::new(nested_eof_text, &nested_lexed)
        .expect("suppressed outer owners must index");
    let outer_string = nested_eof_text.find('"').unwrap();
    let interpolation = nested_eof_text.find("${").unwrap();
    let inner_string = nested_eof_text.rfind('"').unwrap();
    assert_eq!(
        nested_index.terminal_owner_events,
        [
            TerminalOwnerEvent {
                offset: nested_eof_text.len(),
                kind: TerminalOwnerKind::String,
                opener: inner_string,
            },
            TerminalOwnerEvent {
                offset: nested_eof_text.len(),
                kind: TerminalOwnerKind::Interpolation,
                opener: interpolation,
            },
            TerminalOwnerEvent {
                offset: nested_eof_text.len(),
                kind: TerminalOwnerKind::String,
                opener: outer_string,
            },
        ]
    );

    let escape_text = "fun f(p: T = [ \"terminal\\";
    let mut escape_sources = SourceMap::new();
    let escape_id = escape_sources
        .add_source("terminal-escape-owner.ko", escape_text)
        .expect("test source name must be unique");
    let escape_lexed = lex(&escape_sources, escape_id).expect("test source must lex");
    let escape_index =
        LexicalRecoveryIndex::new(escape_text, &escape_lexed).expect("terminal escape must index");
    assert_eq!(
        escape_index.terminal_owner_events,
        [TerminalOwnerEvent {
            offset: escape_text.len(),
            kind: TerminalOwnerKind::String,
            opener: escape_text.find('"').unwrap(),
        }]
    );
}
