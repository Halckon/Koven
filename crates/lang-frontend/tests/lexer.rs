//! SPEC-0006 的公开 Lexer 基线与 SPEC-0012 的 `&` 词法增量测试。

use lang_frontend::{
    diagnostic::{Diagnostic, Severity},
    lexer::{
        InvalidKind, Keyword, LexedFile, Lexeme, LexemeKind, LexerInternalError, ReservedWord,
        Symbol, TokenKind, TriviaKind, lex,
    },
    source::{SourceError, SourceId, SourceMap},
};

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources
        .add_source(name, text)
        .expect("test source names must be unique")
}

fn lex_source(sources: &SourceMap, source_id: SourceId) -> LexedFile {
    lex(sources, source_id).expect("valid source identity must reach the lexer")
}

fn lexeme_text<'a>(sources: &'a SourceMap, lexeme: &Lexeme) -> &'a str {
    sources
        .slice(lexeme.span())
        .expect("lexer spans must resolve in their source map")
}

fn significant_lexemes(lexed: &LexedFile) -> impl Iterator<Item = &Lexeme> {
    lexed
        .lexemes()
        .iter()
        .filter(|lexeme| !matches!(lexeme.kind(), LexemeKind::Trivia(_) | LexemeKind::Eof))
}

fn assert_complete_coverage(sources: &SourceMap, source_id: SourceId, lexed: &LexedFile) {
    let source_len = sources
        .source_text(source_id)
        .expect("test source exists")
        .len();
    let mut next_offset = 0;
    let mut eof_count = 0;

    assert_eq!(lexed.source_id(), source_id);
    for lexeme in lexed.lexemes() {
        let span = lexeme.span();
        assert_eq!(span.source_id(), source_id);
        assert_eq!(span.start(), next_offset, "lexemes must not leave gaps");

        if matches!(lexeme.kind(), LexemeKind::Eof) {
            eof_count += 1;
            assert_eq!((span.start(), span.end()), (source_len, source_len));
        } else {
            assert!(!span.is_empty(), "only EOF may have an empty span");
            assert!(span.end() > span.start());
            next_offset = span.end();
        }
    }

    assert_eq!(next_offset, source_len);
    assert_eq!(eof_count, 1, "every lexed file has exactly one EOF");
    assert!(matches!(
        lexed.lexemes().last().map(|lexeme| lexeme.kind()),
        Some(LexemeKind::Eof)
    ));
}

fn diagnostic_fingerprint(diagnostic: &Diagnostic) -> (String, Severity, &str, usize, usize) {
    let span = diagnostic.primary_span();
    (
        diagnostic.code().to_string(),
        diagnostic.severity(),
        diagnostic.message(),
        span.start(),
        span.end(),
    )
}

fn diagnostic_fingerprints(lexed: &LexedFile) -> Vec<(String, Severity, &str, usize, usize)> {
    lexed
        .diagnostics()
        .iter()
        .map(diagnostic_fingerprint)
        .collect()
}

fn lexeme_fingerprint(lexed: &LexedFile) -> Vec<(LexemeKind, usize, usize)> {
    lexed
        .lexemes()
        .iter()
        .map(|lexeme| (lexeme.kind(), lexeme.span().start(), lexeme.span().end()))
        .collect()
}

