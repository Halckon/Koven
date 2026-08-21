//! Parser source-mutation matrices shared exact-lexeme assertions.

use lang_frontend::{
    lexer::{LexedFile, LexemeKind, TokenKind},
    source::Span,
};

pub(crate) fn assert_exact_token(
    lexed: &LexedFile,
    expected_kind: TokenKind,
    expected_span: Span,
    context: &str,
) {
    assert_eq!(
        lexed
            .lexemes()
            .iter()
            .filter(|lexeme| {
                lexeme.span() == expected_span
                    && matches!(lexeme.kind(), LexemeKind::Token(kind) if kind == expected_kind)
            })
            .count(),
        1,
        "missing exact mutation token for {context}: {:?}",
        lexed.lexemes()
    );
}
