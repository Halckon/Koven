//! SPEC-0074 的完整词法片段到四个公开 Parser 入口矩阵契约。

use std::collections::BTreeSet;

use lang_frontend::{
    lexer::{LexedFile, LexemeKind, TokenKind, TriviaKind, lex},
    parser::{Item, parse_block, parse_declaration, parse_expression, parse_file},
    source::{SourceId, SourceMap},
};

const KEYWORDS: &[&str] = &[
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

const SYMBOLS: &[&str] = &[
    "(", ")", "[", "]", "{", "}", ",", ":", ";", "@", ".", "?.", "?", "?:", "!!", "!", "::", "->",
    "*", "/", "%", "+", "-", "..", "..<", "<", ">", "<=", ">=", "==", "!=", "&", "&&", "||", "+=",
    "-=", "*=", "/=", "%=", "=", "as?", "!in", "!is",
];

const ATOMS: &[&str] = &[
    "name", "_", "to", "infix", "0", "1L", "1u", "1uL", "1.0", "1f", "'x'", "\"text\"", "\"${x}\"",
];

const TRIVIA: &[&str] = &[" ", "\n", "// comment\n", "/* comment */"];

const INVALID: &[&str] = &["β", "/*", "\"", "\"${", r#""a\q""#, "''", "1e3"];

fn inventory() -> Vec<&'static str> {
    KEYWORDS
        .iter()
        .chain(RESERVED_WORDS)
        .chain(SYMBOLS)
        .chain(ATOMS)
        .chain(TRIVIA)
        .chain(INVALID)
        .copied()
        .collect()
}

fn lex_case(text: &str) -> (SourceId, LexedFile) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-token-inventory.ko", text)
        .expect("matrix source name must be unique");
    let lexed = lex(&sources, source_id).expect("matrix source must lex internally");
    (source_id, lexed)
}

fn assert_lexeme_coverage(source_id: SourceId, source_len: usize, lexed: &LexedFile) {
    let mut next_offset = 0;
    let mut eof_count = 0;
    for lexeme in lexed.lexemes() {
        let span = lexeme.span();
        assert_eq!(span.source_id(), source_id);
        assert_eq!(span.start(), next_offset);
        if lexeme.kind() == LexemeKind::Eof {
            eof_count += 1;
            assert_eq!((span.start(), span.end()), (source_len, source_len));
        } else {
            assert!(span.end() > span.start());
            next_offset = span.end();
        }
    }
    assert_eq!(next_offset, source_len);
    assert_eq!(eof_count, 1);
}

fn first_non_eof_kind(lexed: &LexedFile) -> LexemeKind {
    lexed
        .lexemes()
        .first()
        .filter(|lexeme| lexeme.kind() != LexemeKind::Eof)
        .expect("inventory fragment must produce a non-EOF lexeme")
        .kind()
}

#[test]
fn inventory_is_unique_and_covers_every_public_lexical_family() {
    assert_eq!(KEYWORDS.len(), 42);
    assert_eq!(RESERVED_WORDS.len(), 11);
    assert_eq!(SYMBOLS.len(), 43);
    assert_eq!(ATOMS.len(), 13);
    assert_eq!(TRIVIA.len(), 4);
    assert_eq!(INVALID.len(), 7);

    let inventory = inventory();
    assert_eq!(inventory.len(), 120);
    assert_eq!(
        inventory.iter().copied().collect::<BTreeSet<_>>().len(),
        120
    );

    for text in KEYWORDS {
        let (_, lexed) = lex_case(text);
        assert!(matches!(
            first_non_eof_kind(&lexed),
            LexemeKind::Token(TokenKind::Keyword(_))
        ));
    }
    for text in RESERVED_WORDS {
        let (_, lexed) = lex_case(text);
        assert!(matches!(
            first_non_eof_kind(&lexed),
            LexemeKind::Token(TokenKind::ReservedWord(_))
        ));
    }
    for text in SYMBOLS {
        let (_, lexed) = lex_case(text);
        assert!(matches!(
            first_non_eof_kind(&lexed),
            LexemeKind::Token(TokenKind::Symbol(_))
        ));
    }
    for (text, expected) in TRIVIA.iter().zip([
        TriviaKind::Whitespace,
        TriviaKind::Newline,
        TriviaKind::LineComment,
        TriviaKind::BlockComment,
    ]) {
        let (_, lexed) = lex_case(text);
        assert_eq!(first_non_eof_kind(&lexed), LexemeKind::Trivia(expected));
    }

    let mut diagnostic_codes = BTreeSet::new();
    for text in &inventory {
        let (source_id, lexed) = lex_case(text);
        assert_lexeme_coverage(source_id, text.len(), &lexed);
        diagnostic_codes.extend(
            lexed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string()),
        );
    }
    assert_eq!(
        diagnostic_codes,
        (1..=8).map(|code| format!("L{code:04}")).collect()
    );
}