#[test]
fn all_keywords_soft_words_and_reserved_words_have_distinct_classes() {
    let hard = [
        ("class", Keyword::Class),
        ("companion", Keyword::Companion),
        ("const", Keyword::Const),
        ("enum", Keyword::Enum),
        ("extern", Keyword::Extern),
        ("fun", Keyword::Fun),
        ("import", Keyword::Import),
        ("interface", Keyword::Interface),
        ("object", Keyword::Object),
        ("package", Keyword::Package),
        ("typealias", Keyword::Typealias),
        ("val", Keyword::Val),
        ("value", Keyword::Value),
        ("var", Keyword::Var),
        ("vararg", Keyword::Vararg),
        ("break", Keyword::Break),
        ("continue", Keyword::Continue),
        ("else", Keyword::Else),
        ("for", Keyword::For),
        ("if", Keyword::If),
        ("in", Keyword::In),
        ("is", Keyword::Is),
        ("loop", Keyword::Loop),
        ("return", Keyword::Return),
        ("when", Keyword::When),
        ("while", Keyword::While),
        ("borrow", Keyword::Borrow),
        ("inout", Keyword::Inout),
        ("move", Keyword::Move),
        ("own", Keyword::Own),
        ("unsafe", Keyword::Unsafe),
        ("internal", Keyword::Internal),
        ("private", Keyword::Private),
        ("public", Keyword::Public),
        ("as", Keyword::As),
        ("false", Keyword::False),
        ("null", Keyword::Null),
        ("operator", Keyword::Operator),
        ("override", Keyword::Override),
        ("super", Keyword::Super),
        ("this", Keyword::This),
        ("true", Keyword::True),
    ];
    let soft = ["to", "infix"];
    let reserved = [
        ("async", ReservedWord::Async),
        ("await", ReservedWord::Await),
        ("suspend", ReservedWord::Suspend),
        ("actor", ReservedWord::Actor),
        ("spawn", ReservedWord::Spawn),
        ("sealed", ReservedWord::Sealed),
        ("dyn", ReservedWord::Dyn),
        ("where", ReservedWord::Where),
        ("yield", ReservedWord::Yield),
        ("macro", ReservedWord::Macro),
        ("reify", ReservedWord::Reify),
    ];

    let mut sources = SourceMap::new();
    let hard_text = hard
        .iter()
        .map(|(spelling, _)| *spelling)
        .collect::<Vec<_>>()
        .join(" ");
    let hard_id = add_source(&mut sources, "hard.ko", &hard_text);
    let hard_file = lex_source(&sources, hard_id);
    let hard_tokens = significant_lexemes(&hard_file).collect::<Vec<_>>();
    assert_eq!(hard_tokens.len(), 42);
    assert!(hard_file.diagnostics().is_empty());
    for (lexeme, (expected_text, expected_kind)) in hard_tokens.iter().zip(hard) {
        assert_eq!(lexeme_text(&sources, lexeme), expected_text);
        assert_eq!(
            lexeme.kind(),
            LexemeKind::Token(TokenKind::Keyword(expected_kind))
        );
    }

    let soft_text = soft.join(" ");
    let soft_id = add_source(&mut sources, "soft.ko", &soft_text);
    let soft_file = lex_source(&sources, soft_id);
    let soft_tokens = significant_lexemes(&soft_file).collect::<Vec<_>>();
    assert_eq!(soft_tokens.len(), 2);
    assert!(soft_file.diagnostics().is_empty());
    for (lexeme, expected) in soft_tokens.iter().zip(soft) {
        assert_eq!(lexeme_text(&sources, lexeme), expected);
        assert!(matches!(
            lexeme.kind(),
            LexemeKind::Token(TokenKind::Identifier)
        ));
    }

    let retired_id = add_source(&mut sources, "retired-module.ko", "module");
    let retired_file = lex_source(&sources, retired_id);
    let retired = significant_lexemes(&retired_file).collect::<Vec<_>>();
    assert_eq!(retired.len(), 1);
    assert_eq!(retired[0].kind(), LexemeKind::Token(TokenKind::Identifier));
    assert!(retired_file.diagnostics().is_empty());

    let reserved_text = reserved
        .iter()
        .map(|(spelling, _)| *spelling)
        .collect::<Vec<_>>()
        .join(" ");
    let reserved_id = add_source(&mut sources, "reserved.ko", &reserved_text);
    let reserved_file = lex_source(&sources, reserved_id);
    let reserved_tokens = significant_lexemes(&reserved_file).collect::<Vec<_>>();
    assert_eq!(reserved_tokens.len(), 11);
    assert_eq!(reserved_file.diagnostics().len(), 11);
    for ((lexeme, diagnostic), (expected_text, expected_kind)) in reserved_tokens
        .iter()
        .zip(reserved_file.diagnostics())
        .zip(reserved)
    {
        assert_eq!(lexeme_text(&sources, lexeme), expected_text);
        assert_eq!(
            lexeme.kind(),
            LexemeKind::Token(TokenKind::ReservedWord(expected_kind))
        );
        assert_eq!(diagnostic.code().to_string(), "L0002");
        assert_eq!(diagnostic.primary_span(), lexeme.span());
    }
}

