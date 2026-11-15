//! Numeric, character and string literal lexical contracts.
use super::*;

#[test]
fn decimal_numbers_ranges_and_invalid_suffixes_have_stable_boundaries() {
    let text = "0 001 1.0 1L 1u 1U 1uL 1UL 1.0f 1.0F 1f 1F 1..2 1..<2 .5 1. 1e3 1.0e3 0x_10 1l 1LU 1Ul 1ul 1.0L 1.0u 1uName 1__0 next";
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "numbers.ko", text);
    let lexed = lex_source(&sources, source_id);
    let actual = significant_lexemes(&lexed)
        .map(|lexeme| {
            let class = match lexeme.kind() {
                LexemeKind::Token(TokenKind::IntegerLiteral(IntegerLiteralSuffix::None)) => {
                    "integer"
                }
                LexemeKind::Token(TokenKind::IntegerLiteral(IntegerLiteralSuffix::Long)) => "long",
                LexemeKind::Token(TokenKind::IntegerLiteral(IntegerLiteralSuffix::Unsigned)) => {
                    "unsigned"
                }
                LexemeKind::Token(TokenKind::IntegerLiteral(
                    IntegerLiteralSuffix::UnsignedLong,
                )) => "unsigned-long",
                LexemeKind::Token(TokenKind::FloatLiteral(FloatLiteralSuffix::None)) => "double",
                LexemeKind::Token(TokenKind::FloatLiteral(FloatLiteralSuffix::Float)) => "float",
                LexemeKind::Token(TokenKind::Identifier) => "identifier",
                LexemeKind::Token(TokenKind::Symbol(_)) => "symbol",
                LexemeKind::Invalid(InvalidKind::InvalidNumericLiteral) => "invalid-number",
                other => panic!("unexpected number test lexeme {other:?}"),
            };
            (class, lexeme_text(&sources, lexeme))
        })
        .collect::<Vec<_>>();

    assert_eq!(
        actual,
        [
            ("integer", "0"),
            ("integer", "001"),
            ("double", "1.0"),
            ("long", "1L"),
            ("unsigned", "1u"),
            ("unsigned", "1U"),
            ("unsigned-long", "1uL"),
            ("unsigned-long", "1UL"),
            ("float", "1.0f"),
            ("float", "1.0F"),
            ("float", "1f"),
            ("float", "1F"),
            ("integer", "1"),
            ("symbol", ".."),
            ("integer", "2"),
            ("integer", "1"),
            ("symbol", "..<"),
            ("integer", "2"),
            ("symbol", "."),
            ("integer", "5"),
            ("integer", "1"),
            ("symbol", "."),
            ("invalid-number", "1e3"),
            ("invalid-number", "1.0e3"),
            ("invalid-number", "0x_10"),
            ("invalid-number", "1l"),
            ("invalid-number", "1LU"),
            ("invalid-number", "1Ul"),
            ("invalid-number", "1ul"),
            ("invalid-number", "1.0L"),
            ("invalid-number", "1.0u"),
            ("invalid-number", "1uName"),
            ("invalid-number", "1__0"),
            ("identifier", "next"),
        ]
    );
    assert_eq!(lexed.diagnostics().len(), 11);
    for diagnostic in lexed.diagnostics() {
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.code().to_string(), "L0008");
        assert_eq!(diagnostic.message(), "invalid numeric literal");
    }
    assert_complete_coverage(&sources, source_id, &lexed);
}

#[test]
fn hex_and_binary_and_underscored_numbers_lex_correctly() {
    let valid_text = "0x10 0X2A 0xFF_AA_00 0x10L 0x10u 0x10uL 0b1010 0B1111 0b1010_0110 0b1010L 0b1010u 0b1010uL 1_000 1_000_000 10_000L 1_000u 1_000uL 1_000.5 1.234_567 1_000.5f 1_000f";
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "valid_numbers.ko", valid_text);
    let lexed = lex_source(&sources, source_id);
    assert!(
        lexed.diagnostics().is_empty(),
        "expected no diagnostics for valid numbers: {:?}",
        lexed.diagnostics()
    );
    assert_complete_coverage(&sources, source_id, &lexed);

    let invalid_cases = [
        "1_", "1__0", "0x_1", "0b_1", "0x", "0b", "1_.0", "1._0", "0xFF_", "0b10_",
    ];
    for case in invalid_cases {
        let mut sources = SourceMap::new();
        let sid = add_source(&mut sources, "invalid.ko", case);
        let lexed = lex_source(&sources, sid);
        assert!(
            !lexed.diagnostics().is_empty(),
            "expected diagnostic for {case}"
        );
        assert_eq!(
            lexed.diagnostics()[0].code().to_string(),
            "L0008",
            "expected L0008 for {case}"
        );
    }
}

