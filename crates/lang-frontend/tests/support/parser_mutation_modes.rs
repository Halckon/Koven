//! Parser source-mutation matrices shared lexical-mode classification.

use lang_frontend::lexer::TokenKind;

pub(crate) fn token_is_lexical_mode_segment(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::StringStart
            | TokenKind::StringText
            | TokenKind::StringEnd
            | TokenKind::InterpolationStart
            | TokenKind::InterpolationEnd
    )
}
