//! Parser source-mutation matrices shared owner classification.

use lang_frontend::lexer::{Symbol, TokenKind};

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
