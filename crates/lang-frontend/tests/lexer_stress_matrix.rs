//! SPEC-0150 / SPEC-0160 的 Lexer 大输入、深模式与大诊断流公开产物压力矩阵。

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
const SMALL_CALLER_STACK: usize = 64 * 1_024;

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

fn assert_lexeme(lexed: &LexedFile, index: usize, kind: LexemeKind, start: usize, end: usize) {
    let lexeme = &lexed.lexemes()[index];
    assert_eq!(lexeme.kind(), kind);
    assert_eq!((lexeme.span().start(), lexeme.span().end()), (start, end));
}

fn assert_single_diagnostic(lexed: &LexedFile, code: &str, start: usize, end: usize) {
    assert_eq!(lexed.diagnostics().len(), 1);
    let diagnostic = &lexed.diagnostics()[0];
    assert_eq!(diagnostic.code().to_string(), code);
    assert_eq!(
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end()
        ),
        (start, end)
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
fn long_invalid_lexemes_preserve_exact_recovery_segmentation() {
    let long_text = "a".repeat(LONG_RUN);

    let unterminated_comment = format!("/*{long_text}");
    let lexed = lex_stress("long-unterminated-comment.ko", &unterminated_comment);
    assert_eq!(lexed.lexemes().len(), 2);
    assert_lexeme(
        &lexed,
        0,
        LexemeKind::Invalid(InvalidKind::UnterminatedBlockComment),
        0,
        unterminated_comment.len(),
    );
    assert_single_diagnostic(&lexed, "L0003", 0, unterminated_comment.len());

    let unterminated_string = format!("\"{long_text}");
    let lexed = lex_stress("long-unterminated-string.ko", &unterminated_string);
    assert_eq!(lexed.lexemes().len(), 3);
    assert_lexeme(&lexed, 0, LexemeKind::Token(TokenKind::StringStart), 0, 1);
    assert_lexeme(
        &lexed,
        1,
        LexemeKind::Token(TokenKind::StringText),
        1,
        unterminated_string.len(),
    );
    assert_single_diagnostic(&lexed, "L0004", 0, unterminated_string.len());

    let unterminated_interpolation = format!("\"${{{long_text}");
    let lexed = lex_stress(
        "long-unterminated-interpolation.ko",
        &unterminated_interpolation,
    );
    assert_eq!(lexed.lexemes().len(), 4);
    assert_lexeme(&lexed, 0, LexemeKind::Token(TokenKind::StringStart), 0, 1);
    assert_lexeme(
        &lexed,
        1,
        LexemeKind::Token(TokenKind::InterpolationStart),
        1,
        3,
    );
    assert_lexeme(
        &lexed,
        2,
        LexemeKind::Token(TokenKind::Identifier),
        3,
        unterminated_interpolation.len(),
    );
    assert_single_diagnostic(&lexed, "L0005", 1, unterminated_interpolation.len());

    let terminal_escape = format!("\"{long_text}\\");
    let terminal_escape_start = terminal_escape.len() - 1;
    let lexed = lex_stress("long-terminal-escape.ko", &terminal_escape);
    assert_eq!(lexed.lexemes().len(), 4);
    assert_lexeme(&lexed, 0, LexemeKind::Token(TokenKind::StringStart), 0, 1);
    assert_lexeme(
        &lexed,
        1,
        LexemeKind::Token(TokenKind::StringText),
        1,
        terminal_escape_start,
    );
    assert_lexeme(
        &lexed,
        2,
        LexemeKind::Invalid(InvalidKind::InvalidStringEscape),
        terminal_escape_start,
        terminal_escape.len(),
    );
    assert_single_diagnostic(
        &lexed,
        "L0006",
        terminal_escape_start,
        terminal_escape.len(),
    );

    let long_suffix = "b".repeat(LONG_RUN);
    let interior_escape = format!("\"{long_text}\\q{long_suffix}\"");
    let interior_escape_start = 1 + long_text.len();
    let interior_escape_end = interior_escape_start + 2;
    let suffix_end = interior_escape_end + long_suffix.len();
    let lexed = lex_stress("long-interior-escape.ko", &interior_escape);
    assert_eq!(lexed.lexemes().len(), 6);
    assert_lexeme(&lexed, 0, LexemeKind::Token(TokenKind::StringStart), 0, 1);
    assert_lexeme(
        &lexed,
        1,
        LexemeKind::Token(TokenKind::StringText),
        1,
        interior_escape_start,
    );
    assert_lexeme(
        &lexed,
        2,
        LexemeKind::Invalid(InvalidKind::InvalidStringEscape),
        interior_escape_start,
        interior_escape_end,
    );
    assert_lexeme(
        &lexed,
        3,
        LexemeKind::Token(TokenKind::StringText),
        interior_escape_end,
        suffix_end,
    );
    assert_lexeme(
        &lexed,
        4,
        LexemeKind::Token(TokenKind::StringEnd),
        suffix_end,
        interior_escape.len(),
    );
    assert_single_diagnostic(&lexed, "L0006", interior_escape_start, interior_escape_end);

    let invalid_char = format!("'{long_text}'");
    let lexed = lex_stress("long-invalid-char.ko", &invalid_char);
    assert_eq!(lexed.lexemes().len(), 2);
    assert_lexeme(
        &lexed,
        0,
        LexemeKind::Invalid(InvalidKind::InvalidCharLiteral),
        0,
        invalid_char.len(),
    );
    assert_single_diagnostic(&lexed, "L0007", 0, invalid_char.len());

    let invalid_number = format!("{}e3", "1".repeat(LONG_RUN));
    let lexed = lex_stress("long-invalid-number.ko", &invalid_number);
    assert_eq!(lexed.lexemes().len(), 2);
    assert_lexeme(
        &lexed,
        0,
        LexemeKind::Invalid(InvalidKind::InvalidNumericLiteral),
        0,
        invalid_number.len(),
    );
    assert_single_diagnostic(&lexed, "L0008", 0, invalid_number.len());
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

#[test]
fn deep_modes_and_large_diagnostics_stay_iterative_on_a_small_caller_stack() {
    std::thread::Builder::new()
        .name("lexer-stress-small-caller".to_owned())
        .stack_size(SMALL_CALLER_STACK)
        .spawn(|| {
            let mut closed_modes = String::with_capacity(MODE_DEPTH * 5 + 1);
            for _ in 0..MODE_DEPTH {
                closed_modes.push_str("\"${");
            }
            closed_modes.push('x');
            for _ in 0..MODE_DEPTH {
                closed_modes.push_str("}\"");
            }
            let lexed = lex_stress("small-stack-closed-modes.ko", &closed_modes);
            assert!(lexed.diagnostics().is_empty());
            assert_eq!(lexed.lexemes().len(), MODE_DEPTH * 4 + 2);

            let mut braces = String::with_capacity(BRACE_DEPTH * 2 + 5);
            braces.push_str("\"${");
            braces.push_str(&"{".repeat(BRACE_DEPTH));
            braces.push('x');
            braces.push_str(&"}".repeat(BRACE_DEPTH + 1));
            braces.push('"');
            let lexed = lex_stress("small-stack-braces.ko", &braces);
            assert!(lexed.diagnostics().is_empty());
            assert_eq!(lexed.lexemes().len(), BRACE_DEPTH * 2 + 6);

            let unterminated = format!("{}x", "\"${".repeat(MODE_DEPTH));
            let lexed = lex_stress("small-stack-unterminated.ko", &unterminated);
            assert_eq!(lexed.lexemes().len(), MODE_DEPTH * 2 + 2);
            assert_eq!(lexed.diagnostics().len(), 1);
            assert_eq!(lexed.diagnostics()[0].code().to_string(), "L0005");

            let invalid = "界".repeat(DIAGNOSTIC_COUNT);
            let lexed = lex_stress("small-stack-diagnostics.ko", &invalid);
            assert_eq!(lexed.lexemes().len(), DIAGNOSTIC_COUNT + 1);
            assert_eq!(lexed.diagnostics().len(), DIAGNOSTIC_COUNT);
            assert!(
                lexed
                    .diagnostics()
                    .iter()
                    .all(|diagnostic| diagnostic.code().to_string() == "L0001")
            );
        })
        .expect("small Lexer caller thread must start")
        .join()
        .expect("Lexer stress must remain iterative on a small caller stack");
}
