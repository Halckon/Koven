//! SPEC-0074 / SPEC-0094 / SPEC-0106 的词法片段库存与四入口产物不变量。

use std::collections::BTreeSet;

use lang_frontend::{
    lexer::{LexedFile, LexemeKind, TokenKind, TriviaKind},
    parser::{Item, parse_block, parse_declaration, parse_expression, parse_file},
    source::{SourceId, SourceMap, Span},
};

#[path = "support/frontend_output_assertions.rs"]
mod frontend_output_assertions;
#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;

use frontend_output_assertions::{validate_ast, validate_diagnostics, validate_lexed};
use lexer_matrix_assertions::lex_source_twice;

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

const RESERVED_WORDS: &[&str] = &[
    "async", "await", "suspend", "actor", "spawn", "sealed", "dyn", "where", "yield", "macro",
    "reify",
];

const SYMBOLS: &[&str] = &[
    "(", ")", "[", "]", "{", "}", ",", ":", ";", "@", ".", "?.", "?", "?:", "!!", "!", "::", "->",
    "*", "/", "%", "+", "-", "..", "..<", "<", ">", "<=", ">=", "==", "!=", "&", "&&", "||", "+=",
    "-=", "*=", "/=", "%=", "=", "as?", "!in", "!is",
];

// Guide 01 的上下文/软关键字在 Lexer 中仍是 Identifier，库存不可把它们漏掉。
const IDENTIFIERS: &[&str] = &[
    "name", "_", "value", "loop", "own", "borrow", "inout", "move", "to", "by", "infix", "and",
    "or", "xor", "shl", "shr", "ushr",
];

const LITERALS: &[&str] = &[
    "0", "1L", "1u", "1uL", "1.0", "1f", "'x'", "\"text\"", "\"${x}\"",
];

const TRIVIA: &[&str] = &[" ", "\n", "// comment\n", "/* comment */"];

const INVALID: &[&str] = &["β", "/*", "\"", "\"${", r#""a\q""#, "''", "1e3"];

fn inventory() -> Vec<&'static str> {
    KEYWORDS
        .iter()
        .chain(RESERVED_WORDS)
        .chain(SYMBOLS)
        .chain(IDENTIFIERS)
        .chain(LITERALS)
        .chain(TRIVIA)
        .chain(INVALID)
        .copied()
        .collect()
}

fn lex_case(text: &str) -> LexedFile {
    let (_, _, lexed) = lex_source_twice("parser-token-inventory.ko", text, text, validate_lexed);
    lexed
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
    assert_eq!(KEYWORDS.len(), 36);
    assert_eq!(RESERVED_WORDS.len(), 11);
    assert_eq!(SYMBOLS.len(), 43);
    assert_eq!(IDENTIFIERS.len(), 17);
    assert_eq!(LITERALS.len(), 9);
    assert_eq!(TRIVIA.len(), 4);
    assert_eq!(INVALID.len(), 7);

    let inventory = inventory();
    assert_eq!(inventory.len(), 127);
    assert_eq!(
        inventory.iter().copied().collect::<BTreeSet<_>>().len(),
        127
    );

    let mut lexed_cases = 0;
    for text in KEYWORDS {
        let lexed = lex_case(text);
        assert!(matches!(
            first_non_eof_kind(&lexed),
            LexemeKind::Token(TokenKind::Keyword(_))
        ));
        lexed_cases += 1;
    }
    for text in IDENTIFIERS {
        let lexed = lex_case(text);
        assert_eq!(
            first_non_eof_kind(&lexed),
            LexemeKind::Token(TokenKind::Identifier),
            "identifier fragment {text:?}"
        );
        assert_eq!(lexed.lexemes().len(), 2, "identifier and EOF: {text:?}");
        assert!(
            lexed.diagnostics().is_empty(),
            "identifier fragment {text:?}"
        );
        lexed_cases += 1;
    }
    for text in RESERVED_WORDS {
        let lexed = lex_case(text);
        assert!(matches!(
            first_non_eof_kind(&lexed),
            LexemeKind::Token(TokenKind::ReservedWord(_))
        ));
        lexed_cases += 1;
    }
    for text in SYMBOLS {
        let lexed = lex_case(text);
        assert!(matches!(
            first_non_eof_kind(&lexed),
            LexemeKind::Token(TokenKind::Symbol(_))
        ));
        lexed_cases += 1;
    }
    for (text, expected) in TRIVIA.iter().zip([
        TriviaKind::Whitespace,
        TriviaKind::Newline,
        TriviaKind::LineComment,
        TriviaKind::BlockComment,
    ]) {
        let lexed = lex_case(text);
        assert_eq!(first_non_eof_kind(&lexed), LexemeKind::Trivia(expected));
        lexed_cases += 1;
    }

    let mut diagnostic_codes = BTreeSet::new();
    for text in &inventory {
        let lexed = lex_case(text);
        diagnostic_codes.extend(
            lexed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string()),
        );
        lexed_cases += 1;
    }
    assert_eq!(lexed_cases, 238);
    assert_eq!(
        diagnostic_codes,
        (1..=8).map(|code| format!("L{code:04}")).collect()
    );
}

