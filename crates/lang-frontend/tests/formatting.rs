//! SPEC-0057 formatter corpus 与语义保持不变量。

use std::{fs, path::Path};

use lang_frontend::{
    formatting::format_source,
    lexer::{LexemeKind, TokenKind, TriviaKind, lex},
    parser::parse_file,
    source::{SourceId, SourceMap},
};

#[derive(Debug, PartialEq, Eq)]
struct LexicalSnapshot {
    tokens: Vec<(TokenKind, String)>,
    comments: Vec<(TriviaKind, String)>,
    newlines: Vec<String>,
}

#[test]
fn dedicated_corpus_locks_the_conservative_style() {
    let corpus = fixture_root().join("phase6/formatter-pass");
    let source = fs::read_to_string(corpus.join("complete-file.ko")).expect("formatter source");
    let expected =
        fs::read_to_string(corpus.join("complete-file.formatted")).expect("formatter expectation");

    assert_eq!(format_and_verify("complete-file.ko", &source), expected);
}

#[test]
fn every_complete_file_pass_fixture_preserves_lexical_semantics_and_is_idempotent() {
    let mut paths = Vec::new();
    collect_ko_files(&fixture_root(), &mut paths);
    paths.retain(|path| is_complete_file_pass(path));
    paths.sort();
    assert!(!paths.is_empty(), "formatter corpus must not be empty");

    for path in paths {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let name = path.to_string_lossy();
        format_and_verify(&name, &source);
    }
}

fn format_and_verify(name: &str, source: &str) -> String {
    let (sources, source_id) = loaded_source(name, source);
    let before = lexical_snapshot(&sources, source_id);
    let formatted = format_source(&sources, source_id)
        .unwrap_or_else(|error| panic!("first format failed for {name}: {error:?}"));

    let (formatted_sources, formatted_id) = loaded_source("formatted.ko", &formatted);
    let after = lexical_snapshot(&formatted_sources, formatted_id);
    assert_eq!(after, before, "lexical semantics changed for {name}");

    let formatted_lexed = lex(&formatted_sources, formatted_id).expect("formatted lexing");
    assert!(
        formatted_lexed.diagnostics().is_empty(),
        "formatted lexer diagnostics for {name}: {:?}",
        formatted_lexed.diagnostics()
    );
    let parsed =
        parse_file(&formatted_sources, &formatted_lexed).expect("formatted complete-file parse");
    assert!(
        parsed.diagnostics().is_empty(),
        "formatted parser diagnostics for {name}: {:?}",
        parsed.diagnostics()
    );

    let repeated = format_source(&formatted_sources, formatted_id)
        .unwrap_or_else(|error| panic!("repeated format failed for {name}: {error:?}"));
    assert_eq!(
        repeated, formatted,
        "formatter is not idempotent for {name}"
    );
    formatted
}

fn lexical_snapshot(sources: &SourceMap, source_id: SourceId) -> LexicalSnapshot {
    let lexed = lex(sources, source_id).expect("snapshot lexing");
    assert!(
        lexed.diagnostics().is_empty(),
        "snapshot source must lex without diagnostics"
    );
    let mut tokens = Vec::new();
    let mut comments = Vec::new();
    let mut newlines = Vec::new();

    for lexeme in lexed.lexemes().iter().copied() {
        let bytes = sources
            .slice(lexeme.span())
            .expect("lexeme span belongs to its source")
            .to_owned();
        match lexeme.kind() {
            LexemeKind::Token(kind) => tokens.push((kind, bytes)),
            LexemeKind::Trivia(kind @ (TriviaKind::LineComment | TriviaKind::BlockComment)) => {
                comments.push((kind, bytes));
            }
            LexemeKind::Trivia(TriviaKind::Newline) => newlines.push(bytes),
            LexemeKind::Trivia(TriviaKind::Whitespace) | LexemeKind::Eof => {}
            LexemeKind::Invalid(kind) => panic!("valid corpus contains invalid lexeme {kind:?}"),
        }
    }

    LexicalSnapshot {
        tokens,
        comments,
        newlines,
    }
}

fn loaded_source(name: &str, source: &str) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source(name, source)
        .expect("test source name is unique");
    (sources, source_id)
}

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn collect_ko_files(directory: &Path, output: &mut Vec<std::path::PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to enumerate {}: {error}", directory.display()));
    for entry in entries {
        let path = entry.expect("fixture directory entry").path();
        if path.is_dir() {
            collect_ko_files(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "ko") {
            output.push(path);
        }
    }
}

fn is_complete_file_pass(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some(
                "parser-file-pass"
                    | "name-pass"
                    | "type-pass"
                    | "ownership-pass"
                    | "structural-pass"
                    | "closure-pass"
                    | "formatter-pass"
            )
        )
    })
}
