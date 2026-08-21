//! SPEC-0014 / SPEC-0128 完整文件组合与跨声明恢复契约测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    parser::{Item, ParsedFile},
    source::{SourceId, SourceMap},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    lex_and_parse_declaration_twice, parse_declaration_twice, parse_file_twice,
};

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources.add_source(name, text).expect("unique source")
}

fn parsed(text: &str) -> (SourceMap, ParsedFile) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "file.ko", text);
    let parsed = parse_file_twice(&sources, source_id, text);
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
    let text = "val x=1 val y=2";
    let source_id = add_source(&mut sources, "standalone.ko", text);
    let parsed = parse_declaration_twice(&sources, source_id, text);
    assert_eq!(codes(parsed.diagnostics()), ["L0013"]);
}

#[test]
fn standalone_declaration_does_not_accept_a_trailing_semicolon() {
    let mut sources = SourceMap::new();
    let text = "val x=1;";
    let source_id = add_source(&mut sources, "standalone-semicolon.ko", text);
    let (lexed, parsed) = lex_and_parse_declaration_twice(&sources, source_id, text);
    assert!(lexed.diagnostics().is_empty());
    assert_eq!(codes(parsed.diagnostics()), ["L0013"]);
}

#[test]
fn parsed_file_preserves_map_local_source_identity() {
    let mut sources = SourceMap::new();
    let first = add_source(&mut sources, "first.ko", "val a=1");
    let second = add_source(&mut sources, "second.ko", "val b=2");
    let parsed = parse_file_twice(&sources, second, "second file source");
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

#[test]
fn package_and_kotlin_style_imports_preserve_source_order_and_spans() {
    let text = "package alpha.beta\nimport koven.io.println\nimport koven.collections.*\nimport koven.math.Vector as Vec\nval answer=42";
    let (sources, parsed) = parsed(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );

    let package = parsed.package().expect("package");
    assert_eq!(sources.slice(package.span).unwrap(), "package alpha.beta");
    assert_eq!(
        package
            .segments
            .iter()
            .map(|segment| sources.slice(segment.span).unwrap())
            .collect::<Vec<_>>(),
        ["alpha", "beta"]
    );

    assert_eq!(parsed.imports().len(), 3);
    assert_eq!(
        parsed
            .imports()
            .iter()
            .map(|import| sources.slice(import.span).unwrap())
            .collect::<Vec<_>>(),
        [
            "import koven.io.println",
            "import koven.collections.*",
            "import koven.math.Vector as Vec",
        ]
    );
    assert!(parsed.imports()[0].wildcard_span.is_none());
    assert_eq!(
        sources
            .slice(parsed.imports()[1].wildcard_span.expect("wildcard"))
            .unwrap(),
        "*"
    );
    let alias = parsed.imports()[2].alias.expect("alias");
    assert_eq!(sources.slice(alias.as_span).unwrap(), "as");
    assert_eq!(sources.slice(alias.name_span).unwrap(), "Vec");
    assert_eq!(parsed.roots().len(), 1);
}

#[test]
fn default_package_and_semicolons_can_separate_file_header_elements() {
    let (_, parsed) = parsed("import a.b; import c.*; val x=1");
    assert!(parsed.package().is_none());
    assert_eq!(parsed.imports().len(), 2);
    assert_eq!(parsed.roots().len(), 1);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
}

#[test]
fn malformed_header_fields_use_their_dedicated_diagnostics() {
    let (_, missing_names) = parsed("package\nimport\nval x=1");
    assert_eq!(codes(missing_names.diagnostics()), ["L0048", "L0049"]);
    assert!(missing_names.diagnostics()[0].primary_span().is_empty());
    assert!(missing_names.diagnostics()[1].primary_span().is_empty());

    let (_, missing_alias) = parsed("import a.b as\nval x=1");
    assert_eq!(codes(missing_alias.diagnostics()), ["L0050"]);

    let (wildcard_sources, wildcard_alias) = parsed("import a.* as Alias\nval x=1");
    assert_eq!(codes(wildcard_alias.diagnostics()), ["L0054"]);
    assert_eq!(
        wildcard_sources
            .slice(wildcard_alias.diagnostics()[0].primary_span())
            .unwrap(),
        "as"
    );
    assert!(wildcard_alias.imports()[0].alias.is_none());

    let (_, malformed_region) = parsed("package 123 junk\nval x=1");
    assert_eq!(codes(malformed_region.diagnostics()), ["L0048"]);
    assert_eq!(malformed_region.roots().len(), 1);

    let (_, lexer_owned) = parsed("package $ junk\nimport valid.name\nval x=1");
    assert_eq!(codes(lexer_owned.diagnostics()), ["L0001"]);
    assert_eq!(lexer_owned.imports().len(), 1);
    assert_eq!(lexer_owned.roots().len(), 1);
}

#[test]
fn header_elements_require_newline_or_semicolon_separators() {
    for text in [
        "package a import b\nval x=1",
        "import a import b\nval x=1",
        "import a val x=1",
    ] {
        let (sources, parsed) = parsed(text);
        assert_eq!(codes(parsed.diagnostics()), ["L0053"], "{text:?}");
        assert!(matches!(
            sources
                .slice(parsed.diagnostics()[0].primary_span())
                .unwrap(),
            "import" | "val"
        ));
    }

    let (_, block_comment_line) = parsed("package a /* line\nbreak */ import b\nval x=1");
    assert!(block_comment_line.diagnostics().is_empty());
}

#[test]
fn repeated_or_late_directives_are_error_roots_and_preserve_declarations() {
    let (sources, parsed) = parsed("package a\npackage b\nval x=1\nimport c");
    assert_eq!(codes(parsed.diagnostics()), ["L0051", "L0052"]);
    assert_eq!(
        sources
            .slice(parsed.diagnostics()[0].primary_span())
            .unwrap(),
        "package"
    );
    assert_eq!(
        sources
            .slice(parsed.diagnostics()[1].primary_span())
            .unwrap(),
        "import"
    );
    assert_eq!(parsed.roots().len(), 3);
    assert!(matches!(root_items(&parsed)[0], Item::Error));
    assert!(matches!(root_items(&parsed)[1], Item::Variable { .. }));
    assert!(matches!(root_items(&parsed)[2], Item::Error));
}

#[test]
fn rust_style_and_retired_module_spellings_are_not_file_header_syntax() {
    let (_, names) = parsed("module demo\nuse path\nmod child\nval x=1");
    assert!(names.package().is_none());
    assert!(names.imports().is_empty());
    assert_eq!(codes(names.diagnostics()), ["L0017"]);
    assert_eq!(names.roots().len(), 2);

    let (_, path_separator) = parsed("import a::b\nval x=1");
    assert!(codes(path_separator.diagnostics()).contains(&"L0053".to_owned()));
    assert_eq!(path_separator.roots().len(), 2);

    let (_, grouped) = parsed("import a.{b,c}\nval x=1");
    assert_eq!(codes(grouped.diagnostics()), ["L0049"]);
    assert_eq!(grouped.roots().len(), 1);

    let (_, missing_wildcard_dot) = parsed("import a*\nval x=1");
    assert!(codes(missing_wildcard_dot.diagnostics()).contains(&"L0053".to_owned()));
}

#[test]
fn nested_header_keywords_do_not_become_file_directives() {
    let (_, parsed) = parsed("val x=call(package hidden, import other)\nval y=2");
    assert!(parsed.package().is_none());
    assert!(parsed.imports().is_empty());
    assert_eq!(parsed.roots().len(), 2, "{:?}", parsed.diagnostics());
}

#[test]
fn long_import_sequence_is_preserved_without_cursor_drift() {
    let text: String = (0..256)
        .map(|index| format!("import pkg.name{index}\n"))
        .chain(std::iter::once("val x=1".to_owned()))
        .collect();
    let (_, parsed) = parsed(&text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.imports().len(), 256);
    assert_eq!(parsed.roots().len(), 1);
}
