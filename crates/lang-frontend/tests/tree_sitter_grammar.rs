//! SPEC-0059 / SPEC-0070 / SPEC-0116 的 Tree-sitter fixture 与生产 Lexer/Parser 交叉验收。

use std::{fs, path::PathBuf};

use lang_frontend::lexer::{LexemeKind, TokenKind};

#[path = "support/frontend_matrix_assertions.rs"]
mod frontend_matrix_assertions;

use frontend_matrix_assertions::{lex_source_twice, parse_file_twice};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../editors/tree-sitter/test/fixtures")
        .join(name)
}

fn tree_sitter_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../editors/tree-sitter")
        .join(name)
}

fn read_fixture(name: &str) -> String {
    fs::read_to_string(fixture_path(name)).expect("Tree-sitter fixture must be readable UTF-8")
}

#[test]
fn external_scanner_word_table_matches_the_complete_production_lexer_contract() {
    const HARD_KEYWORDS: &[&str] = &[
        "class",
        "companion",
        "const",
        "enum",
        "extern",
        "fun",
        "import",
        "interface",
        "object",
        "package",
        "typealias",
        "val",
        "var",
        "vararg",
        "break",
        "continue",
        "else",
        "for",
        "if",
        "in",
        "is",
        "return",
        "when",
        "while",
        "unsafe",
        "internal",
        "private",
        "public",
        "as",
        "false",
        "null",
        "operator",
        "override",
        "super",
        "this",
        "true",
    ];
    const FUTURE_RESERVED_WORDS: &[&str] = &[
        "async", "await", "suspend", "actor", "spawn", "sealed", "dyn", "where", "yield", "macro",
        "reify",
    ];

    assert_eq!(HARD_KEYWORDS.len(), 36);
    assert_eq!(FUTURE_RESERVED_WORDS.len(), 11);
    let expected = HARD_KEYWORDS
        .iter()
        .chain(FUTURE_RESERVED_WORDS)
        .copied()
        .collect::<Vec<_>>();

    let scanner = fs::read_to_string(tree_sitter_path("src/scanner.c"))
        .expect("external scanner must be readable UTF-8");
    let table = scanner
        .split_once("static const char *const RESERVED_WORDS[] = {")
        .expect("external scanner word table start")
        .1
        .split_once("};")
        .expect("external scanner word table end")
        .0;
    let actual = table.split('"').skip(1).step_by(2).collect::<Vec<_>>();
    assert_eq!(actual, expected);

    let text = expected.join(" ");
    let (sources, _, lexed) = lex_source_twice(
        "tree-sitter-word-contract.ko",
        &text,
        "Tree-sitter word contract",
    );
    let tokens = lexed
        .lexemes()
        .iter()
        .filter_map(|lexeme| match lexeme.kind() {
            LexemeKind::Token(kind) => Some((kind, lexeme.span())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tokens.len(), expected.len());
    assert!(
        tokens[..HARD_KEYWORDS.len()]
            .iter()
            .all(|(kind, _)| { matches!(kind, TokenKind::Keyword(_)) })
    );
    assert!(
        tokens[HARD_KEYWORDS.len()..]
            .iter()
            .all(|(kind, _)| { matches!(kind, TokenKind::ReservedWord(_)) })
    );
    assert_eq!(lexed.diagnostics().len(), FUTURE_RESERVED_WORDS.len());
    for (diagnostic, expected_word) in lexed.diagnostics().iter().zip(FUTURE_RESERVED_WORDS) {
        assert_eq!(diagnostic.code().to_string(), "L0002");
        assert_eq!(
            sources
                .slice(diagnostic.primary_span())
                .expect("reserved-word span"),
            *expected_word
        );
    }
}

#[test]
fn representative_fixture_is_accepted_by_the_production_frontend() {
    let text = read_fixture("representative.ko");
    let context = "Tree-sitter representative fixture";
    let (sources, source_id, lexed) = lex_source_twice(
        "editors/tree-sitter/test/fixtures/representative.ko",
        &text,
        context,
    );
    assert!(lexed.diagnostics().is_empty());

    let parsed = parse_file_twice(&sources, source_id, text.len(), &lexed, context);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(
        parsed.package().expect("package directive").segments.len(),
        2
    );
    assert_eq!(parsed.imports().len(), 1);
    assert_eq!(parsed.roots().len(), 6);
}

#[test]
fn recovery_fixture_locks_parser_code_and_empty_span_before_later_root() {
    let text = read_fixture("recovery.ko");
    let expected_offset = text.find("\n}").expect("broken block closer") + 1;
    let context = "Tree-sitter recovery fixture";
    let (sources, source_id, lexed) = lex_source_twice(
        "editors/tree-sitter/test/fixtures/recovery.ko",
        &text,
        context,
    );
    assert!(lexed.diagnostics().is_empty());

    let parsed = parse_file_twice(&sources, source_id, text.len(), &lexed, context);
    assert_eq!(parsed.roots().len(), 2, "later declaration must recover");
    assert_eq!(parsed.diagnostics().len(), 1);
    let diagnostic = &parsed.diagnostics()[0];
    assert_eq!(diagnostic.code().to_string(), "L0009");
    assert_eq!(diagnostic.primary_span().start(), expected_offset);
    assert_eq!(diagnostic.primary_span().end(), expected_offset);
}

#[test]
fn reserved_fixture_uses_exact_lexer_diagnostics_and_preserves_following_text() {
    let text = read_fixture("reserved.ko");
    assert!(text.ends_with("val asyncTask = 4\n"));
    let context = "Tree-sitter reserved fixture";
    let (sources, source_id, lexed) = lex_source_twice(
        "editors/tree-sitter/test/fixtures/reserved.ko",
        &text,
        context,
    );

    assert_eq!(lexed.diagnostics().len(), 1);
    assert_eq!(lexed.diagnostics()[0].code().to_string(), "L0002");
    assert_eq!(
        sources
            .slice(lexed.diagnostics()[0].primary_span())
            .expect("reserved-word span"),
        "async"
    );
    assert!(lexed.lexemes().iter().any(|lexeme| {
        lexeme.kind() == LexemeKind::Token(TokenKind::Identifier)
            && sources.slice(lexeme.span()).expect("identifier span") == "value"
    }));

    let parsed = parse_file_twice(&sources, source_id, text.len(), &lexed, context);
    let last_root = parsed.roots().last().expect("following declaration root");
    assert_eq!(
        sources
            .slice(
                parsed
                    .ast()
                    .items()
                    .get(*last_root)
                    .expect("root item")
                    .span()
            )
            .expect("root span"),
        "val asyncTask = 4"
    );
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0002"]
    );
}
