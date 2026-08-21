//! SPEC-0058 / SPEC-0071 / SPEC-0116 的 TextMate grammar、corpus 与词法契约回归。

use std::{collections::BTreeSet, fs, path::PathBuf};

use lang_frontend::lexer::{LexemeKind, TokenKind, TriviaKind};

#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;
#[path = "support/lexer_output_assertions.rs"]
mod lexer_output_assertions;

use lexer_matrix_assertions::lex_source_twice;
use lexer_output_assertions::validate_lexed;

const REPOSITORIES: &[&str] = &[
    "comments",
    "strings",
    "characters",
    "numbers",
    "annotations",
    "declarations",
    "builtin-types",
    "keywords",
    "reserved-words",
    "operators",
    "punctuation",
];

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
    "value",
    "var",
    "vararg",
    "break",
    "continue",
    "else",
    "for",
    "if",
    "in",
    "is",
    "loop",
    "return",
    "when",
    "while",
    "borrow",
    "inout",
    "move",
    "own",
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

const RESERVED_WORDS: &[&str] = &[
    "async", "await", "suspend", "actor", "spawn", "sealed", "dyn", "where", "yield", "macro",
    "reify",
];

fn textmate_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../editors/textmate")
        .join(relative)
}

fn read_asset(relative: &str) -> String {
    fs::read_to_string(textmate_path(relative)).expect("TextMate asset must be readable UTF-8")
}

#[test]
fn lexical_contract_positive_cases_match_the_production_lexer() {
    let contract = read_asset("tests/lexical-contract.tsv");
    let mut counts = std::collections::BTreeMap::new();

    for (index, line) in contract.lines().enumerate() {
        let (family, spelling) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("contract line {} needs two columns", index + 1));
        assert!(!family.is_empty() && !spelling.is_empty());
        *counts.entry(family).or_insert(0_usize) += 1;
        if family.starts_with("reject.") {
            continue;
        }

        let text = if family == "string.escape" {
            format!("\"a{spelling}b\"")
        } else {
            spelling.to_owned()
        };
        let context = format!("{family} {spelling:?}");
        let (_, _, lexed) = lex_source_twice(
            "textmate-lexical-contract.ko",
            &text,
            &context,
            validate_lexed,
        );
        assert!(
            lexed.diagnostics().is_empty(),
            "{family} {spelling:?}: {:?}",
            lexed.diagnostics()
        );
        let tokens = lexed
            .lexemes()
            .iter()
            .filter_map(|lexeme| match lexeme.kind() {
                LexemeKind::Token(kind) => Some(kind),
                _ => None,
            })
            .collect::<Vec<_>>();

        match family {
            "symbol.operator" | "symbol.punctuation" => {
                assert_eq!(tokens.len(), 1, "{spelling:?}");
                assert!(matches!(tokens[0], TokenKind::Symbol(_)), "{spelling:?}");
            }
            "number.float" => {
                assert_eq!(tokens.len(), 1, "{spelling:?}");
                assert!(
                    matches!(tokens[0], TokenKind::FloatLiteral(_)),
                    "{spelling:?}"
                );
            }
            "number.integer" => {
                assert_eq!(tokens.len(), 1, "{spelling:?}");
                assert!(
                    matches!(tokens[0], TokenKind::IntegerLiteral(_)),
                    "{spelling:?}"
                );
            }
            "string.escape" => {
                assert!(matches!(tokens.first(), Some(TokenKind::StringStart)));
                assert!(matches!(tokens.last(), Some(TokenKind::StringEnd)));
            }
            "character" => assert_eq!(tokens, [TokenKind::CharLiteral]),
            other => panic!("unknown positive contract family {other}"),
        }
    }

    assert_eq!(counts.get("symbol.operator"), Some(&33));
    assert_eq!(counts.get("symbol.punctuation"), Some(&10));
    assert_eq!(counts.get("string.escape"), Some(&8));
    assert_eq!(counts.get("number.float"), Some(&4));
    assert_eq!(counts.get("number.integer"), Some(&7));
    assert_eq!(counts.get("character"), Some(&4));
}

