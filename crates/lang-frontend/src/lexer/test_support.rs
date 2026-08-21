use crate::{
    diagnostic::DiagnosticDetail,
    source::{SourceId, SourceMap, Span},
};

use super::{LexedFile, LexemeKind, TokenKind, lex};

fn validate_span(source_id: SourceId, source_len: usize, span: Span) {
    assert_eq!(span.source_id(), source_id);
    assert!(span.start() <= span.end());
    assert!(span.end() <= source_len);
}

fn validate_lexed(source_id: SourceId, source_len: usize, lexed: &LexedFile) {
    assert_eq!(lexed.source_id(), source_id);
    let mut covered = 0;
    let mut eof_count = 0;

    for (index, lexeme) in lexed.lexemes().iter().enumerate() {
        let span = lexeme.span();
        validate_span(source_id, source_len, span);
        if lexeme.kind() == LexemeKind::Eof {
            eof_count += 1;
            assert_eq!(index + 1, lexed.lexemes().len());
            assert_eq!((span.start(), span.end()), (source_len, source_len));
            continue;
        }

        assert_eq!(span.start(), covered);
        assert!(span.end() > span.start());
        covered = span.end();
    }

    assert_eq!(covered, source_len);
    assert_eq!(eof_count, 1);
    for diagnostic in lexed.diagnostics() {
        validate_span(source_id, source_len, diagnostic.primary_span());
        for detail in diagnostic.details() {
            if let DiagnosticDetail::Label(label) = detail {
                validate_span(source_id, source_len, label.span());
            }
        }
    }
}

/// 为 Parser 私有算法单元测试返回首个已验证的确定 Lexer 产物。
pub(crate) fn lex_test_source_twice(
    sources: &SourceMap,
    source_id: SourceId,
    context: &str,
) -> LexedFile {
    let source_len = sources
        .source_text(source_id)
        .unwrap_or_else(|error| panic!("source lookup failed for {context}: {error}"))
        .len();
    let first = lex(sources, source_id)
        .unwrap_or_else(|error| panic!("first lex failed for {context}: {error}"));
    let repeated = lex(sources, source_id)
        .unwrap_or_else(|error| panic!("repeated lex failed for {context}: {error}"));
    validate_lexed(source_id, source_len, &first);
    validate_lexed(source_id, source_len, &repeated);
    assert_eq!(first.source_id, repeated.source_id, "{context}");
    assert_eq!(first.lexemes, repeated.lexemes, "{context}");
    assert_eq!(first.diagnostics, repeated.diagnostics, "{context}");
    first
}

fn copy_lexed(lexed: &LexedFile) -> LexedFile {
    LexedFile {
        source_id: lexed.source_id,
        lexemes: lexed.lexemes.clone(),
        diagnostics: lexed.diagnostics.clone(),
    }
}

fn replace_first_token_kind(lexed: &mut LexedFile, expected: TokenKind, replacement: TokenKind) {
    let lexeme = lexed
        .lexemes
        .iter_mut()
        .find(|lexeme| lexeme.kind == LexemeKind::Token(expected))
        .unwrap_or_else(|| panic!("test Lexer product must contain {expected:?}"));
    lexeme.kind = LexemeKind::Token(replacement);
}

/// 从正常 Lexer 产物派生 Parser 必须拒绝的 test-only 结构破坏。
pub(crate) fn malformed_test_lexed_files(
    sources: &SourceMap,
    source_id: SourceId,
    foreign_span: Span,
) -> Vec<(&'static str, LexedFile, bool)> {
    let valid = lex_test_source_twice(sources, source_id, "malformed lexeme corpus base");
    assert!(valid.lexemes.len() >= 4);
    assert!(!matches!(valid.lexemes[0].kind, LexemeKind::Eof));
    assert!(matches!(valid.lexemes.last(), Some(lexeme) if lexeme.kind == LexemeKind::Eof));

    let span = |start, end| {
        sources
            .span(source_id, start, end)
            .expect("malformed corpus span must be source-local")
    };
    let mut cases = Vec::with_capacity(8);

    let mut empty = copy_lexed(&valid);
    empty.lexemes.clear();
    cases.push(("empty stream", empty, false));

    let mut missing_eof = copy_lexed(&valid);
    missing_eof.lexemes.pop();
    cases.push(("missing EOF", missing_eof, false));

    let mut eof_before_tokens = copy_lexed(&valid);
    let eof = eof_before_tokens
        .lexemes
        .pop()
        .expect("valid stream must end in EOF");
    eof_before_tokens.lexemes.insert(0, eof);
    cases.push(("EOF before tokens", eof_before_tokens, false));

    let mut duplicate_eof = copy_lexed(&valid);
    let eof = *duplicate_eof
        .lexemes
        .last()
        .expect("valid stream must end in EOF");
    let final_index = duplicate_eof.lexemes.len() - 1;
    duplicate_eof.lexemes.insert(final_index, eof);
    cases.push(("duplicate EOF", duplicate_eof, false));

    let mut empty_non_eof = copy_lexed(&valid);
    empty_non_eof.lexemes[0].span = span(0, 0);
    cases.push(("empty non-EOF", empty_non_eof, false));

    let mut discontinuous = copy_lexed(&valid);
    discontinuous.lexemes[1].span = span(2, 3);
    cases.push(("discontinuous span", discontinuous, false));

    let mut early_eof = copy_lexed(&valid);
    early_eof
        .lexemes
        .last_mut()
        .expect("valid stream must end in EOF")
        .span = span(2, 2);
    cases.push(("early EOF", early_eof, false));

    let mut foreign = copy_lexed(&valid);
    foreign.lexemes[0].span = foreign_span;
    cases.push(("foreign span", foreign, true));

    cases
}