#[test]
fn standalone_declaration_recovers_a_complete_string_as_one_user_error_region() {
    let text = "\"text\"";
    let (sources, source_id, lexed) = lex_source_twice(
        "declaration-string-recovery.ko",
        text,
        "declaration string recovery",
        validate_lexed,
    );
    let first = parse_declaration(&sources, &lexed)
        .expect("valid segmented string lexemes must not become an internal parser error");
    let repeated = parse_declaration(&sources, &lexed)
        .expect("repeated declaration string recovery must not fail internally");

    for parsed in [&first, &repeated] {
        assert_eq!(parsed.source_id(), source_id);
        validate_ast(source_id, text.len(), parsed.ast());
        validate_diagnostics(source_id, text.len(), parsed.diagnostics());
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
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic declaration string recovery"
    );
}

type WrapFragment = fn(&str) -> String;

#[derive(Clone, Copy)]
enum EntryKind {
    Expression,
    Declaration,
    Block,
    File,
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

fn validate_span(source_id: SourceId, source_len: usize, span: Span) {
    assert_eq!(span.source_id(), source_id);
    assert!(span.start() <= span.end());
    assert!(span.end() <= source_len);
}

fn parse_twice(
    entry_name: &str,
    kind: EntryKind,
    fragment: &str,
    sources: &SourceMap,
    source_id: SourceId,
    source_len: usize,
    lexed: &LexedFile,
) {
    macro_rules! validate_common {
        ($parsed:expr) => {{
            assert_eq!($parsed.source_id(), source_id);
            validate_ast(source_id, source_len, $parsed.ast());
            validate_diagnostics(source_id, source_len, $parsed.diagnostics());
        }};
    }
    macro_rules! parse_entry {
        ($parse:ident, $table:ident) => {{
            let first = $parse(sources, lexed).unwrap_or_else(|error| {
                panic!("{entry_name} fragment {fragment:?} failed internally: {error}")
            });
            let repeated = $parse(sources, lexed).unwrap_or_else(|error| {
                panic!("repeated {entry_name} fragment {fragment:?} failed internally: {error}")
            });
            for parsed in [&first, &repeated] {
                validate_common!(parsed);
                parsed
                    .ast()
                    .$table()
                    .get(parsed.root())
                    .unwrap_or_else(|error| {
                        panic!("invalid {entry_name} root for {fragment:?}: {error}")
                    });
            }
            assert_eq!(
                format!("{first:?}"),
                format!("{repeated:?}"),
                "non-deterministic {entry_name} fragment {fragment:?}"
            );
        }};
    }

    match kind {
        EntryKind::Expression => parse_entry!(parse_expression, expressions),
        EntryKind::Declaration => parse_entry!(parse_declaration, items),
        EntryKind::Block => parse_entry!(parse_block, statements),
        EntryKind::File => {
            let first = parse_file(sources, lexed).unwrap_or_else(|error| {
                panic!("file fragment {fragment:?} failed internally: {error}")
            });
            let repeated = parse_file(sources, lexed).unwrap_or_else(|error| {
                panic!("repeated file fragment {fragment:?} failed internally: {error}")
            });
            for parsed in [&first, &repeated] {
                validate_common!(parsed);
                for root in parsed.roots() {
                    parsed.ast().items().get(*root).unwrap_or_else(|error| {
                        panic!("invalid file root for {fragment:?}: {error}")
                    });
                }
                if let Some(package) = parsed.package() {
                    validate_span(source_id, source_len, package.span);
                    validate_span(source_id, source_len, package.keyword_span);
                    for segment in &package.segments {
                        validate_span(source_id, source_len, segment.span);
                    }
                }
                for import in parsed.imports() {
                    validate_span(source_id, source_len, import.span);
                    validate_span(source_id, source_len, import.keyword_span);
                    for segment in &import.segments {
                        validate_span(source_id, source_len, segment.span);
                    }
                    if let Some(span) = import.wildcard_span {
                        validate_span(source_id, source_len, span);
                    }
                    if let Some(alias) = &import.alias {
                        validate_span(source_id, source_len, alias.as_span);
                        validate_span(source_id, source_len, alias.name_span);
                    }
                }
            }
            assert_eq!(
                format!("{first:?}"),
                format!("{repeated:?}"),
                "non-deterministic file fragment {fragment:?}"
            );
        }
    }
}

#[test]
fn every_lexical_fragment_preserves_output_invariants_in_every_public_parser_entry() {
    let entries = [
        ("expression", direct as WrapFragment, EntryKind::Expression),
        (
            "declaration",
            direct as WrapFragment,
            EntryKind::Declaration,
        ),
        ("block", in_block as WrapFragment, EntryKind::Block),
        ("file", in_file as WrapFragment, EntryKind::File),
    ];
    let inventory = inventory();
    let mut executed = 0;

    for fragment in inventory {
        for (entry_name, wrap, kind) in entries {
            let text = wrap(fragment);
            let context = format!("{entry_name} fragment {fragment:?}");
            let (sources, source_id, lexed) =
                lex_source_twice("parser-token-entry.ko", &text, &context, validate_lexed);
            parse_twice(
                entry_name,
                kind,
                fragment,
                &sources,
                source_id,
                text.len(),
                &lexed,
            );
            executed += 1;
        }
    }

    assert_eq!(executed, 127 * 4);
}