#[test]
fn grammar_declares_koven_repositories_and_spelling_sets() {
    let grammar = read_asset("syntaxes/koven.tmLanguage.json");
    assert!(grammar.ends_with("}\n"));
    assert!(grammar.contains("\"scopeName\": \"source.koven\""));
    assert!(grammar.contains("\"ko\""));

    for repository in REPOSITORIES {
        assert!(
            grammar.contains(&format!("\"include\": \"#{repository}\"")),
            "top-level patterns must include repository {repository}"
        );
        assert!(
            grammar.contains(&format!("    \"{repository}\": {{")),
            "repository {repository} must exist"
        );
    }
    assert!(grammar.contains("    \"interpolation-braces\": {"));

    for spelling in HARD_KEYWORDS.iter().chain(RESERVED_WORDS) {
        assert!(
            grammar.contains(spelling),
            "grammar must retain lexer spelling {spelling}"
        );
    }
}

#[test]
fn scope_expectations_reference_declared_scopes_and_corpus_text() {
    let grammar = read_asset("syntaxes/koven.tmLanguage.json");
    let corpus = format!(
        "{}\n{}",
        read_asset("tests/highlight.ko"),
        read_asset("tests/reserved.ko")
    );
    let expectations = read_asset("tests/scopes.tsv");
    let mut count = 0_usize;

    for (index, line) in expectations.lines().enumerate() {
        let (scope, sample) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("scope expectation line {} needs two columns", index + 1));
        assert!(!scope.is_empty() && !sample.is_empty());
        assert!(
            grammar.contains(&format!("\"name\": \"{scope}\"")),
            "scope {scope} must be declared by the grammar"
        );
        assert!(
            corpus.contains(sample),
            "scope sample {sample:?} must occur in a corpus"
        );
        count += 1;
    }

    assert_eq!(
        count, 24,
        "zero or silently dropped scope fixtures are invalid"
    );
}

#[test]
fn highlight_corpus_is_lexer_valid_and_covers_primary_families() {
    let text = read_asset("tests/highlight.ko");
    let (_, _, lexed) = lex_source_twice(
        "editors/textmate/tests/highlight.ko",
        &text,
        "TextMate highlight corpus",
        validate_lexed,
    );
    assert!(lexed.diagnostics().is_empty());

    let mut token_families = BTreeSet::new();
    let mut trivia_families = BTreeSet::new();
    for lexeme in lexed.lexemes() {
        match lexeme.kind() {
            LexemeKind::Token(kind) => {
                token_families.insert(match kind {
                    TokenKind::Identifier => "identifier",
                    TokenKind::IntegerLiteral(_) => "integer",
                    TokenKind::FloatLiteral(_) => "float",
                    TokenKind::CharLiteral => "character",
                    TokenKind::StringStart => "string-start",
                    TokenKind::StringText => "string-text",
                    TokenKind::InterpolationStart => "interpolation-start",
                    TokenKind::InterpolationEnd => "interpolation-end",
                    TokenKind::StringEnd => "string-end",
                    TokenKind::Keyword(_) => "keyword",
                    TokenKind::ReservedWord(_) => "reserved",
                    TokenKind::Symbol(_) => "symbol",
                });
            }
            LexemeKind::Trivia(kind) => {
                trivia_families.insert(match kind {
                    TriviaKind::Whitespace => "whitespace",
                    TriviaKind::Newline => "newline",
                    TriviaKind::LineComment => "line-comment",
                    TriviaKind::BlockComment => "block-comment",
                });
            }
            LexemeKind::Invalid(kind) => panic!("valid corpus contained {kind:?}"),
            LexemeKind::Eof => {}
        }
    }

    assert_eq!(
        token_families,
        BTreeSet::from([
            "character",
            "float",
            "identifier",
            "integer",
            "interpolation-end",
            "interpolation-start",
            "keyword",
            "string-end",
            "string-start",
            "string-text",
            "symbol",
        ])
    );
    assert_eq!(
        trivia_families,
        BTreeSet::from(["block-comment", "line-comment", "newline", "whitespace"])
    );
}

#[test]
fn reserved_corpus_matches_the_lexer_contract() {
    let text = read_asset("tests/reserved.ko");
    let (_, _, lexed) = lex_source_twice(
        "editors/textmate/tests/reserved.ko",
        &text,
        "TextMate reserved corpus",
        validate_lexed,
    );

    let reserved_count = lexed
        .lexemes()
        .iter()
        .filter(|lexeme| matches!(lexeme.kind(), LexemeKind::Token(TokenKind::ReservedWord(_))))
        .count();
    assert_eq!(reserved_count, RESERVED_WORDS.len());
    assert_eq!(lexed.diagnostics().len(), RESERVED_WORDS.len());
    assert!(
        lexed
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.code().to_string() == "L0002")
    );
}
