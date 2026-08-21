//! SPEC-0073 的固定词边界、符号最长匹配与注释优先级矩阵契约。

use lang_frontend::{
    lexer::{LexedFile, LexemeKind, Symbol, TokenKind, TriviaKind, lex},
    source::{SourceId, SourceMap},
};

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

fn lex_case(text: &str) -> (SourceMap, SourceId, LexedFile) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("lexer-boundary-matrix.ko", text)
        .expect("matrix source name must be unique");
    let lexed = lex(&sources, source_id).expect("matrix source must lex internally");
    (sources, source_id, lexed)
}

fn assert_complete_coverage(source_id: SourceId, source_len: usize, lexed: &LexedFile) {
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
            let (_sources, source_id, lexed) = lex_case(&text);
            assert!(
                lexed.diagnostics().is_empty(),
                "{text:?}: {:?}",
                lexed.diagnostics()
            );
            assert_eq!(
                significant_kinds(&lexed),
                [LexemeKind::Token(TokenKind::Identifier)],
                "{text:?}",
            );
            assert_complete_coverage(source_id, text.len(), &lexed);
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
            let (_sources, source_id, lexed) = lex_case(&text);
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
            assert_complete_coverage(source_id, text.len(), &lexed);
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
            let (_sources, source_id, lexed) = lex_case(&text);
            assert!(lexed.diagnostics().is_empty(), "{text:?}");
            assert_eq!(
                significant_kinds(&lexed),
                [
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Bang)),
                    LexemeKind::Token(TokenKind::Identifier),
                ],
                "{text:?}",
            );
            assert_complete_coverage(source_id, text.len(), &lexed);
            executed += 1;
        }

        for suffix in ["", " ", ".", "?", "("] {
            let text = format!("{spelling}{suffix}");
            let (_sources, source_id, lexed) = lex_case(&text);
            assert!(lexed.diagnostics().is_empty(), "{text:?}");
            let first = &lexed.lexemes()[0];
            assert_eq!(
                first.kind(),
                LexemeKind::Token(TokenKind::Symbol(kind)),
                "{text:?}",
            );
            assert_eq!((first.span().start(), first.span().end()), (0, 3));
            assert_complete_coverage(source_id, text.len(), &lexed);
            executed += 1;
        }
    }

    for continuation in CONTINUES.chars() {
        let text = format!("as?{continuation}");
        let (_sources, source_id, lexed) = lex_case(&text);
        assert!(lexed.diagnostics().is_empty(), "{text:?}");
        let first = &lexed.lexemes()[0];
        assert_eq!(
            first.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::AsQuestion)),
            "{text:?}",
        );
        assert_eq!((first.span().start(), first.span().end()), (0, 3));
        assert_complete_coverage(source_id, text.len(), &lexed);
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
        let (_sources, source_id, lexed) = lex_case(text);
        assert!(lexed.diagnostics().is_empty(), "{text:?}");
        assert_eq!(lexed.lexemes()[0].kind(), LexemeKind::Trivia(expected));
        assert_complete_coverage(source_id, text.len(), &lexed);
    }
}
