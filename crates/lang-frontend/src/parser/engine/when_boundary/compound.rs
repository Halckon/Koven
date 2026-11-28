//! Share same-line selector suffix scans across possible recovery boundaries.

use super::*;
use std::collections::HashMap;

pub(super) struct CompoundScan {
    first: usize,
    pending: Vec<(usize, usize)>,
    rejected: Vec<usize>,
    boundary: Option<usize>,
    collect: bool,
}

impl CompoundScan {
    pub(super) fn new(first: usize) -> Self {
        Self {
            first,
            pending: vec![(first, 0)],
            rejected: Vec::new(),
            boundary: None,
            collect: true,
        }
    }

    pub(super) fn collecting(&self) -> bool {
        self.collect
    }

    pub(super) fn add_candidate(&mut self, raw: usize, else_depth: usize) {
        self.pending.push((raw, else_depth));
    }

    pub(super) fn leave_then(&mut self, else_depth: usize) {
        if else_depth == 0 {
            return;
        }
        // This else belongs to an if before every candidate at this depth.
        while self
            .pending
            .last()
            .is_some_and(|(_, depth)| *depth >= else_depth)
        {
            if let Some((raw, _)) = self.pending.pop() {
                self.rejected.push(raw);
            }
        }
    }

    pub(super) fn leave_body_expression(&mut self) {
        // A comma can continue a selector, but not its preceding arm body.
        self.collect = false;
    }

    pub(super) fn reach_arrow(&mut self, else_depth: usize) {
        self.boundary = self
            .pending
            .last()
            .and_then(|&(raw, depth)| (depth == else_depth).then_some(raw));
    }

    pub(super) fn publish(
        self,
        body: WhenEntryBody,
        suffixes: &mut HashMap<(usize, WhenEntryBody), bool>,
    ) -> bool {
        suffixes.extend(self.rejected.into_iter().map(|raw| ((raw, body), false)));
        suffixes.extend(
            self.pending
                .into_iter()
                .map(|(raw, _)| ((raw, body), self.boundary == Some(raw))),
        );
        self.boundary == Some(self.first)
    }
}

impl Parser<'_> {
    pub(super) fn separated_group_suffix(
        &self,
        opener: usize,
        kind: TokenKind,
    ) -> Result<Option<usize>, ParserInternalError> {
        if !matches!(
            kind,
            TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBrace)
        ) {
            return Ok(None);
        }
        let Some(closer) = self.lexical_recoveries.group_closer(self.lexed, opener) else {
            return Ok(None);
        };
        let Some((_, follower)) = self.selector_token(closer + 1) else {
            return Ok(None);
        };
        let group_end = self.lexed.lexemes()[closer].span().end();
        let can_end_at_arrow = follower.kind()
            == LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow))
            || self.grouped_selector_can_continue(group_end, follower)?;
        Ok((can_end_at_arrow && self.group_can_be_selector(opener, closer)?).then_some(closer + 1))
    }

    pub(super) fn grouped_selector_can_continue(
        &self,
        group_end: usize,
        follower: Lexeme,
    ) -> Result<bool, ParserInternalError> {
        match follower.kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) => {
                Ok(!self.gap_has_line_break(group_end, follower.span().start())?)
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)) => Ok(true),
            _ => Ok(Self::is_postfix_start_token(follower) || self.infix_rule(follower)?.is_some()),
        }
    }

    pub(super) fn group_can_be_selector(
        &self,
        opener: usize,
        closer: usize,
    ) -> Result<bool, ParserInternalError> {
        let shapes = &self.lexical_recoveries.when_group_shapes;
        if let Some(&shape) = shapes.borrow().get(&opener) {
            return Ok(shape);
        }
        // Nested control bodies can all stop at the same paired selector.
        let shape = self.scan_group_selector_shape(opener, closer)?;
        shapes.borrow_mut().insert(opener, shape);
        Ok(shape)
    }

    fn scan_group_selector_shape(
        &self,
        opener: usize,
        closer: usize,
    ) -> Result<bool, ParserInternalError> {
        let lexemes = self.lexed.lexemes();
        if lexemes[opener].kind() == LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) {
            return Ok(true);
        }
        let mut raw = opener + 1;
        let mut nonempty = false;
        while let Some((index, token)) = self.selector_token(raw) {
            if index == closer {
                return Ok(nonempty);
            }
            if index > closer {
                return Ok(false);
            }
            nonempty = true;
            raw = index + 1;
            match token.kind() {
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::Comma | Symbol::Semicolon | Symbol::Arrow | Symbol::Ampersand,
                )) => {
                    return Ok(false);
                }
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::LeftParen | Symbol::LeftBracket | Symbol::LeftBrace,
                )) => {
                    let Some(end) = self.lexical_recoveries.group_closer(self.lexed, index) else {
                        return Ok(false);
                    };
                    raw = end + 1;
                }
                LexemeKind::Token(TokenKind::Keyword(Keyword::Super)) => {
                    let Some(end) = self.selector_super_member(raw) else {
                        return Ok(false);
                    };
                    raw = end;
                }
                LexemeKind::Token(TokenKind::Symbol(Symbol::Less)) => {
                    if let Some(CallTrial::Match {
                        closing_raw,
                        suffix_raw,
                        ..
                    }) = self.strict_trials.trial_at_raw(index)?
                        && self.typed_call_suffix_continues(closing_raw, suffix_raw)?
                    {
                        raw = closing_raw + 1;
                    }
                }
                LexemeKind::Token(TokenKind::StringStart) => {
                    let Some(end) = self.lexical_recoveries.string_owner_closer(index) else {
                        return Ok(false);
                    };
                    raw = end + 1;
                }
                _ if matches!(
                    self.infix_rule(token)?.map(|rule| rule.kind),
                    Some(InfixKind::Cast(_) | InfixKind::TypeTest { .. })
                ) =>
                {
                    let Some(end) = self.strict_trials.type_end(raw) else {
                        return Ok(false);
                    };
                    raw = end;
                }
                _ => {}
            }
        }
        Ok(false)
    }
}