#[test]
fn char_and_string_modes_preserve_unicode_escapes_and_nested_interpolation() {
    let text = r#"'a' 'β' '\\' '\'' '\"' '\n' '\r' '\t' '\0' "head\\\'\"\n\r\t\0\$${foo { "nested ${bar}" }}tail""#;
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "literals.ko", text);
    let lexed = lex_source(&sources, source_id);
    assert!(lexed.diagnostics().is_empty());
    assert_eq!(
        significant_lexemes(&lexed)
            .filter(|lexeme| matches!(lexeme.kind(), LexemeKind::Token(TokenKind::CharLiteral)))
            .count(),
        9
    );

    let string_pieces = significant_lexemes(&lexed)
        .skip(9)
        .map(|lexeme| {
            let class = match lexeme.kind() {
                LexemeKind::Token(TokenKind::StringStart) => "start",
                LexemeKind::Token(TokenKind::StringText) => "text",
                LexemeKind::Token(TokenKind::InterpolationStart) => "interpolation-start",
                LexemeKind::Token(TokenKind::InterpolationEnd) => "interpolation-end",
                LexemeKind::Token(TokenKind::StringEnd) => "end",
                LexemeKind::Token(TokenKind::Identifier) => "identifier",
                LexemeKind::Token(TokenKind::Symbol(_)) => "symbol",
                other => panic!("unexpected string test lexeme {other:?}"),
            };
            (class, lexeme_text(&sources, lexeme))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        string_pieces,
        [
            ("start", "\""),
            ("text", r#"head\\\'\"\n\r\t\0\$"#),
            ("interpolation-start", "${"),
            ("identifier", "foo"),
            ("symbol", "{"),
            ("start", "\""),
            ("text", "nested "),
            ("interpolation-start", "${"),
            ("identifier", "bar"),
            ("interpolation-end", "}"),
            ("end", "\""),
            ("symbol", "}"),
            ("interpolation-end", "}"),
            ("text", "tail"),
            ("end", "\""),
        ]
    );
    assert_complete_coverage(&sources, source_id, &lexed);
}

#[test]
fn invalid_char_forms_consume_one_region_and_recover_at_line_or_eof_boundaries() {
    let cases = [
        ("empty", "'' next", 0, 2, true),
        ("multiple", "'ab' next", 0, 4, true),
        ("escape", "'\\q' next", 0, 4, true),
        ("newline", "'a\nnext", 0, 2, true),
        ("eof", "'a", 0, 2, false),
    ];

    for (name, text, start, end, recovers_to_next) in cases {
        let mut sources = SourceMap::new();
        let source_id = add_source(&mut sources, &format!("char-{name}.ko"), text);
        let lexed = lex_source(&sources, source_id);
        assert_eq!(lexed.diagnostics().len(), 1, "case {name}");
        let diagnostic = &lexed.diagnostics()[0];
        assert_eq!(diagnostic.code().to_string(), "L0007", "case {name}");
        assert_eq!(diagnostic.message(), "invalid character literal");
        assert_eq!(
            (
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end()
            ),
            (start, end),
            "case {name}",
        );
        assert!(significant_lexemes(&lexed).any(|lexeme| {
            matches!(
                lexeme.kind(),
                LexemeKind::Invalid(InvalidKind::InvalidCharLiteral)
            ) && lexeme.span() == diagnostic.primary_span()
        }));
        assert_eq!(
            significant_lexemes(&lexed).any(|lexeme| {
                matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier))
                    && lexeme_text(&sources, lexeme) == "next"
            }),
            recovers_to_next,
            "case {name}",
        );
        assert_complete_coverage(&sources, source_id, &lexed);
    }
}
