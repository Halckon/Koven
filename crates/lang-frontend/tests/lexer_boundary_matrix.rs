//! SPEC-0073 / SPEC-0103 的固定词边界、符号最长匹配与注释优先级矩阵契约。

use lang_frontend::{
    diagnostic::DiagnosticDetail,
    lexer::{LexedFile, LexemeKind, Symbol, TokenKind, TriviaKind},
    source::SourceId,
};

#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;

use lexer_matrix_assertions::lex_source_twice;

const WORDS: &[&str] = &[
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
    "async",
    "await",
    "suspend",
    "actor",
    "spawn",
    "sealed",
    "dyn",
    "where",
    "yield",
    "macro",
    "reify",
    "to",
    "infix",
];

const FIXED_SYMBOLS: &[(&str, Symbol)] = &[
    ("..<", Symbol::DotDotLess),
    ("?.", Symbol::QuestionDot),
    ("?:", Symbol::QuestionColon),
    ("!!", Symbol::BangBang),
    ("::", Symbol::ColonColon),
    ("->", Symbol::Arrow),
    ("..", Symbol::DotDot),
    ("<=", Symbol::LessEqual),
    (">=", Symbol::GreaterEqual),
    ("==", Symbol::EqualEqual),
    ("!=", Symbol::BangEqual),
    ("&&", Symbol::AndAnd),
    ("||", Symbol::OrOr),
    ("+=", Symbol::PlusEqual),
    ("-=", Symbol::MinusEqual),
    ("*=", Symbol::StarEqual),
    ("/=", Symbol::SlashEqual),
    ("%=", Symbol::PercentEqual),
    ("(", Symbol::LeftParen),
    (")", Symbol::RightParen),
    ("[", Symbol::LeftBracket),
    ("]", Symbol::RightBracket),
    ("{", Symbol::LeftBrace),
    ("}", Symbol::RightBrace),
    (",", Symbol::Comma),
    (":", Symbol::Colon),
    (";", Symbol::Semicolon),
    ("@", Symbol::At),
    (".", Symbol::Dot),
    ("?", Symbol::Question),
    ("!", Symbol::Bang),
    ("*", Symbol::Star),
    ("/", Symbol::Slash),
    ("%", Symbol::Percent),
    ("+", Symbol::Plus),
    ("-", Symbol::Minus),
    ("<", Symbol::Less),
    (">", Symbol::Greater),
    ("&", Symbol::Ampersand),
    ("=", Symbol::Equal),
];

fn validate_boundary_lexed(source_id: SourceId, source_len: usize, lexed: &LexedFile) {
    assert_eq!(lexed.source_id(), source_id);
    let mut covered = 0;
    let mut eof_count = 0;
    for (index, lexeme) in lexed.lexemes().iter().enumerate() {
        let span = lexeme.span();
        assert_eq!(span.source_id(), source_id);
        assert!(span.start() <= span.end());
        assert!(span.end() <= source_len);
        if lexeme.kind() == LexemeKind::Eof {
            eof_count += 1;
            assert_eq!(index + 1, lexed.lexemes().len());
            assert_eq!((span.start(), span.end()), (source_len, source_len));
        } else {
            assert_eq!(span.start(), covered);
            assert!(span.end() > span.start());
            covered = span.end();
        }
    }
    assert_eq!(covered, source_len);
    assert_eq!(eof_count, 1);
    for diagnostic in lexed.diagnostics() {
        let primary = diagnostic.primary_span();
        assert_eq!(primary.source_id(), source_id);
        assert!(primary.start() <= primary.end());
        assert!(primary.end() <= source_len);
        for detail in diagnostic.details() {
            if let DiagnosticDetail::Label(label) = detail {
                let span = label.span();
                assert_eq!(span.source_id(), source_id);
                assert!(span.start() <= span.end());
                assert!(span.end() <= source_len);
            }
        }
    }
}

fn lex_case(text: &str) -> LexedFile {
    let (_, _, lexed) = lex_source_twice(
        "lexer-boundary-matrix.ko",
        text,
        text,
        validate_boundary_lexed,
    );
    assert!(
        lexed.diagnostics().is_empty(),
        "{text:?}: {:?}",
        lexed.diagnostics()
    );
    lexed
}

