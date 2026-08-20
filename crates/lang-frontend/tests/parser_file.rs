//! SPEC-0014 完整文件组合与跨声明恢复契约测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    parser::{Item, ParsedFile, parse_declaration, parse_file},
    source::{SourceId, SourceMap},
};

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources.add_source(name, text).expect("unique source")
}

fn parsed(text: &str) -> (SourceMap, ParsedFile) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "file.ko", text);
    let lexed = lex(&sources, source_id).expect("lex");
    let parsed = parse_file(&sources, &lexed).expect("parse");
    (sources, parsed)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn root_items(parsed: &ParsedFile) -> Vec<&Item> {
    parsed
        .roots()
        .iter()
        .map(|id| parsed.ast().items().get(*id).expect("root").payload())
        .collect()
}

#[test]
fn empty_and_trivia_only_files_have_no_roots_or_diagnostics() {
    for text in ["", " \n// comment\n"] {
        let (_, parsed) = parsed(text);
        assert!(parsed.roots().is_empty());
        assert!(parsed.diagnostics().is_empty());
    }
}

#[test]
fn newline_semicolon_and_optional_trailing_semicolon_separate_declarations() {
    let text = "val a=1\nvar b=2; const val c=3\r\nfun f(){}\n; fun g():Int=4;";
    let (sources, parsed) = parsed(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.roots().len(), 5);
    let declarations: Vec<_> = parsed
        .roots()
        .iter()
        .map(|id| {
            sources
                .slice(parsed.ast().items().get(*id).unwrap().span())
                .unwrap()
        })
        .collect();
    assert_eq!(
        declarations,
        [
            "val a=1",
            "var b=2",
            "const val c=3",
            "fun f(){}",
            "fun g():Int=4",
        ]
    );
}

#[test]
fn same_line_declarations_without_semicolon_report_the_next_starter_and_recover() {
    let (_, parsed) = parsed("val answer = 42 fun next() {}");
    assert_eq!(parsed.roots().len(), 2);
    assert_eq!(codes(parsed.diagnostics()), ["L0047"]);
    let primary = parsed.diagnostics()[0].primary_span();
    assert_eq!((primary.start(), primary.end()), (16, 19));
    assert!(matches!(root_items(&parsed)[0], Item::Variable { .. }));
    assert!(matches!(root_items(&parsed)[1], Item::Function { .. }));
}

#[test]
fn only_a_real_line_break_or_semicolon_in_comment_trivia_separates_declarations() {
    let (_, same_line) = parsed("val a=1 /* comment */ fun f() {}");
    assert_eq!(codes(same_line.diagnostics()), ["L0047"]);

    for text in [
        "val a=1 /* comment\ncontinues */ fun f() {}",
        "val a=1 // comment\nfun f() {}",
    ] {
        let (_, separated) = parsed(text);
        assert!(separated.diagnostics().is_empty(), "{text:?}");
        assert_eq!(separated.roots().len(), 2);
    }
}

#[test]
fn leading_and_consecutive_semicolons_are_error_items_not_empty_declarations() {
    let (_, parsed) = parsed("; val a=1;; fun f() {};");
    assert_eq!(parsed.roots().len(), 4);
    assert_eq!(codes(parsed.diagnostics()), ["L0017", "L0017"]);
    assert!(matches!(root_items(&parsed)[0], Item::Error));
    assert!(matches!(root_items(&parsed)[1], Item::Variable { .. }));
    assert!(matches!(root_items(&parsed)[2], Item::Error));
    assert!(matches!(root_items(&parsed)[3], Item::Function { .. }));
}

#[test]
fn ordinary_unknown_region_is_one_error_item_and_preserves_next_declaration() {
    let (_, parsed) = parsed("@ junk val answer=42");
    assert_eq!(parsed.roots().len(), 2);
    assert!(matches!(root_items(&parsed)[0], Item::Error));
    assert!(matches!(root_items(&parsed)[1], Item::Variable { .. }));
    assert_eq!(codes(parsed.diagnostics()), ["L0017"]);
    let span = parsed.diagnostics()[0].primary_span();
    assert_eq!((span.start(), span.end()), (0, 6));
}

#[test]
fn lexer_poison_builds_error_item_without_duplicate_parser_classification() {
    let (_, parsed) = parsed("$ junk val answer=42");
    assert_eq!(parsed.roots().len(), 2);
    assert_eq!(codes(parsed.diagnostics()), ["L0001"]);
}

#[test]
fn unsupported_top_level_destructuring_recovers_at_next_declaration() {
    let (_, parsed) = parsed("val (a,b)=pair\nfun next(){}");
    assert_eq!(parsed.roots().len(), 2);
    assert!(matches!(root_items(&parsed)[0], Item::Error));
    assert!(matches!(root_items(&parsed)[1], Item::Function { .. }));
    assert_eq!(codes(parsed.diagnostics()), ["L0043"]);
}

