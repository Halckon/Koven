//! Lexer / 完整文件 Parser 矩阵共用的公开不变量断言。

use lang_frontend::{
    diagnostic::DiagnosticDetail,
    lexer::{LexedFile, LexemeKind},
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap, Span},
};

fn validate_span(source_id: SourceId, source_len: usize, span: Span) {
    assert_eq!(span.source_id(), source_id);
    assert!(span.start() <= span.end());
    assert!(span.end() <= source_len);
}

fn validate_diagnostics(
    source_id: SourceId,
    source_len: usize,
    diagnostics: &[lang_frontend::diagnostic::Diagnostic],
) {
    for diagnostic in diagnostics {
        validate_span(source_id, source_len, diagnostic.primary_span());
        for detail in diagnostic.details() {
            if let DiagnosticDetail::Label(label) = detail {
                validate_span(source_id, source_len, label.span());
            }
        }
    }
}

pub(crate) fn validate_lexed(source_id: SourceId, source_len: usize, lexed: &LexedFile) {
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
    validate_diagnostics(source_id, source_len, lexed.diagnostics());
}

pub(crate) fn parse_file_twice(
    sources: &SourceMap,
    source_id: SourceId,
    source_len: usize,
    lexed: &LexedFile,
    context: &str,
) -> ParsedFile {
    let first = parse_file(sources, lexed)
        .unwrap_or_else(|error| panic!("first parse failed for {context}: {error}"));
    let repeated = parse_file(sources, lexed)
        .unwrap_or_else(|error| panic!("repeated parse failed for {context}: {error}"));
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic parse for {context}"
    );

    validate_diagnostics(source_id, source_len, first.diagnostics());
    let ast = first.ast();
    for (_, node) in ast.items().iter() {
        validate_span(source_id, source_len, node.span());
    }
    for (_, node) in ast.statements().iter() {
        validate_span(source_id, source_len, node.span());
    }
    for (_, node) in ast.expressions().iter() {
        validate_span(source_id, source_len, node.span());
    }
    for (_, node) in ast.type_refs().iter() {
        validate_span(source_id, source_len, node.span());
    }
    first
}