#[test]
fn keyword_boundaries_and_ascii_identifiers_do_not_use_unicode_rules() {
    let text = "_ a A z9 _0 myclass className Class package packageName mypackage movement asyncTask error get set Int Copyable classβ next // β\n'β' \"界\"";
    let identifiers = [
        "_",
        "a",
        "A",
        "z9",
        "_0",
        "myclass",
        "className",
        "Class",
        "packageName",
        "mypackage",
        "movement",
        "asyncTask",
        "error",
        "get",
        "set",
        "Int",
        "Copyable",
        "next",
    ];
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "identifiers.ko", text);
    let lexed = lex_source(&sources, source_id);

    for expected in identifiers {
        let lexeme = significant_lexemes(&lexed)
            .find(|lexeme| lexeme_text(&sources, lexeme) == expected)
            .unwrap_or_else(|| panic!("missing identifier {expected:?}"));
        assert!(matches!(
            lexeme.kind(),
            LexemeKind::Token(TokenKind::Identifier)
        ));
    }

    let class = significant_lexemes(&lexed)
        .find(|lexeme| lexeme_text(&sources, lexeme) == "class")
        .expect("class prefix is still a complete keyword");
    assert!(matches!(
        class.kind(),
        LexemeKind::Token(TokenKind::Keyword(_))
    ));
    let package = significant_lexemes(&lexed)
        .find(|lexeme| lexeme_text(&sources, lexeme) == "package")
        .expect("package is a complete keyword");
    assert_eq!(
        package.kind(),
        LexemeKind::Token(TokenKind::Keyword(Keyword::Package))
    );
    let invalid_beta = significant_lexemes(&lexed)
        .find(|lexeme| {
            lexeme_text(&sources, lexeme) == "β"
                && matches!(
                    lexeme.kind(),
                    LexemeKind::Invalid(InvalidKind::UnexpectedCharacter)
                )
        })
        .expect("Unicode outside comments and literals is invalid");
    assert_eq!(lexed.diagnostics().len(), 1);
    assert_eq!(lexed.diagnostics()[0].code().to_string(), "L0001");
    assert_eq!(lexed.diagnostics()[0].primary_span(), invalid_beta.span());
    assert!(
        significant_lexemes(&lexed)
            .any(|lexeme| matches!(lexeme.kind(), LexemeKind::Token(TokenKind::CharLiteral)))
    );
    assert!(
        significant_lexemes(&lexed)
            .any(|lexeme| matches!(lexeme.kind(), LexemeKind::Token(TokenKind::StringText)))
    );
    assert_complete_coverage(&sources, source_id, &lexed);
}

#[test]
fn trivia_is_segmented_maximally_and_non_ascii_whitespace_is_not_hidden() {
    let text = " \t\t\n\r\n// comment\r\n/* block */\r\u{a0}\u{feff}next";
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "trivia.ko", text);
    let lexed = lex_source(&sources, source_id);
    let trivia = lexed
        .lexemes()
        .iter()
        .filter_map(|lexeme| match lexeme.kind() {
            LexemeKind::Trivia(kind) => Some((kind, lexeme_text(&sources, lexeme))),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        trivia,
        [
            (TriviaKind::Whitespace, " \t\t"),
            (TriviaKind::Newline, "\n"),
            (TriviaKind::Newline, "\r\n"),
            (TriviaKind::LineComment, "// comment"),
            (TriviaKind::Newline, "\r\n"),
            (TriviaKind::BlockComment, "/* block */"),
        ]
    );
    let diagnostics = lexed
        .diagnostics()
        .iter()
        .map(diagnostic_fingerprint)
        .collect::<Vec<_>>();
    let cr = text.find("\r\u{a0}").expect("bare CR marker");
    let nbsp = cr + 1;
    let bom = nbsp + "\u{a0}".len();
    assert_eq!(
        diagnostics,
        [
            (
                "L0001".to_owned(),
                Severity::Error,
                "unexpected character",
                cr,
                cr + 1
            ),
            (
                "L0001".to_owned(),
                Severity::Error,
                "unexpected character",
                nbsp,
                nbsp + "\u{a0}".len(),
            ),
            (
                "L0001".to_owned(),
                Severity::Error,
                "unexpected character",
                bom,
                bom + "\u{feff}".len(),
            ),
        ]
    );
    assert_complete_coverage(&sources, source_id, &lexed);
}

