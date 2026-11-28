use super::*;

#[derive(Clone, Copy)]
pub(super) struct AdjacentPoisonTail {
    cursor: usize,
    boundary: Option<(usize, usize)>,
}

#[derive(Clone, Copy)]
pub(super) struct ElsePoisonTail {
    start: usize,
    boundary: usize,
}

fn is_ordinary_poison(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Invalid(
            InvalidKind::UnexpectedCharacter
                | InvalidKind::InvalidCharLiteral
                | InvalidKind::InvalidNumericLiteral
        ) | LexemeKind::Token(TokenKind::ReservedWord(_))
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ElsePoisonScan {
    FoundElse { boundary: usize, last_poison: usize },
    Rejected { boundary: usize },
    Exhausted,
}

pub(super) fn scan_poison_before_else_core<F>(
    len: usize,
    start: usize,
    mut kind_at: F,
) -> ElsePoisonScan
where
    F: FnMut(usize) -> LexemeKind,
{
    let mut last_poison = None;
    let mut index = start;
    while index < len {
        let kind = kind_at(index);
        if matches!(kind, LexemeKind::Trivia(_)) {
            index += 1;
            continue;
        }
        if is_ordinary_poison(kind) {
            last_poison = Some(index);
            index += 1;
            continue;
        }
        if let Some(last_poison) = last_poison
            && kind == LexemeKind::Token(TokenKind::Keyword(Keyword::Else))
        {
            return ElsePoisonScan::FoundElse {
                boundary: index,
                last_poison,
            };
        }
        return ElsePoisonScan::Rejected { boundary: index };
    }
    ElsePoisonScan::Exhausted
}

impl Parser<'_> {
    /// Else is an explicit caller delimiter, including across trivia and newlines.
    pub(in crate::parser::engine) fn recover_poison_before_else(
        &mut self,
        stops: Stops,
    ) -> Result<Option<usize>, ParserInternalError> {
        if stops.delimiters & Stops::ELSE == 0 || !self.is_poison() {
            return Ok(None);
        }
        let cache = &self.lexical_recoveries.else_poison_tail;
        if cache
            .get()
            .is_some_and(|tail| (tail.start..=tail.boundary).contains(&self.index))
        {
            return Ok(None);
        }

        let lexemes = self.lexed.lexemes();
        match scan_poison_before_else_core(lexemes.len(), self.index, |index| {
            #[cfg(test)]
            self.significant_raw_visits
                .set(self.significant_raw_visits.get() + 1);
            lexemes[index].kind()
        }) {
            ElsePoisonScan::FoundElse {
                boundary,
                last_poison,
            } => {
                self.index = boundary;
                Ok(Some(lexemes[last_poison].span().end()))
            }
            ElsePoisonScan::Rejected { boundary } => {
                // A rejected run cannot reach Else from any cursor within the same run.
                cache.set(Some(ElsePoisonTail {
                    start: self.index,
                    boundary,
                }));
                Ok(None)
            }
            ElsePoisonScan::Exhausted => Err(ParserInternalError::InvalidLexemeStream),
        }
    }

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
            if !is_ordinary_poison(lexeme.kind())
                || self.gap_has_line_break(end, lexeme.span().start())?
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
