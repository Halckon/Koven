use crate::{
    diagnostic::DiagnosticDetail,
    source::{SourceId, SourceMap, Span},
};

use super::{LexedFile, LexemeKind, lex};

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