#[test]
fn block_comments_are_non_nested_and_end_at_the_first_closer() {
    let text = "/* outer /* inner */ after */";
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "comments.ko", text);
    let lexed = lex_source(&sources, source_id);
    let pieces = lexed
        .lexemes()
        .iter()
        .filter(|lexeme| !matches!(lexeme.kind(), LexemeKind::Eof))
        .map(|lexeme| {
            (
                format!("{:?}", lexeme.kind()),
                lexeme_text(&sources, lexeme),
            )
        })
        .collect::<Vec<_>>();

    assert!(lexed.diagnostics().is_empty());
    assert_eq!(pieces[0].1, "/* outer /* inner */");
    assert_eq!(pieces[1].1, " ");
    assert_eq!(pieces[2].1, "after");
    assert_eq!(pieces[3].1, " ");
    assert_eq!(pieces[4].1, "*");
    assert_eq!(pieces[5].1, "/");
    assert_complete_coverage(&sources, source_id, &lexed);
}

#[test]
fn decimal_numbers_ranges_and_invalid_suffixes_have_stable_boundaries() {
    let text = "0 001 1.0 1..2 1..<2 .5 1. 1e3 1.0e3 0x10 1L 1_0 next";
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "numbers.ko", text);
    let lexed = lex_source(&sources, source_id);
    let actual = significant_lexemes(&lexed)
        .map(|lexeme| {
            let class = match lexeme.kind() {
                LexemeKind::Token(TokenKind::IntegerLiteral) => "integer",
                LexemeKind::Token(TokenKind::FloatLiteral) => "float",
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
            ("float", "1.0"),
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
            ("invalid-number", "0x10"),
            ("invalid-number", "1L"),
            ("invalid-number", "1_0"),
            ("identifier", "next"),
        ]
    );
    assert_eq!(lexed.diagnostics().len(), 5);
    for diagnostic in lexed.diagnostics() {
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.code().to_string(), "L0008");
        assert_eq!(diagnostic.message(), "invalid numeric literal");
    }
    assert_complete_coverage(&sources, source_id, &lexed);
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

#[test]
fn each_lexical_error_has_a_stable_code_message_span_and_recovery_shape() {
    struct Case {
        name: &'static str,
        text: &'static str,
        code: &'static str,
        message: &'static str,
        start: usize,
        end: usize,
        invalid: Option<InvalidKind>,
        reserved: bool,
        recovers_to_next: bool,
    }

    let cases = [
        Case {
            name: "character",
            text: "β next",
            code: "L0001",
            message: "unexpected character",
            start: 0,
            end: 2,
            invalid: Some(InvalidKind::UnexpectedCharacter),
            reserved: false,
            recovers_to_next: true,
        },
        Case {
            name: "reserved",
            text: "async next",
            code: "L0002",
            message: "reserved word is not available in Koven v1",
            start: 0,
            end: 5,
            invalid: None,
            reserved: true,
            recovers_to_next: true,
        },
        Case {
            name: "comment",
            text: "/* open",
            code: "L0003",
            message: "unterminated block comment",
            start: 0,
            end: 7,
            invalid: Some(InvalidKind::UnterminatedBlockComment),
            reserved: false,
            recovers_to_next: false,
        },
        Case {
            name: "string",
            text: "\"abc\nnext",
            code: "L0004",
            message: "unterminated string literal",
            start: 0,
            end: 4,
            invalid: None,
            reserved: false,
            recovers_to_next: true,
        },
        Case {
            name: "interpolation",
            text: "\"${foo",
            code: "L0005",
            message: "unterminated string interpolation",
            start: 1,
            end: 6,
            invalid: None,
            reserved: false,
            recovers_to_next: false,
        },
        Case {
            name: "escape",
            text: "\"a\\qz\" next",
            code: "L0006",
            message: "invalid escape in string literal",
            start: 2,
            end: 4,
            invalid: Some(InvalidKind::InvalidStringEscape),
            reserved: false,
            recovers_to_next: true,
        },
        Case {
            name: "char",
            text: "'ab' next",
            code: "L0007",
            message: "invalid character literal",
            start: 0,
            end: 4,
            invalid: Some(InvalidKind::InvalidCharLiteral),
            reserved: false,
            recovers_to_next: true,
        },
        Case {
            name: "number",
            text: "1e3 next",
            code: "L0008",
            message: "invalid numeric literal",
            start: 0,
            end: 3,
            invalid: Some(InvalidKind::InvalidNumericLiteral),
            reserved: false,
            recovers_to_next: true,
        },
    ];

    for case in cases {
        let mut sources = SourceMap::new();
        let source_id = add_source(&mut sources, &format!("{}.ko", case.name), case.text);
        let lexed = lex_source(&sources, source_id);
        assert_eq!(lexed.diagnostics().len(), 1, "case {}", case.name);
        let diagnostic = &lexed.diagnostics()[0];
        assert_eq!(diagnostic.severity(), Severity::Error, "case {}", case.name);
        assert_eq!(
            diagnostic.code().to_string(),
            case.code,
            "case {}",
            case.name
        );
        assert_eq!(diagnostic.message(), case.message, "case {}", case.name);
        assert_eq!(
            (
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end()
            ),
            (case.start, case.end),
            "case {}",
            case.name
        );

        if let Some(expected) = case.invalid {
            assert!(
                significant_lexemes(&lexed).any(|lexeme| {
                    matches!(lexeme.kind(), LexemeKind::Invalid(actual) if actual == expected)
                        && lexeme.span() == diagnostic.primary_span()
                }),
                "case {} must expose its invalid region",
                case.name
            );
        }
        if case.reserved {
            assert!(significant_lexemes(&lexed).any(|lexeme| {
                matches!(lexeme.kind(), LexemeKind::Token(TokenKind::ReservedWord(_)))
                    && lexeme.span() == diagnostic.primary_span()
            }));
        }
        if case.recovers_to_next {
            assert!(significant_lexemes(&lexed).any(|lexeme| {
                matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier))
                    && lexeme_text(&sources, lexeme) == "next"
            }));
        }
        assert_complete_coverage(&sources, source_id, &lexed);
    }
}

