//! SPEC-0059 的 Tree-sitter fixture 与生产 Lexer/Parser 交叉验收。

use std::{fs, path::PathBuf};

use lang_frontend::{
    lexer::{Keyword, LexemeKind, TokenKind, lex},
    parser::parse_file,
    source::SourceMap,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../editors/tree-sitter/test/fixtures")
        .join(name)
}

fn read_fixture(name: &str) -> String {
    fs::read_to_string(fixture_path(name)).expect("Tree-sitter fixture must be readable UTF-8")
}

#[test]
fn representative_fixture_is_accepted_by_the_production_frontend() {
    let text = read_fixture("representative.ko");
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("editors/tree-sitter/test/fixtures/representative.ko", text)
        .expect("unique fixture source");
    let lexed = lex(&sources, source_id).expect("lexer invariant");
    assert!(lexed.diagnostics().is_empty());

    let parsed = parse_file(&sources, &lexed).expect("parser invariant");
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
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("editors/tree-sitter/test/fixtures/recovery.ko", text)
        .expect("unique fixture source");
    let lexed = lex(&sources, source_id).expect("lexer invariant");
    assert!(lexed.diagnostics().is_empty());

    let parsed = parse_file(&sources, &lexed).expect("parser invariant");
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
    assert!(text.ends_with("val after = 3\n"));
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("editors/tree-sitter/test/fixtures/reserved.ko", text)
        .expect("unique fixture source");
    let lexed = lex(&sources, source_id).expect("lexer invariant");

    assert_eq!(lexed.diagnostics().len(), 1);
    assert_eq!(lexed.diagnostics()[0].code().to_string(), "L0002");
    assert_eq!(
        sources
            .slice(lexed.diagnostics()[0].primary_span())
            .expect("reserved-word span"),
        "async"
    );
    assert!(lexed.lexemes().iter().any(|lexeme| {
        lexeme.kind() == LexemeKind::Token(TokenKind::Keyword(Keyword::Value))
            && sources.slice(lexeme.span()).expect("keyword span") == "value"
    }));

    let parsed = parse_file(&sources, &lexed).expect("parser invariant");
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
        "val after = 3"
    );
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0018", "L0020", "L0047", "L0066", "L0067", "L0077", "L0017", "L0002"
        ]
    );
}
