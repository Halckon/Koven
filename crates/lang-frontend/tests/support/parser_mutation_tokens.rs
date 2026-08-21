//! Parser source-mutation matrices shared token-owner classification.

use lang_frontend::{
    lexer::{LexedFile, LexemeKind, Symbol, TokenKind},
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

pub(crate) fn token_affects_owner(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::StringStart
            | TokenKind::StringEnd
            | TokenKind::InterpolationStart
            | TokenKind::InterpolationEnd
            | TokenKind::Symbol(
                Symbol::LeftParen
                    | Symbol::RightParen
                    | Symbol::LeftBracket
                    | Symbol::RightBracket
                    | Symbol::LeftBrace
                    | Symbol::RightBrace
                    | Symbol::Less
                    | Symbol::Greater
            )
    )
}