#[test]
fn declaration_starter_inside_nested_delimiter_is_not_a_file_boundary() {
    let (_, parsed) = parsed("@ (fun hidden) val answer=42");
    assert_eq!(parsed.roots().len(), 2);
    assert_eq!(codes(parsed.diagnostics()), ["L0017"]);
    let first = parsed.ast().items().get(parsed.roots()[0]).unwrap();
    assert_eq!((first.span().start(), first.span().end()), (0, 14));
}

#[test]
fn nested_semicolon_and_declaration_starter_do_not_become_file_roots() {
    for text in [
        "val x=(1; fun hidden() {})\nval y=2",
        "val x=(1 fun hidden() {})\nval y=2",
        "val x=call(1; fun hidden() {})\nval y=2",
        "val x=\"${1; fun hidden() {}}\"\nval y=2",
    ] {
        let (_, parsed) = parsed(text);
        assert_eq!(
            parsed.roots().len(),
            2,
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        assert!(matches!(root_items(&parsed)[0], Item::Variable { .. }));
        assert!(matches!(root_items(&parsed)[1], Item::Variable { .. }));
    }
}

#[test]
fn missing_initializer_preserves_following_function_at_an_empty_boundary() {
    let (_, parsed) = parsed("val answer\nfun next(){}");
    assert_eq!(parsed.roots().len(), 2);
    assert_eq!(codes(parsed.diagnostics()), ["L0020"]);
    let span = parsed.diagnostics()[0].primary_span();
    assert_eq!((span.start(), span.end()), (11, 11));
}

#[test]
fn declaration_keyword_inside_call_owner_does_not_start_a_root() {
    let (_, parsed) = parsed("val x=call(fun hidden)\nval y=2");
    assert_eq!(
        parsed.roots().len(),
        2,
        "roots={:?} diagnostics={:?}",
        parsed.roots(),
        parsed.diagnostics()
    );
    assert_eq!(codes(parsed.diagnostics()), ["L0033"]);
}

#[test]
fn unmatched_call_owner_does_not_promote_inner_starters_to_file_roots() {
    let (_, parsed) = parsed("val x=call(fun hidden val y=2");
    assert_eq!(parsed.roots().len(), 1, "{:?}", parsed.diagnostics());
    assert!(codes(parsed.diagnostics()).contains(&"L0010".to_owned()));
}

#[test]
fn unmatched_parameter_owner_does_not_promote_inner_function_to_file_root() {
    let (_, parsed) = parsed("fun first(x:Int fun second(){}");
    assert_eq!(parsed.roots().len(), 1, "{:?}", parsed.diagnostics());
    assert!(codes(parsed.diagnostics()).contains(&"L0010".to_owned()));
}

#[test]
fn terminal_string_recovery_returns_to_file_baseline_before_recognizing_boundary() {
    let (_, parsed) = parsed("@ \"bad\n fun next(){}");
    assert_eq!(parsed.roots().len(), 2);
    assert_eq!(codes(parsed.diagnostics()), ["L0017", "L0004"]);
}

#[test]
fn standalone_declaration_keeps_its_trailing_token_contract() {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "standalone.ko", "val x=1 val y=2");
    let lexed = lex(&sources, source_id).expect("lex");
    let parsed = parse_declaration(&sources, &lexed).expect("parse");
    assert_eq!(codes(parsed.diagnostics()), ["L0013"]);
}

#[test]
fn standalone_declaration_does_not_accept_a_trailing_semicolon() {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "standalone-semicolon.ko", "val x=1;");
    let lexed = lex(&sources, source_id).expect("lex");
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_declaration(&sources, &lexed).expect("parse");
    assert_eq!(codes(parsed.diagnostics()), ["L0013"]);
}

#[test]
fn parsed_file_preserves_map_local_source_identity() {
    let mut sources = SourceMap::new();
    let first = add_source(&mut sources, "first.ko", "val a=1");
    let second = add_source(&mut sources, "second.ko", "val b=2");
    let lexed = lex(&sources, second).expect("lex");
    let parsed = parse_file(&sources, &lexed).expect("parse");
    assert_eq!(parsed.source_id(), second);
    assert_ne!(parsed.source_id(), first);
}

#[test]
fn long_file_keeps_every_root_without_recovery_drift() {
    for separator in ["\n", ";"] {
        let text: String = (0..512)
            .map(|index| format!("val n{index}={index}{separator}"))
            .collect();
        let (_, parsed) = parsed(&text);
        assert!(parsed.diagnostics().is_empty(), "separator={separator:?}");
        assert_eq!(parsed.roots().len(), 512);
    }
}
