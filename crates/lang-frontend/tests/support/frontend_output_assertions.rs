//! Lexer / Parser matrix shared source-local output assertions.

use lang_frontend::{
    diagnostic::DiagnosticDetail,
    lexer::{LexedFile, LexemeKind},
    parser::SyntaxAst,
    source::{SourceId, Span},
};

fn validate_span(source_id: SourceId, source_len: usize, span: Span) {
    assert_eq!(span.source_id(), source_id);
    assert!(span.start() <= span.end());
    assert!(span.end() <= source_len);
}

pub(crate) fn validate_diagnostics(
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

pub(crate) fn validate_ast(source_id: SourceId, source_len: usize, ast: &SyntaxAst) {
    assert_eq!(ast.source_id(), source_id);
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
}
