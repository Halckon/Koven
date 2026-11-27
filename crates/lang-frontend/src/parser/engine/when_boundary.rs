//! Read-only evidence for a line-leading group that starts a new when selector.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum WhenEntryBody {
    WithSubject,
    WithoutSubject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SelectorScanState {
    pub(super) needs_operand: bool,
    pub(super) pending_else: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectorEvent {
    Arrow,
    Else,
    NewSelectorAfterLineBreak,
    NonOperandContinuation,
    NonOperandExpectsOperand,
    NonOperandGroupFallthrough,
    OperandIf,
    OperandPrefixContinue,
    OperandComplete,
    Reject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectorStep {
    Continue(SelectorScanState),
    Finish(bool),
}

pub(super) const fn is_new_selector_after_line_break(body: WhenEntryBody, kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBrace | Symbol::Plus | Symbol::Minus)
    ) || (matches!(body, WhenEntryBody::WithSubject)
        && matches!(
            kind,
            TokenKind::Keyword(Keyword::Is | Keyword::In)
                | TokenKind::Symbol(Symbol::BangIs | Symbol::BangIn)
        ))
}

impl SelectorScanState {
    pub(super) const INITIAL: Self = Self {
        needs_operand: false,
        pending_else: 0,
    };

    pub(super) const fn step(self, event: SelectorEvent) -> SelectorStep {
        if !self.needs_operand {
            match event {
                SelectorEvent::Arrow => SelectorStep::Finish(self.pending_else == 0),
                SelectorEvent::Else => {
                    if let Some(next) = self.pending_else.checked_sub(1) {
                        SelectorStep::Continue(Self {
                            needs_operand: true,
                            pending_else: next,
                        })
                    } else {
                        SelectorStep::Finish(false)
                    }
                }
                SelectorEvent::NewSelectorAfterLineBreak => SelectorStep::Finish(false),
                SelectorEvent::NonOperandContinuation
                | SelectorEvent::NonOperandGroupFallthrough => SelectorStep::Continue(self),
                SelectorEvent::NonOperandExpectsOperand => SelectorStep::Continue(Self {
                    needs_operand: true,
                    pending_else: self.pending_else,
                }),
                SelectorEvent::OperandIf
                | SelectorEvent::OperandPrefixContinue
                | SelectorEvent::OperandComplete
                | SelectorEvent::Reject => SelectorStep::Finish(false),
            }
        } else {
            match event {
                SelectorEvent::OperandIf => {
                    if let Some(next) = self.pending_else.checked_add(1) {
                        SelectorStep::Continue(Self {
                            needs_operand: true,
                            pending_else: next,
                        })
                    } else {
                        SelectorStep::Finish(false)
                    }
                }
                SelectorEvent::OperandPrefixContinue => SelectorStep::Continue(self),
                SelectorEvent::OperandComplete => SelectorStep::Continue(Self {
                    needs_operand: false,
                    pending_else: self.pending_else,
                }),
                SelectorEvent::Arrow
                | SelectorEvent::Else
                | SelectorEvent::NewSelectorAfterLineBreak
                | SelectorEvent::NonOperandContinuation
                | SelectorEvent::NonOperandExpectsOperand
                | SelectorEvent::NonOperandGroupFallthrough
                | SelectorEvent::Reject => SelectorStep::Finish(false),
            }
        }
    }

    pub(super) const fn step_non_operand_control(
        self,
        body: WhenEntryBody,
        kind: TokenKind,
        has_line_break: bool,
    ) -> Option<SelectorStep> {
        if self.needs_operand {
            return None;
        }
        if matches!(kind, TokenKind::Symbol(Symbol::Arrow)) {
            return Some(self.step(SelectorEvent::Arrow));
        }
        if self.pending_else > 0 && matches!(kind, TokenKind::Keyword(Keyword::Else)) {
            return Some(self.step(SelectorEvent::Else));
        }
        if is_new_selector_after_line_break(body, kind) && has_line_break {
            return Some(self.step(SelectorEvent::NewSelectorAfterLineBreak));
        }
        None
    }
}

#[cfg(any(test, kani))]
pub(super) fn run_selector_events(events: &[SelectorEvent]) -> bool {
    let mut state = SelectorScanState::INITIAL;
    let mut index = 0;
    while index < events.len() {
        match state.step(events[index]) {
            SelectorStep::Continue(next) => state = next,
            SelectorStep::Finish(accepted) => return accepted,
        }
        index += 1;
    }
    false
}

