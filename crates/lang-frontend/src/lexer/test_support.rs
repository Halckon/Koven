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
