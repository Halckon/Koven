use super::*;

#[derive(Clone, Copy)]
pub(super) struct AdjacentPoisonTail {
    cursor: usize,
    boundary: Option<(usize, usize)>,
}

impl Parser<'_> {
    pub(in crate::parser::engine) fn adjacent_poison_tail_boundary(
        &self,
    ) -> Result<Option<(usize, usize)>, ParserInternalError> {
        let cache = &self.lexical_recoveries.adjacent_poison_tail;
        if let Some(tail) = cache.get()
            && tail.cursor == self.index
        {
            return Ok(tail.boundary);
        }

        // The next owner must be visible after an adjacent run of ordinary Lexer errors.
        // A new line can start another statement, and terminal/string errors keep their owners.
        let start = self.previous_significant_end();
        let mut end = start;
        for (offset, lexeme) in self.lexed.lexemes()[self.index..].iter().enumerate() {
            #[cfg(test)]
            self.significant_raw_visits
                .set(self.significant_raw_visits.get() + 1);
            if matches!(lexeme.kind(), LexemeKind::Trivia(_)) {
                continue;
            }
            if !matches!(
                lexeme.kind(),
                LexemeKind::Invalid(
                    InvalidKind::UnexpectedCharacter
                        | InvalidKind::InvalidCharLiteral
                        | InvalidKind::InvalidNumericLiteral
                ) | LexemeKind::Token(TokenKind::ReservedWord(_))
            ) || self.gap_has_line_break(end, lexeme.span().start())?
            {
                // A same-line brace can become a trailing lambda if handed to Pratt.
                let trailing_lambda = end > start
                    && matches!(
                        lexeme.kind(),
                        LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
                    )
                    && !self.gap_has_line_break(end, lexeme.span().start())?;
                let boundary =
                    (end > start && !trailing_lambda).then_some((self.index + offset, end));
                // Several enclosing groups may unwind at the same rejected suffix.
                cache.set(Some(AdjacentPoisonTail {
                    cursor: self.index,
                    boundary,
                }));
                return Ok(boundary);
            }
            end = lexeme.span().end();
        }
        Err(ParserInternalError::InvalidLexemeStream)
    }
}
