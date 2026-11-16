//! Fixed symbols, ampersands and unsupported operator spelling contracts.
use super::*;

#[test]
fn fixed_symbols_use_longest_match_and_keyword_composites_require_adjacency() {
    let symbols = [
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
        ("?.", Symbol::QuestionDot),
        ("?", Symbol::Question),
        ("?:", Symbol::QuestionColon),
        ("!!", Symbol::BangBang),
        ("!", Symbol::Bang),
        ("::", Symbol::ColonColon),
        ("->", Symbol::Arrow),
        ("*", Symbol::Star),
        ("/", Symbol::Slash),
        ("%", Symbol::Percent),
        ("+", Symbol::Plus),
        ("-", Symbol::Minus),
        ("..", Symbol::DotDot),
        ("..<", Symbol::DotDotLess),
        ("<", Symbol::Less),
        (">", Symbol::Greater),
        ("<=", Symbol::LessEqual),
        (">=", Symbol::GreaterEqual),
        ("==", Symbol::EqualEqual),
        ("!=", Symbol::BangEqual),
        ("&", Symbol::Ampersand),
        ("&&", Symbol::AndAnd),
        ("||", Symbol::OrOr),
        ("+=", Symbol::PlusEqual),
        ("-=", Symbol::MinusEqual),
        ("*=", Symbol::StarEqual),
        ("/=", Symbol::SlashEqual),
        ("%=", Symbol::PercentEqual),
        ("=", Symbol::Equal),
        ("as?", Symbol::AsQuestion),
        ("!in", Symbol::BangIn),
        ("!is", Symbol::BangIs),
    ];
    let text = symbols
        .iter()
        .map(|(spelling, _)| *spelling)
        .collect::<Vec<_>>()
        .join(" ");
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "symbols.ko", &text);
    let lexed = lex_source(&sources, source_id);
    let actual = significant_lexemes(&lexed).collect::<Vec<_>>();
    assert_eq!(actual.len(), symbols.len());
    for (lexeme, (expected_text, expected_kind)) in actual.iter().zip(symbols) {
        assert_eq!(lexeme_text(&sources, lexeme), expected_text);
        assert_eq!(
            lexeme.kind(),
            LexemeKind::Token(TokenKind::Symbol(expected_kind))
        );
    }
    assert!(lexed.diagnostics().is_empty());

    let boundaries = "as? as ? !in ! in !inside !is ! is !island";
    let boundary_id = add_source(&mut sources, "symbol-boundaries.ko", boundaries);
    let boundary_file = lex_source(&sources, boundary_id);
    let pieces = significant_lexemes(&boundary_file)
        .map(|lexeme| lexeme_text(&sources, lexeme))
        .collect::<Vec<_>>();
    assert_eq!(
        pieces,
        [
            "as?", "as", "?", "!in", "!", "in", "!", "inside", "!is", "!", "is", "!", "island",
        ]
    );
    assert_complete_coverage(&sources, boundary_id, &boundary_file);
}

#[test]
fn ampersand_and_logical_and_have_exact_lexemes_and_spans() {
    let text = "&x &&x & &x";
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "ampersand.ko", text);
    let lexed = lex_source(&sources, source_id);
    let actual = significant_lexemes(&lexed)
        .map(|lexeme| {
            (
                lexeme_text(&sources, lexeme),
                lexeme.kind(),
                lexeme.span().start(),
                lexeme.span().end(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        actual,
        [
            (
                "&",
                LexemeKind::Token(TokenKind::Symbol(Symbol::Ampersand)),
                0,
                1,
            ),
            ("x", LexemeKind::Token(TokenKind::Identifier), 1, 2),
            (
                "&&",
                LexemeKind::Token(TokenKind::Symbol(Symbol::AndAnd)),
                3,
                5,
            ),
            ("x", LexemeKind::Token(TokenKind::Identifier), 5, 6),
            (
                "&",
                LexemeKind::Token(TokenKind::Symbol(Symbol::Ampersand)),
                7,
                8,
            ),
            (
                "&",
                LexemeKind::Token(TokenKind::Symbol(Symbol::Ampersand)),
                9,
                10,
            ),
            ("x", LexemeKind::Token(TokenKind::Identifier), 10, 11),
        ]
    );
    assert!(lexed.diagnostics().is_empty());
    assert_complete_coverage(&sources, source_id, &lexed);
}

#[test]
fn unsupported_operator_spellings_split_or_report_by_available_single_characters() {
    let split_text = "++ -- << ...";
    let mut sources = SourceMap::new();
    let split_id = add_source(&mut sources, "unsupported-split.ko", split_text);
    let split_file = lex_source(&sources, split_id);
    let split = significant_lexemes(&split_file)
        .map(|lexeme| (lexeme_text(&sources, lexeme), lexeme.kind()))
        .collect::<Vec<_>>();
    assert_eq!(
        split,
        [
            ("+", LexemeKind::Token(TokenKind::Symbol(Symbol::Plus))),
            ("+", LexemeKind::Token(TokenKind::Symbol(Symbol::Plus))),
            ("-", LexemeKind::Token(TokenKind::Symbol(Symbol::Minus))),
            ("-", LexemeKind::Token(TokenKind::Symbol(Symbol::Minus))),
            ("<", LexemeKind::Token(TokenKind::Symbol(Symbol::Less))),
            ("<", LexemeKind::Token(TokenKind::Symbol(Symbol::Less))),
            ("..", LexemeKind::Token(TokenKind::Symbol(Symbol::DotDot))),
            (".", LexemeKind::Token(TokenKind::Symbol(Symbol::Dot))),
        ]
    );
    assert!(split_file.diagnostics().is_empty());
    assert_complete_coverage(&sources, split_id, &split_file);

    let invalid_text = "#|";
    let invalid_id = add_source(&mut sources, "unsupported-invalid.ko", invalid_text);
    let invalid_file = lex_source(&sources, invalid_id);
    assert_eq!(
        significant_lexemes(&invalid_file)
            .map(|lexeme| (lexeme_text(&sources, lexeme), lexeme.kind()))
            .collect::<Vec<_>>(),
        [
            ("#", LexemeKind::Invalid(InvalidKind::UnexpectedCharacter)),
            ("|", LexemeKind::Invalid(InvalidKind::UnexpectedCharacter)),
        ]
    );
    assert_eq!(
        invalid_file
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
        [("L0001".to_owned(), 0, 1), ("L0001".to_owned(), 1, 2),]
    );
    assert_complete_coverage(&sources, invalid_id, &invalid_file);
}
