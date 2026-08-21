//! Parser source-mutation matrices shared token-slot enumeration.

use lang_frontend::{
    lexer::{LexedFile, LexemeKind, TokenKind},
    source::Span,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct MutationSlot {
    pub(crate) kind: TokenKind,
    pub(crate) span: Span,
}

pub(crate) fn original_token_slots(lexed: &LexedFile, original_end: usize) -> Vec<MutationSlot> {
    lexed
        .lexemes()
        .iter()
        .filter_map(|lexeme| {
            let LexemeKind::Token(kind) = lexeme.kind() else {
                return None;
            };
            (lexeme.span().end() <= original_end).then_some(MutationSlot {
                kind,
                span: lexeme.span(),
            })
        })
        .collect()
}