fn significant_kinds(lexed: &LexedFile) -> Vec<LexemeKind> {
    lexed
        .lexemes()
        .iter()
        .filter_map(|lexeme| match lexeme.kind() {
            LexemeKind::Trivia(_) | LexemeKind::Eof => None,
            kind => Some(kind),
        })
        .collect()
}

#[test]
fn every_fixed_word_mutation_remains_one_identifier_without_diagnostics() {
    assert_eq!(WORDS.len(), 42 + 11 + 2);
    let mut executed = 0;

    for word in WORDS {
        let uppercase_initial = format!("{}{}", word[..1].to_ascii_uppercase(), &word[1..]);
        for text in [
            format!("x{word}"),
            format!("_{word}"),
            format!("{word}x"),
            format!("{word}_"),
            format!("{word}0"),
            uppercase_initial,
        ] {
            let lexed = lex_case(&text);
            assert_eq!(
                significant_kinds(&lexed),
                [LexemeKind::Token(TokenKind::Identifier)],
                "{text:?}",
            );
            executed += 1;
        }
    }

    assert_eq!(executed, 55 * 6);
}

#[test]
fn every_fixed_symbol_pair_uses_the_longest_available_first_token() {
    assert_eq!(FIXED_SYMBOLS.len(), 40);
    let mut executed = 0;

    for (left, _) in FIXED_SYMBOLS {
        for (right, _) in FIXED_SYMBOLS {
            let text = format!("{left}{right}");
            if text.starts_with("//") || text.starts_with("/*") {
                continue;
            }
            let (expected_spelling, expected_kind) = FIXED_SYMBOLS
                .iter()
                .filter(|(spelling, _)| text.starts_with(spelling))
                .max_by_key(|(spelling, _)| spelling.len())
                .copied()
                .expect("each matrix source starts with a fixed symbol");
            let lexed = lex_case(&text);
            let first = &lexed.lexemes()[0];
            assert_eq!(
                first.kind(),
                LexemeKind::Token(TokenKind::Symbol(expected_kind)),
                "{text:?}",
            );
            assert_eq!(
                (first.span().start(), first.span().end()),
                (0, expected_spelling.len()),
                "{text:?}",
            );
            executed += 1;
        }
    }

    assert_eq!(executed, 40 * 40 - 4);
}

#[test]
fn compound_word_symbols_enforce_their_complete_ascii_boundary_contract() {
    const CONTINUES: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_";
    let compounds = [("!in", Symbol::BangIn), ("!is", Symbol::BangIs)];
    let mut executed = 0;

    for (spelling, kind) in compounds {
        for continuation in CONTINUES.chars() {
            let text = format!("{spelling}{continuation}");
            let lexed = lex_case(&text);
            assert_eq!(
                significant_kinds(&lexed),
                [
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Bang)),
                    LexemeKind::Token(TokenKind::Identifier),
                ],
                "{text:?}",
            );
            executed += 1;
        }

        for suffix in ["", " ", ".", "?", "("] {
            let text = format!("{spelling}{suffix}");
            let lexed = lex_case(&text);
            let first = &lexed.lexemes()[0];
            assert_eq!(
                first.kind(),
                LexemeKind::Token(TokenKind::Symbol(kind)),
                "{text:?}",
            );
            assert_eq!((first.span().start(), first.span().end()), (0, 3));
            executed += 1;
        }
    }

    for continuation in CONTINUES.chars() {
        let text = format!("as?{continuation}");
        let lexed = lex_case(&text);
        let first = &lexed.lexemes()[0];
        assert_eq!(
            first.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::AsQuestion)),
            "{text:?}",
        );
        assert_eq!((first.span().start(), first.span().end()), (0, 3));
        executed += 1;
    }

    assert_eq!(CONTINUES.len(), 63);
    assert_eq!(executed, 2 * (63 + 5) + 63);
}

#[test]
fn comment_openers_take_priority_over_their_fixed_symbol_prefixes() {
    for (text, expected) in [
        ("// /= trailing", TriviaKind::LineComment),
        ("/* /= */+", TriviaKind::BlockComment),
    ] {
        let lexed = lex_case(text);
        assert_eq!(lexed.lexemes()[0].kind(), LexemeKind::Trivia(expected));
    }
}