#[test]
fn string_recovery_pops_only_the_current_mode_and_suppresses_eof_cascades() {
    let cases = [
        ("plain", "\"abc\nnext", "L0004", "next"),
        ("plain-eof", "\"abc", "L0004", ""),
        ("nested", "\"${ \"abc\nnext }\"", "L0004", "next"),
        ("nested-eof", "\"${ \"abc", "L0004", ""),
        ("escape-lf", "\"abc\\\nnext", "L0006", "next"),
        ("escape-eof", "\"abc\\", "L0006", ""),
        ("nested-escape-eof", "\"${ \"abc\\", "L0006", ""),
    ];

    for (name, text, code, recovered_identifier) in cases {
        let mut sources = SourceMap::new();
        let source_id = add_source(&mut sources, &format!("{name}.ko"), text);
        let lexed = lex_source(&sources, source_id);
        assert_eq!(lexed.diagnostics().len(), 1, "case {name}");
        assert_eq!(lexed.diagnostics()[0].code().to_string(), code);
        if !recovered_identifier.is_empty() {
            assert!(significant_lexemes(&lexed).any(|lexeme| {
                matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier))
                    && lexeme_text(&sources, lexeme) == recovered_identifier
            }));
        }
        assert_complete_coverage(&sources, source_id, &lexed);
    }
}

#[test]
fn char_at_interpolation_eof_suppresses_only_cascading_outer_errors() {
    let cases = [
        (
            "closed-invalid",
            r#""${'ab'"#,
            vec![("L0005", 1, 7), ("L0007", 3, 7)],
        ),
        ("unclosed", r#""${'"#, vec![("L0007", 3, 4)]),
    ];

    for (name, text, expected) in cases {
        let mut sources = SourceMap::new();
        let source_id = add_source(&mut sources, &format!("char-at-eof-{name}.ko"), text);
        let lexed = lex_source(&sources, source_id);
        assert_eq!(
            lexed
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
            expected
                .into_iter()
                .map(|(code, start, end)| (code.to_owned(), start, end))
                .collect::<Vec<_>>(),
            "case {name}",
        );
        assert_complete_coverage(&sources, source_id, &lexed);
    }
}

#[test]
fn terminal_eof_error_preserves_prior_independent_diagnostics() {
    let text = r#""${β/*"#;
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "independent-errors-before-eof.ko", text);
    let lexed = lex_source(&sources, source_id);

    assert_eq!(
        lexed
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
        [("L0001".to_owned(), 3, 5), ("L0003".to_owned(), 5, 7)],
    );
    assert_complete_coverage(&sources, source_id, &lexed);
}