impl Stops {
    pub(super) const WHEN_CALL_OPERAND_RECOVERY: u16 = 1 << 11;
    pub(super) const BODY_SOFT_STOPS: u16 = Self::LAMBDA_COMMA
        | Self::ARROW
        | Self::FILE_DECLARATION
        | Self::SEMICOLON
        | Self::WHEN_CALL_OPERAND_RECOVERY;
}

impl Parser<'_> {
    pub(super) fn at_when_operand_boundary(
        &self,
        stops: Stops,
    ) -> Result<bool, ParserInternalError> {
        if self.at_committed_when_arm_boundary(stops)? {
            return Ok(true);
        }
        self.unclosed_when_operand_boundary(stops)
    }

    pub(super) fn commit_when_arm_boundary(
        &self,
        body: WhenEntryBody,
    ) -> Result<(), ParserInternalError> {
        self.lexical_recoveries
            .committed_when_arm
            .set(Some((self.current_raw()?, body)));
        Ok(())
    }

    pub(super) fn at_committed_when_arm_boundary(
        &self,
        stops: Stops,
    ) -> Result<bool, ParserInternalError> {
        self.when_arm_is_committed(stops.when_entry_body)
    }

    pub(super) fn when_arm_is_committed(
        &self,
        when_body: Option<WhenEntryBody>,
    ) -> Result<bool, ParserInternalError> {
        if let Some((raw, body)) = self.lexical_recoveries.committed_when_arm.get()
            && self.index <= raw
            && when_body == Some(body)
        {
            return Ok(self.current_raw()? == raw);
        }
        Ok(false)
    }

    pub(super) fn unclosed_when_operand_boundary(
        &self,
        stops: Stops,
    ) -> Result<bool, ParserInternalError> {
        if stops.delimiters & Stops::WHEN_CALL_OPERAND_RECOVERY != 0
            && let Some(body) = stops.when_entry_body
        {
            return self.when_arm_boundary_follows(self.previous_significant_end(), body);
        }
        Ok(false)
    }

    pub(super) fn strict_when_type_has_local_end(&self) -> Result<bool, ParserInternalError> {
        let Some(end) = self.strict_trials.type_end(self.current_raw()?) else {
            return Ok(false);
        };
        let closer = self.lexed.lexemes()[end - 1];
        if closer.kind() == LexemeKind::Token(TokenKind::Symbol(Symbol::Greater))
            && let Some((next_raw, next)) = self.selector_token(end)
            && self.can_start_expression(next)
            && !self.gap_has_line_break(closer.span().end(), next.span().start())?
        {
            // A soft infix name can continue the completed type only when it
            // has a right operand. Otherwise the selector may use it as a name.
            return Ok(self.infix_rule(next)?.is_some()
                && self
                    .selector_token(next_raw + 1)
                    .is_some_and(|(_, right)| self.can_start_expression(right)));
        }
        Ok(true)
    }

    pub(super) fn grouped_when_entry_follows(
        &self,
        receiver: ExpressionId,
        body: WhenEntryBody,
    ) -> Result<Option<bool>, ParserInternalError> {
        if !self.current_is_symbol(Symbol::LeftParen)
            || !self.gap_has_line_break(
                self.expression_span(receiver)?.end(),
                self.current()?.span().start(),
            )?
        {
            return Ok(None);
        }
        let Some(closer) = self
            .lexical_recoveries
            .group_closer(self.lexed, self.current_raw()?)
        else {
            return Ok(None);
        };
        self.selector_arrow_after_group(closer, body).map(Some)
    }

    pub(super) fn brace_when_entry_follows(
        &self,
        previous_end: usize,
        body: WhenEntryBody,
    ) -> Result<bool, ParserInternalError> {
        if !self.current_is_symbol(Symbol::LeftBrace)
            || !self.gap_has_line_break(previous_end, self.current()?.span().start())?
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
        self.selector_arrow(
            closer + 1,
            self.lexed.lexemes()[closer].span().end(),
            false,
            body,
            None,
        )
    }

    /// Only called when the current call has no paired closer.
    pub(super) fn when_arm_boundary_follows(
        &self,
        previous_end: usize,
        body: WhenEntryBody,
    ) -> Result<bool, ParserInternalError> {
        if self.current_is_symbol(Symbol::Semicolon) {
            return Ok(true);
        }
        let current = self.current()?;
        let raw = self.current_raw()?;
        let is_else = self.current_is_keyword(Keyword::Else);
        let boundaries = &self.lexical_recoveries.when_arm_boundaries;
        let cached = boundaries.borrow().get(&(raw, body)).copied();
        if cached == Some(false) {
            return Ok(false);
        }
        if !is_else && !self.gap_has_line_break(previous_end, current.span().start())? {
            return Ok(false);
        }
        if let Some(result) = cached {
            return Ok(result);
        }
        let mut starts = Vec::new();
        let boundary = if is_else {
            self.selector_token(raw + 1).is_some_and(|(_, next)| {
                next.kind() == LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow))
            })
        } else {
            self.selector_arrow(raw, current.span().start(), true, body, Some(&mut starts))?
        };
        let mut boundaries = boundaries.borrow_mut();
        boundaries.insert((raw, body), boundary);
        boundaries.extend(
            starts
                .into_iter()
                .map(|(start, pending)| ((start, body), boundary && pending == 0)),
        );
        Ok(boundary)
    }

    fn selector_arrow(
        &self,
        mut raw: usize,
        mut end: usize,
        needs_operand: bool,
        body: WhenEntryBody,
        mut recovery_starts: Option<&mut Vec<(usize, usize)>>,
    ) -> Result<bool, ParserInternalError> {
        let lexemes = self.lexed.lexemes();
        let mut state = SelectorScanState {
            needs_operand,
            ..SelectorScanState::INITIAL
        };
        while let Some((opener, lexeme)) = self.selector_token(raw) {
            raw = opener + 1;
            let LexemeKind::Token(kind) = lexeme.kind() else {
                return Ok(false);
            };
            if state.needs_operand
                && !matches!(
                    kind,
                    TokenKind::Keyword(Keyword::Else) | TokenKind::Symbol(Symbol::Semicolon)
                )
                && let Some(starts) = recovery_starts.as_deref_mut()
            {
                // A fresh operand does not depend on the previous operand's span.
                if state.pending_else == 0
                    && let Some(&boundary) = self
                        .lexical_recoveries
                        .when_arm_boundaries
                        .borrow()
                        .get(&(opener, body))
                {
                    return Ok(boundary);
                }
                starts.push((opener, state.pending_else));
            }
            if !state.needs_operand {
                // A line-leading group or lambda may start its own entry.
                // Only a when with a subject can have a bare is/in selector.
                let has_line_break = is_new_selector_after_line_break(body, kind)
                    && self.gap_has_line_break(end, lexeme.span().start())?;
                if kind == TokenKind::Symbol(Symbol::Arrow)
                    && state.pending_else > 0
                    && let Some(starts) = recovery_starts.as_deref_mut()
                {
                    starts.retain(|&(_, debt)| debt < state.pending_else);
                }
                if let Some(step) = state.step_non_operand_control(body, kind, has_line_break) {
                    match step {
                        SelectorStep::Continue(next) => {
                            state = next;
                            continue;
                        }
                        SelectorStep::Finish(accepted) => return Ok(accepted),
                    }
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
                    let SelectorStep::Continue(next) =
                        state.step(SelectorEvent::NonOperandContinuation)
                    else {
                        return Ok(false);
                    };
                    state = next;
                    continue;
                }
                if let Some(rule) = self.infix_rule(lexeme)? {
                    let event =
                        if matches!(rule.kind, InfixKind::Cast(_) | InfixKind::TypeTest { .. }) {
                            let Some(cursor) = self.strict_trials.type_end(raw) else {
                                return Ok(false);
                            };
                            end = lexemes[cursor - 1].span().end();
                            raw = cursor;
                            SelectorEvent::NonOperandContinuation
                        } else {
                            SelectorEvent::NonOperandExpectsOperand
                        };
                    let SelectorStep::Continue(next) = state.step(event) else {
                        return Ok(false);
                    };
                    state = next;
                    continue;
                }
                if matches!(
                    kind,
                    TokenKind::Symbol(
                        Symbol::Comma | Symbol::Dot | Symbol::QuestionDot | Symbol::ColonColon
                    )
                ) {
                    if kind == TokenKind::Symbol(Symbol::Comma)
                        && state.pending_else > 0
                        && let Some(starts) = recovery_starts.as_deref_mut()
                    {
                        // A selector cannot carry its unfinished if across the comma.
                        // A probe starting with less debt might reach a complete selector.
                        starts.retain(|&(_, debt)| debt < state.pending_else);
                        return Ok(false);
                    }
                    let SelectorStep::Continue(next) =
                        state.step(SelectorEvent::NonOperandExpectsOperand)
                    else {
                        return Ok(false);
                    };
                    state = next;
                    continue;
                }
                if matches!(kind, TokenKind::Symbol(Symbol::Question | Symbol::BangBang)) {
                    end = lexeme.span().end();
                    let SelectorStep::Continue(next) =
                        state.step(SelectorEvent::NonOperandContinuation)
                    else {
                        return Ok(false);
                    };
                    state = next;
                    continue;
                }
                if !matches!(
                    kind,
                    TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket | Symbol::LeftBrace)
                ) {
                    return Ok(false);
                }
            }

            let operand_event = match kind {
                TokenKind::Keyword(Keyword::Return) => {
                    if let Some((_, value)) = self.selector_token(raw)
                        && self.can_start_expression(value)
                        && !self.gap_has_line_break(lexeme.span().end(), value.span().start())?
                    {
                        let SelectorStep::Continue(next) =
                            state.step(SelectorEvent::OperandPrefixContinue)
                        else {
                            return Ok(false);
                        };
                        state = next;
                        continue;
                    }
                    SelectorEvent::OperandComplete
                }
                TokenKind::Keyword(Keyword::Super) => {
                    let Some(cursor) = self.selector_super_member(raw) else {
                        return Ok(false);
                    };
                    raw = cursor;
                    SelectorEvent::OperandComplete
                }
                TokenKind::Keyword(Keyword::If) => {
                    let Some(closer) = self.selector_group_close(raw, Symbol::LeftParen) else {
                        return Ok(false);
                    };
                    raw = closer + 1;
                    let SelectorStep::Continue(next) = state.step(SelectorEvent::OperandIf) else {
                        return Ok(false);
                    };
                    state = next;
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
                    SelectorEvent::OperandComplete
                }
                TokenKind::Keyword(Keyword::Is) | TokenKind::Symbol(Symbol::BangIs) => {
                    let Some(cursor) = self.strict_trials.type_end(raw) else {
                        return Ok(false);
                    };
                    raw = cursor;
                    SelectorEvent::OperandComplete
                }
                TokenKind::Symbol(
                    Symbol::Plus | Symbol::Minus | Symbol::Bang | Symbol::ColonColon,
                )
                | TokenKind::Keyword(Keyword::In)
                | TokenKind::Symbol(Symbol::BangIn) => {
                    let SelectorStep::Continue(next) =
                        state.step(SelectorEvent::OperandPrefixContinue)
                    else {
                        return Ok(false);
                    };
                    state = next;
                    continue;
                }
                TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket | Symbol::LeftBrace) => {
                    let Some(closer) = self.lexical_recoveries.group_closer(self.lexed, opener)
                    else {
                        return Ok(false);
                    };
                    raw = closer + 1;
                    if state.needs_operand {
                        SelectorEvent::OperandComplete
                    } else {
                        SelectorEvent::NonOperandGroupFallthrough
                    }
                }
                TokenKind::StringStart => {
                    let Some(string_end) = self
                        .lexical_recoveries
                        .string_owner_end(lexeme.span().start())
                    else {
                        return Ok(false);
                    };
                    raw += lexemes[raw..].partition_point(|token| token.span().end() <= string_end);
                    SelectorEvent::OperandComplete
                }
                TokenKind::Identifier
                | TokenKind::IntegerLiteral(_)
                | TokenKind::FloatLiteral(_)
                | TokenKind::CharLiteral
                | TokenKind::Keyword(
                    Keyword::False
                    | Keyword::True
                    | Keyword::Null
                    | Keyword::This
                    | Keyword::Break
                    | Keyword::Continue,
                ) => SelectorEvent::OperandComplete,
                _ => SelectorEvent::Reject,
            };
            end = lexemes[raw - 1].span().end();
            let SelectorStep::Continue(next) = state.step(operand_event) else {
                return Ok(false);
            };
            state = next;
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
    use super::{
        SelectorEvent, SelectorScanState, SelectorStep, WhenEntryBody,
        is_new_selector_after_line_break, run_selector_events,
    };
    use crate::lexer::{Keyword, Symbol, TokenKind};

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

    #[test]
    fn nested_unclosed_calls_reuse_the_same_when_arm_boundary() {
        let metrics = |depth| {
            block_dispatch_metrics(format!(
                "{{ when {{ false -> {}1\nflag{} -> 2\nelse -> 3 }} }}",
                "take(".repeat(depth),
                " && true".repeat(128),
            ))
        };
        let (_, _, shallow_visits, shallow_diagnostics) = metrics(4);
        let (_, _, deep_visits, deep_diagnostics) = metrics(16);
        assert_eq!((shallow_diagnostics, deep_diagnostics), (4, 16));
        assert!(
            deep_visits <= shallow_visits + (16 - 4) * 96,
            "raw visits grew with selector rescanning: {shallow_visits} -> {deep_visits}"
        );
    }

    #[test]
    fn nested_unclosed_type_arguments_reuse_the_same_when_arm_boundary() {
        let metrics = |depth| {
            block_dispatch_metrics(format!(
                "{{ when {{ false -> take(value as {}\nflag{} -> 2\nelse -> 3 }} }}",
                "List<".repeat(depth),
                " && true".repeat(128),
            ))
        };
        let (_, _, shallow_visits, shallow_diagnostics) = metrics(4);
        let (_, _, deep_visits, deep_diagnostics) = metrics(16);
        assert_eq!((shallow_diagnostics, deep_diagnostics), (6, 18));
        assert!(
            deep_visits <= shallow_visits + (16 - 4) * 96,
            "raw visits grew with selector rescanning: {shallow_visits} -> {deep_visits}"
        );
    }

    #[test]
    fn multiline_unclosed_call_arguments_share_selector_suffixes() {
        for argument in ["value", "if (flag) value"] {
            let metrics = |count| {
                block_dispatch_metrics(format!(
                    "{{ when {{ false -> take({}\n{argument}\n}} }}",
                    format!("\n{argument},").repeat(count),
                ))
            };
            let (_, _, small_visits, small_diagnostics) = metrics(64);
            let (_, _, large_visits, large_diagnostics) = metrics(128);
            assert_eq!((small_diagnostics, large_diagnostics), (1, 1), "{argument}");
            assert!(
                large_visits <= small_visits * 2 + 64,
                "{argument}: raw visits grew with argument rescanning: {small_visits} -> {large_visits}"
            );
        }
    }

    #[test]
    fn damaged_call_reuses_failed_operand_suffixes() {
        for (operand, tail) in [
            ("flag", "flag"),
            ("if (flag) flag", "flag"),
            ("if (flag) flag", "flag -> 2"),
        ] {
            let metrics = |count| {
                block_dispatch_metrics(format!(
                    "{{ when {{ false -> take(:{}\n{tail}\n}} }}",
                    format!("\n{operand} &&").repeat(count),
                ))
            };
            let (_, _, small_visits, small_diagnostics) = metrics(64);
            let (_, _, large_visits, large_diagnostics) = metrics(128);
            assert_eq!(
                (small_diagnostics, large_diagnostics),
                (2, 2),
                "{operand} {tail}"
            );
            assert!(
                large_visits <= small_visits * 2 + 64,
                "{operand} {tail}: {small_visits} -> {large_visits}"
            );
        }
    }

    #[test]
    fn new_selector_after_line_break_respects_when_subject() {
        for kind in [
            TokenKind::Symbol(Symbol::LeftParen),
            TokenKind::Symbol(Symbol::LeftBrace),
            TokenKind::Symbol(Symbol::Plus),
            TokenKind::Symbol(Symbol::Minus),
        ] {
            assert!(is_new_selector_after_line_break(
                WhenEntryBody::WithSubject,
                kind
            ));
            assert!(is_new_selector_after_line_break(
                WhenEntryBody::WithoutSubject,
                kind
            ));
        }
        for kind in [
            TokenKind::Keyword(Keyword::Is),
            TokenKind::Keyword(Keyword::In),
            TokenKind::Symbol(Symbol::BangIs),
            TokenKind::Symbol(Symbol::BangIn),
        ] {
            assert!(is_new_selector_after_line_break(
                WhenEntryBody::WithSubject,
                kind
            ));
            assert!(!is_new_selector_after_line_break(
                WhenEntryBody::WithoutSubject,
                kind
            ));
        }
        assert!(!is_new_selector_after_line_break(
            WhenEntryBody::WithSubject,
            TokenKind::Symbol(Symbol::Arrow)
        ));
    }

    #[test]
    fn selector_scan_state_tracks_if_else_balance_and_arrow_acceptance() {
        assert!(run_selector_events(&[SelectorEvent::Arrow]));
        assert!(!run_selector_events(&[]));
        assert!(!run_selector_events(&[
            SelectorEvent::NonOperandExpectsOperand,
            SelectorEvent::OperandIf,
            SelectorEvent::OperandComplete,
            SelectorEvent::Arrow,
        ]));
        assert!(run_selector_events(&[
            SelectorEvent::NonOperandExpectsOperand,
            SelectorEvent::OperandIf,
            SelectorEvent::OperandComplete,
            SelectorEvent::Else,
            SelectorEvent::OperandComplete,
            SelectorEvent::Arrow,
        ]));
        assert_eq!(
            SelectorScanState {
                needs_operand: true,
                pending_else: usize::MAX,
            }
            .step(SelectorEvent::OperandIf),
            SelectorStep::Finish(false)
        );
        assert_eq!(
            SelectorScanState::INITIAL.step(SelectorEvent::Else),
            SelectorStep::Finish(false)
        );
    }
}