#[test]
fn standalone_declaration_recovers_a_complete_string_as_one_user_error_region() {
    let text = "\"text\"";
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("declaration-string-recovery.ko", text)
        .expect("regression source name must be unique");
    let lexed = lex(&sources, source_id).expect("regression source must lex internally");
    let parsed = parse_declaration(&sources, &lexed)
        .expect("valid segmented string lexemes must not become an internal parser error");

    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code().to_string(),
                    diagnostic.primary_span().start(),
                    diagnostic.primary_span().end(),
                )
            })
            .collect::<Vec<_>>(),
        [("L0017".to_owned(), 0, text.len())]
    );
    let root = parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("declaration error root must resolve");
    assert_eq!((root.span().start(), root.span().end()), (0, text.len()));
    assert_eq!(root.payload(), &Item::Error);
}

type ParseEntry = fn(&SourceMap, &LexedFile) -> Result<String, String>;
type WrapFragment = fn(&str) -> String;

fn expression_fingerprint(sources: &SourceMap, lexed: &LexedFile) -> Result<String, String> {
    parse_expression(sources, lexed)
        .map(|parsed| format!("{parsed:?}"))
        .map_err(|error| error.to_string())
}

fn declaration_fingerprint(sources: &SourceMap, lexed: &LexedFile) -> Result<String, String> {
    parse_declaration(sources, lexed)
        .map(|parsed| format!("{parsed:?}"))
        .map_err(|error| error.to_string())
}

fn block_fingerprint(sources: &SourceMap, lexed: &LexedFile) -> Result<String, String> {
    parse_block(sources, lexed)
        .map(|parsed| format!("{parsed:?}"))
        .map_err(|error| error.to_string())
}

fn file_fingerprint(sources: &SourceMap, lexed: &LexedFile) -> Result<String, String> {
    parse_file(sources, lexed)
        .map(|parsed| format!("{parsed:?}"))
        .map_err(|error| error.to_string())
}

fn direct(fragment: &str) -> String {
    fragment.to_owned()
}

fn in_block(fragment: &str) -> String {
    format!("{{ {fragment}\nval after = 1 }}")
}

fn in_file(fragment: &str) -> String {
    format!("{fragment}\nval after = 1")
}

#[test]
fn every_lexical_fragment_is_total_and_deterministic_in_every_public_parser_entry() {
    let entries = [
        (
            "expression",
            direct as WrapFragment,
            expression_fingerprint as ParseEntry,
        ),
        (
            "declaration",
            direct as WrapFragment,
            declaration_fingerprint as ParseEntry,
        ),
        (
            "block",
            in_block as WrapFragment,
            block_fingerprint as ParseEntry,
        ),
        (
            "file",
            in_file as WrapFragment,
            file_fingerprint as ParseEntry,
        ),
    ];
    let inventory = inventory();
    let mut executed = 0;

    for fragment in inventory {
        for (entry_name, wrap, parse) in entries {
            let text = wrap(fragment);
            let mut sources = SourceMap::new();
            let source_id = sources
                .add_source("parser-token-entry.ko", &text)
                .expect("matrix source name must be unique");
            let lexed = lex(&sources, source_id).expect("matrix source must lex internally");
            assert_lexeme_coverage(source_id, text.len(), &lexed);

            let first = parse(&sources, &lexed).unwrap_or_else(|error| {
                panic!("{entry_name} fragment {fragment:?} failed internally: {error}")
            });
            let repeated = parse(&sources, &lexed).unwrap_or_else(|error| {
                panic!("repeated {entry_name} fragment {fragment:?} failed internally: {error}")
            });
            assert_eq!(
                first, repeated,
                "non-deterministic {entry_name} fragment {fragment:?}"
            );
            executed += 1;
        }
    }

    assert_eq!(executed, 120 * 4);
}
