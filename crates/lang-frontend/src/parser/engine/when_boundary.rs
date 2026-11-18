//! Read-only evidence for a line-leading group that starts a new when selector.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WhenEntryBody {
    WithSubject,
    WithoutSubject,
}

impl Parser<'_> {
    pub(super) fn grouped_when_entry_follows(
        &self,
        receiver: ExpressionId,
        body: WhenEntryBody,
    ) -> Result<bool, ParserInternalError> {
        if !self.current_is_symbol(Symbol::LeftParen)
            || !self.gap_has_line_break(
                self.expression_span(receiver)?.end(),
                self.current()?.span().start(),
            )?
        {
            return Ok(false);
        }
        let Some(closer) = self
            .lexical_recoveries
            .group_closer(self.lexed, self.current_raw()?)
        else {
            return Ok(false);
        };
        self.selector_arrow_after_group(closer, body)
    }

    fn selector_arrow_after_group(
        &self,
        closer: usize,
        body: WhenEntryBody,
    ) -> Result<bool, ParserInternalError> {
        let lexemes = self.lexed.lexemes();
        let mut end = lexemes[closer].span().end();
        let mut raw = closer + 1;
        let mut needs_operand = false;
        let mut pending_else = 0;
        while let Some((opener, lexeme)) = self.selector_token(raw) {
            raw = opener + 1;
            let LexemeKind::Token(kind) = lexeme.kind() else {
                return Ok(false);
            };
            if !needs_operand {
                if kind == TokenKind::Symbol(Symbol::Arrow) {
                    return Ok(pending_else == 0);
                }
                if pending_else > 0 && kind == TokenKind::Keyword(Keyword::Else) {
                    pending_else -= 1;
                    needs_operand = true;
                    continue;
                }
                // A line-leading group or lambda may start its own entry.
                // Only a when with a subject can have a bare is/in selector.
                let new_selector = matches!(
                    kind,
                    TokenKind::Symbol(
                        Symbol::LeftParen | Symbol::LeftBrace | Symbol::Plus | Symbol::Minus
                    )
                ) || (body == WhenEntryBody::WithSubject
                    && matches!(
                        kind,
                        TokenKind::Keyword(Keyword::Is | Keyword::In)
                            | TokenKind::Symbol(Symbol::BangIs | Symbol::BangIn)
                    ));
                if new_selector && self.gap_has_line_break(end, lexeme.span().start())? {
                    return Ok(false);
                }
                if kind == TokenKind::Symbol(Symbol::Less)
                    && let Some(CallTrial::Match {
                        closing_raw,
                        suffix_raw,
                        ..
                    }) = self.strict_trials.trial_at_raw(opener)?
                    && self.typed_call_suffix_continues(closing_raw, suffix_raw)?
                {
                    let Some(closer) = self.lexical_recoveries.group_closer(self.lexed, suffix_raw)
                    else {
                        return Ok(false);
                    };
                    end = lexemes[closer].span().end();
                    raw = closer + 1;
                    continue;
                }
                if let Some(rule) = self.infix_rule(lexeme)? {
                    if matches!(rule.kind, InfixKind::Cast(_) | InfixKind::TypeTest { .. }) {
                        let Some(cursor) = self.strict_trials.type_end(raw) else {
                            return Ok(false);
                        };
                        end = lexemes[cursor - 1].span().end();
                        raw = cursor;
                    } else {
                        needs_operand = true;
                    }
                    continue;
                }
                if matches!(
                    kind,
                    TokenKind::Symbol(
                        Symbol::Comma | Symbol::Dot | Symbol::QuestionDot | Symbol::ColonColon
                    )
                ) {
                    needs_operand = true;
                    continue;
                }
                if matches!(kind, TokenKind::Symbol(Symbol::Question | Symbol::BangBang)) {
                    end = lexeme.span().end();
                    continue;
                }
                if !matches!(
                    kind,
                    TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket | Symbol::LeftBrace)
                ) {
                    return Ok(false);
                }
            }

            match kind {
                TokenKind::Keyword(Keyword::Return) => {
                    if let Some((_, value)) = self.selector_token(raw)
                        && self.can_start_expression(value)
                        && !self.gap_has_line_break(lexeme.span().end(), value.span().start())?
                    {
                        continue;
                    }
                }
                TokenKind::Keyword(Keyword::Super) => {
                    let Some(cursor) = self.selector_super_member(raw) else {
                        return Ok(false);
                    };
                    raw = cursor;
                }
                TokenKind::Keyword(Keyword::If) => {
                    let Some(closer) = self.selector_group_close(raw, Symbol::LeftParen) else {
                        return Ok(false);
                    };
                    raw = closer + 1;
                    pending_else += 1;
                    continue;
                }
                TokenKind::Keyword(Keyword::When) => {
                    if let Some(closer) = self.selector_group_close(raw, Symbol::LeftParen) {
                        raw = closer + 1;
                    }
                    let Some(closer) = self.selector_group_close(raw, Symbol::LeftBrace) else {
                        return Ok(false);
                    };
                    raw = closer + 1;
                }
                TokenKind::Keyword(Keyword::Is) | TokenKind::Symbol(Symbol::BangIs) => {
                    let Some(cursor) = self.strict_trials.type_end(raw) else {
                        return Ok(false);
                    };
                    raw = cursor;
                }
                TokenKind::Symbol(
                    Symbol::Plus | Symbol::Minus | Symbol::Bang | Symbol::ColonColon,
                )
                | TokenKind::Keyword(Keyword::In)
                | TokenKind::Symbol(Symbol::BangIn) => continue,
                TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket | Symbol::LeftBrace) => {
                    let Some(closer) = self.lexical_recoveries.group_closer(self.lexed, opener)
                    else {
                        return Ok(false);
                    };
                    raw = closer + 1;
                }
                TokenKind::StringStart => {
                    let Some(string_end) = self
                        .lexical_recoveries
                        .string_owner_end(lexeme.span().start())
                    else {
                        return Ok(false);
                    };
                    raw += lexemes[raw..].partition_point(|token| token.span().end() <= string_end);
                }
                TokenKind::Identifier
                | TokenKind::IntegerLiteral(_)
                | TokenKind::FloatLiteral(_)
                | TokenKind::CharLiteral
                | TokenKind::Keyword(
                    Keyword::False | Keyword::True | Keyword::Null | Keyword::This,
                ) => {}
                _ => return Ok(false),
            }
            end = lexemes[raw - 1].span().end();
            needs_operand = false;
        }
        Ok(false)
    }
    fn selector_super_member(&self, raw: usize) -> Option<usize> {
        let (less, opener) = self.selector_token(raw)?;
        if opener.kind() != LexemeKind::Token(TokenKind::Symbol(Symbol::Less)) {
            return None;
        }
        let type_end = self.strict_trials.type_end(less + 1)?;
        let (greater, closer) = self.selector_token(type_end)?;
        let (dot, separator) = self.selector_token(greater + 1)?;
        let (name, member) = self.selector_token(dot + 1)?;
        (closer.kind() == LexemeKind::Token(TokenKind::Symbol(Symbol::Greater))
            && separator.kind() == LexemeKind::Token(TokenKind::Symbol(Symbol::Dot))
            && member.kind() == LexemeKind::Token(TokenKind::Identifier))
        .then_some(name + 1)
    }

    fn selector_token(&self, raw: usize) -> Option<(usize, Lexeme)> {
        for (offset, &lexeme) in self.lexed.lexemes().get(raw..)?.iter().enumerate() {
            #[cfg(test)]
            self.significant_raw_visits
                .set(self.significant_raw_visits.get() + 1);
            if !matches!(lexeme.kind(), LexemeKind::Trivia(_)) {
                return Some((raw + offset, lexeme));
            }
        }
        None
    }

    fn selector_group_close(&self, raw: usize, symbol: Symbol) -> Option<usize> {
        let (opener, lexeme) = self.selector_token(raw)?;
        if lexeme.kind() != LexemeKind::Token(TokenKind::Symbol(symbol)) {
            return None;
        }
        self.lexical_recoveries.group_closer(self.lexed, opener)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::block_dispatch_metrics;

    #[test]
    fn grouped_when_selector_work_scales_with_calls_and_condition_length() {
        let metrics = |repetitions| {
            block_dispatch_metrics(format!(
                "{{ when {{ false -> produce{}\n((true)){} -> 2\nelse -> 3 }} }}",
                "\n(1)".repeat(repetitions),
                " && true".repeat(repetitions),
            ))
        };
        let (small_raw, _, small_visits, small_diagnostics) = metrics(64);
        let (large_raw, _, large_visits, large_diagnostics) = metrics(128);
        assert_eq!((small_diagnostics, large_diagnostics), (0, 0));
        assert!(small_visits <= small_raw * 36);
        assert!(large_visits <= large_raw * 36);
        assert!(large_visits <= small_visits * 2 + 32);
    }
}