/// 派生结构有效、但 lexical owner token 序列不可能的 test-only 产物。
pub(crate) fn malformed_lexical_owner_test_files(
    sources: &SourceMap,
    source_id: SourceId,
) -> Vec<(&'static str, LexedFile)> {
    let valid = lex_test_source_twice(sources, source_id, "malformed lexical owner corpus base");
    assert_eq!(valid.lexemes.len(), 4);
    assert!(matches!(valid.lexemes[0].kind, LexemeKind::Token(_)));
    assert!(matches!(valid.lexemes[2].kind, LexemeKind::Token(_)));
    assert!(matches!(valid.lexemes[3].kind, LexemeKind::Eof));
    let mut cases = Vec::with_capacity(6);

    let mut unmatched_string_end = copy_lexed(&valid);
    unmatched_string_end.lexemes[0].kind = LexemeKind::Token(TokenKind::StringEnd);
    cases.push(("unmatched StringEnd", unmatched_string_end));

    let mut unmatched_interpolation_end = copy_lexed(&valid);
    unmatched_interpolation_end.lexemes[0].kind = LexemeKind::Token(TokenKind::InterpolationEnd);
    cases.push(("unmatched InterpolationEnd", unmatched_interpolation_end));

    let mut dangling_string_start = copy_lexed(&valid);
    dangling_string_start.lexemes[0].kind = LexemeKind::Token(TokenKind::StringStart);
    cases.push(("dangling StringStart", dangling_string_start));

    let mut dangling_interpolation_start = copy_lexed(&valid);
    dangling_interpolation_start.lexemes[0].kind = LexemeKind::Token(TokenKind::InterpolationStart);
    cases.push(("dangling InterpolationStart", dangling_interpolation_start));

    let mut string_closed_as_interpolation = copy_lexed(&valid);
    string_closed_as_interpolation.lexemes[0].kind = LexemeKind::Token(TokenKind::StringStart);
    string_closed_as_interpolation.lexemes[2].kind = LexemeKind::Token(TokenKind::InterpolationEnd);
    cases.push((
        "StringStart closed as InterpolationEnd",
        string_closed_as_interpolation,
    ));

    let mut interpolation_closed_as_string = copy_lexed(&valid);
    interpolation_closed_as_string.lexemes[0].kind =
        LexemeKind::Token(TokenKind::InterpolationStart);
    interpolation_closed_as_string.lexemes[2].kind = LexemeKind::Token(TokenKind::StringEnd);
    cases.push((
        "InterpolationStart closed as StringEnd",
        interpolation_closed_as_string,
    ));

    cases
}

/// 保留生产诊断，同时移除其 lexical owner token 的 test-only 产物。
pub(crate) fn mismatched_recovery_diagnostic_test_files(
    sources: &mut SourceMap,
) -> Vec<(&'static str, &'static str, LexedFile)> {
    let mut derive = |name, text, context| {
        let source_id = sources
            .add_source(name, text)
            .unwrap_or_else(|error| panic!("mismatch source setup failed for {context}: {error}"));
        copy_lexed(&lex_test_source_twice(sources, source_id, context))
    };
    let mut cases = Vec::with_capacity(4);

    let mut unterminated_string = derive(
        "mismatch-unterminated-string.ko",
        "\"abc",
        "unterminated string diagnostic mismatch",
    );
    replace_first_token_kind(
        &mut unterminated_string,
        TokenKind::StringStart,
        TokenKind::Identifier,
    );
    cases.push((
        "unterminated string without opener",
        "L0004",
        unterminated_string,
    ));

    let mut unterminated_interpolation = derive(
        "mismatch-unterminated-interpolation.ko",
        "\"${a",
        "unterminated interpolation diagnostic mismatch",
    );
    replace_first_token_kind(
        &mut unterminated_interpolation,
        TokenKind::InterpolationStart,
        TokenKind::Identifier,
    );
    cases.push((
        "unterminated interpolation without opener",
        "L0005",
        unterminated_interpolation,
    ));

    let mut invalid_escape = derive(
        "mismatch-invalid-escape.ko",
        r#""a\q""#,
        "invalid escape diagnostic mismatch",
    );
    replace_first_token_kind(
        &mut invalid_escape,
        TokenKind::StringStart,
        TokenKind::Identifier,
    );
    replace_first_token_kind(
        &mut invalid_escape,
        TokenKind::StringEnd,
        TokenKind::Identifier,
    );
    cases.push(("invalid escape outside string", "L0006", invalid_escape));

    let mut terminal_escape = derive(
        "mismatch-terminal-escape.ko",
        "\"abc\\",
        "terminal escape diagnostic mismatch",
    );
    replace_first_token_kind(
        &mut terminal_escape,
        TokenKind::StringStart,
        TokenKind::Identifier,
    );
    cases.push(("terminal escape outside string", "L0006", terminal_escape));

    cases
}