#[test]
fn diagnostics_are_sorted_by_span_even_when_recovery_discovers_them_out_of_order() {
    let text = "\"a\\q\nβ";
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "diagnostic-order.ko", text);
    let lexed = lex_source(&sources, source_id);

    assert_eq!(
        lexed
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
        [
            ("L0004".to_owned(), 0, 4),
            ("L0006".to_owned(), 2, 4),
            ("L0001".to_owned(), 5, 7),
        ],
    );
    assert_complete_coverage(&sources, source_id, &lexed);
}

#[test]
fn representative_utf8_mode_corpus_never_panics_or_loses_byte_coverage() {
    let fragments = [
        "", "a", "0", " ", "\n", "\r", "\r\n", "β", "😀", "\u{feff}", "\0", "\"", "'", "\\", "${",
        "{", "}", "/*", "*/", "//", "..<", "!inside",
    ];
    let mode_cases = [
        r#""${{ "nested ${value}" }}tail""#,
        r#""${'ab'"#,
        "\"${ \"abc\\",
        "/* unterminated β😀",
        "'\\q'\r\nnext",
    ];

    for left in fragments {
        for right in fragments {
            let text = format!("{left}{right}");
            let mut sources = SourceMap::new();
            let source_id = add_source(&mut sources, "corpus.ko", &text);
            let lexed = lex_source(&sources, source_id);
            assert_complete_coverage(&sources, source_id, &lexed);
        }
    }
    for text in mode_cases {
        let mut sources = SourceMap::new();
        let source_id = add_source(&mut sources, "mode-corpus.ko", text);
        let lexed = lex_source(&sources, source_id);
        assert_complete_coverage(&sources, source_id, &lexed);
    }
}

#[test]
fn empty_and_invalid_files_preserve_eof_and_total_byte_coverage() {
    for (name, text) in [
        ("empty.ko", ""),
        ("valid.ko", "val value = 1\n"),
        ("invalid.ko", "β 1e3 \"a\\qz\"\nnext"),
    ] {
        let mut sources = SourceMap::new();
        let source_id = add_source(&mut sources, name, text);
        let lexed = lex_source(&sources, source_id);
        assert_complete_coverage(&sources, source_id, &lexed);
        if text.is_empty() {
            assert_eq!(lexed.lexemes().len(), 1);
            assert!(lexed.diagnostics().is_empty());
        }
    }
}

#[test]
fn repeated_lexing_and_source_load_order_do_not_change_relative_results() {
    let text = "classβ 1e3 \"${foo";
    let mut first = SourceMap::new();
    let first_id = add_source(&mut first, "same.ko", text);
    add_source(&mut first, "later.ko", "later");
    let mut second = SourceMap::new();
    add_source(&mut second, "earlier.ko", "earlier");
    let second_id = add_source(&mut second, "same.ko", text);

    let first_run = lex_source(&first, first_id);
    let repeated = lex_source(&first, first_id);
    let reordered = lex_source(&second, second_id);
    assert_eq!(
        lexeme_fingerprint(&first_run),
        lexeme_fingerprint(&repeated)
    );
    assert_eq!(
        lexeme_fingerprint(&first_run),
        lexeme_fingerprint(&reordered)
    );

    assert_eq!(
        diagnostic_fingerprints(&first_run),
        diagnostic_fingerprints(&repeated)
    );
    assert_eq!(
        diagnostic_fingerprints(&first_run),
        diagnostic_fingerprints(&reordered)
    );
    assert_eq!(
        first_run
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0001", "L0008", "L0005"],
    );
    assert!(
        first_run
            .diagnostics()
            .windows(2)
            .all(|pair| pair[0].primary_span().start() < pair[1].primary_span().start()),
        "same-source diagnostics must use deterministic source order",
    );
}

#[test]
fn source_ids_from_another_map_return_a_specific_internal_error() {
    let mut first = SourceMap::new();
    let foreign_id = add_source(&mut first, "foreign.ko", "val");
    let mut second = SourceMap::new();
    add_source(&mut second, "local.ko", "val");

    match lex(&second, foreign_id) {
        Err(LexerInternalError::Source(SourceError::InvalidSourceId { source_id })) => {
            assert_eq!(source_id, foreign_id);
        }
        other => panic!("expected the exact foreign-source error, got {other:?}"),
    }
}
