//! SPEC-0150 的 Lexer 大输入、深模式与大诊断流公开产物压力矩阵。

use lang_frontend::lexer::{
    IntegerLiteralSuffix, InvalidKind, LexedFile, LexemeKind, Symbol, TokenKind, TriviaKind,
};

#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;
#[path = "support/lexer_output_assertions.rs"]
mod lexer_output_assertions;

use lexer_matrix_assertions::lex_source_twice;
use lexer_output_assertions::validate_lexed;

const LONG_RUN: usize = 65_536;
const MODE_DEPTH: usize = 4_096;
const BRACE_DEPTH: usize = 16_384;
const DIAGNOSTIC_COUNT: usize = 4_096;

fn lex_stress(name: &str, source: &str) -> LexedFile {
    let (_, _, lexed) = lex_source_twice(name, source, name, validate_lexed);
    lexed
}

fn assert_single_lexeme(source: &str, expected: LexemeKind, name: &str) {
    let lexed = lex_stress(name, source);
    assert!(lexed.diagnostics().is_empty(), "{name}");
    assert_eq!(lexed.lexemes().len(), 2, "{name}");
    assert_eq!(lexed.lexemes()[0].kind(), expected, "{name}");
    assert_eq!(
        (
            lexed.lexemes()[0].span().start(),
            lexed.lexemes()[0].span().end()
        ),
        (0, source.len()),
        "{name}"
    );
}

#[test]
fn long_maximal_runs_preserve_exact_lexeme_segmentation() {
    let identifier = "a".repeat(LONG_RUN);
    assert_single_lexeme(
        &identifier,
        LexemeKind::Token(TokenKind::Identifier),
        "long-identifier.ko",
    );

    let number = "1".repeat(LONG_RUN);
    assert_single_lexeme(
        &number,
        LexemeKind::Token(TokenKind::IntegerLiteral(IntegerLiteralSuffix::None)),
        "long-number.ko",
    );

    let whitespace = " ".repeat(LONG_RUN);
    assert_single_lexeme(
        &whitespace,
        LexemeKind::Trivia(TriviaKind::Whitespace),
        "long-whitespace.ko",
    );

    let line_comment = format!("//{}", "a".repeat(LONG_RUN));
    assert_single_lexeme(
        &line_comment,
        LexemeKind::Trivia(TriviaKind::LineComment),
        "long-line-comment.ko",
    );

    let block_comment = format!("/*{}*/", "a".repeat(LONG_RUN));
    assert_single_lexeme(
        &block_comment,
        LexemeKind::Trivia(TriviaKind::BlockComment),
        "long-block-comment.ko",
    );

    let string_text = "界".repeat(LONG_RUN / 4);
    let string = format!("\"{string_text}\"");
    let lexed = lex_stress("long-string.ko", &string);
    assert!(lexed.diagnostics().is_empty());
    assert_eq!(
        lexed
            .lexemes()
            .iter()
            .map(|lexeme| lexeme.kind())
            .collect::<Vec<_>>(),
        [
            LexemeKind::Token(TokenKind::StringStart),
            LexemeKind::Token(TokenKind::StringText),
            LexemeKind::Token(TokenKind::StringEnd),
            LexemeKind::Eof,
        ]
    );
    assert_eq!(
        (
            lexed.lexemes()[1].span().start(),
            lexed.lexemes()[1].span().end()
        ),
        (1, 1 + string_text.len())
    );
}

#[test]
fn deeply_nested_string_interpolation_closes_the_iterative_mode_stack() {
    let mut source = String::with_capacity(MODE_DEPTH * 5 + 1);
    for _ in 0..MODE_DEPTH {
        source.push_str("\"${");
    }
    source.push('x');
    for _ in 0..MODE_DEPTH {
        source.push_str("}\"");
    }

    let lexed = lex_stress("deep-string-interpolation.ko", &source);
    assert!(lexed.diagnostics().is_empty());
    assert_eq!(lexed.lexemes().len(), MODE_DEPTH * 4 + 2);
    let count = |kind| {
        lexed
            .lexemes()
            .iter()
            .filter(|lexeme| lexeme.kind() == kind)
            .count()
    };
    assert_eq!(count(LexemeKind::Token(TokenKind::StringStart)), MODE_DEPTH);
    assert_eq!(
        count(LexemeKind::Token(TokenKind::InterpolationStart)),
        MODE_DEPTH
    );
    assert_eq!(
        count(LexemeKind::Token(TokenKind::InterpolationEnd)),
        MODE_DEPTH
    );
    assert_eq!(count(LexemeKind::Token(TokenKind::StringEnd)), MODE_DEPTH);
    assert_eq!(count(LexemeKind::Token(TokenKind::Identifier)), 1);
    assert_eq!(count(LexemeKind::Eof), 1);
}

#[test]
fn deep_interpolation_braces_balance_without_parser_recursion() {
    let mut source = String::with_capacity(BRACE_DEPTH * 2 + 5);
    source.push_str("\"${");
    source.push_str(&"{".repeat(BRACE_DEPTH));
    source.push('x');
    source.push_str(&"}".repeat(BRACE_DEPTH + 1));
    source.push('"');

    let lexed = lex_stress("deep-interpolation-braces.ko", &source);
    assert!(lexed.diagnostics().is_empty());
    assert_eq!(lexed.lexemes().len(), BRACE_DEPTH * 2 + 6);
    assert_eq!(
        lexed
            .lexemes()
            .iter()
            .filter(|lexeme| {
                lexeme.kind() == LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
            })
            .count(),
        BRACE_DEPTH
    );
    assert_eq!(
        lexed
            .lexemes()
            .iter()
            .filter(|lexeme| {
                lexeme.kind() == LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace))
            })
            .count(),
        BRACE_DEPTH
    );
    assert_eq!(
        lexed
            .lexemes()
            .iter()
            .filter(|lexeme| lexeme.kind() == LexemeKind::Token(TokenKind::InterpolationEnd))
            .count(),
        1
    );
}

#[test]
fn deeply_unterminated_modes_report_only_the_innermost_interpolation() {
    let source = format!("{}x", "\"${".repeat(MODE_DEPTH));
    let lexed = lex_stress("deep-unterminated-interpolation.ko", &source);
    assert_eq!(lexed.lexemes().len(), MODE_DEPTH * 2 + 2);
    assert_eq!(lexed.diagnostics().len(), 1);
    let diagnostic = &lexed.diagnostics()[0];
    assert_eq!(diagnostic.code().to_string(), "L0005");
    assert_eq!(
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end()
        ),
        (MODE_DEPTH * 3 - 2, source.len())
    );
}

#[test]
fn large_multibyte_diagnostic_stream_is_byte_accurate_and_sorted() {
    let source = "界".repeat(DIAGNOSTIC_COUNT);
    let lexed = lex_stress("large-diagnostic-stream.ko", &source);
    assert_eq!(lexed.lexemes().len(), DIAGNOSTIC_COUNT + 1);
    assert_eq!(lexed.diagnostics().len(), DIAGNOSTIC_COUNT);

    for (index, (lexeme, diagnostic)) in lexed.lexemes().iter().zip(lexed.diagnostics()).enumerate()
    {
        let start = index * '界'.len_utf8();
        let end = start + '界'.len_utf8();
        assert_eq!(
            lexeme.kind(),
            LexemeKind::Invalid(InvalidKind::UnexpectedCharacter)
        );
        assert_eq!((lexeme.span().start(), lexeme.span().end()), (start, end));
        assert_eq!(diagnostic.code().to_string(), "L0001");
        assert_eq!(
            (
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end()
            ),
            (start, end)
        );
    }
}
