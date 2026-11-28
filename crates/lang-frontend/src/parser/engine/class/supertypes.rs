use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DelegationLambdaTailEvent {
    Trivia,
    SameLineClosedCall,
    LineBreakLeftParen,
    UnclosedLeftParen,
    OuterHeaderDelimiter,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DelegationLambdaTailStep {
    Continue,
    Accept,
    Reject,
}

pub(super) const fn is_direct_delegation_boundary_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Eof
            | LexemeKind::Token(TokenKind::Symbol(
                Symbol::Comma | Symbol::LeftBrace | Symbol::RightBrace
            ))
    )
}

pub(super) const fn delegation_suffix_extra_stop(invalid_target: bool, poison: bool) -> u32 {
    if invalid_target {
        DeclarationStops::DELEGATION_ERROR_TARGET
    } else if poison {
        DeclarationStops::LEADING_UNPROVED_ANGLE_COMMA
    } else {
        DeclarationStops::DELEGATION_UNSUPPORTED_SUFFIX
    }
}

pub(super) const fn supertype_deinit_suffix_decision(
    owner: ClassifierHeaderOwner,
    in_delegation_recovery: bool,
    in_unsupported_delegation_suffix: bool,
    next_line: bool,
    call_body_follows: bool,
) -> Option<bool> {
    if !matches!(owner, ClassifierHeaderOwner::Member) || !in_delegation_recovery {
        return None;
    }
    if in_unsupported_delegation_suffix || !next_line {
        return Some((!in_unsupported_delegation_suffix || next_line) && call_body_follows);
    }
    None
}

pub(super) const fn classify_delegation_lambda_token(
    kind: LexemeKind,
    has_line_break: bool,
    has_call_closer: bool,
) -> DelegationLambdaTailEvent {
    match kind {
        LexemeKind::Trivia(_) => DelegationLambdaTailEvent::Trivia,
        LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => {
            if has_line_break {
                DelegationLambdaTailEvent::LineBreakLeftParen
            } else if has_call_closer {
                DelegationLambdaTailEvent::SameLineClosedCall
            } else {
                DelegationLambdaTailEvent::UnclosedLeftParen
            }
        }
        LexemeKind::Token(TokenKind::Symbol(Symbol::Comma | Symbol::LeftBrace)) => {
            DelegationLambdaTailEvent::OuterHeaderDelimiter
        }
        _ => DelegationLambdaTailEvent::Other,
    }
}

pub(super) const fn step_delegation_lambda_tail(
    event: DelegationLambdaTailEvent,
) -> DelegationLambdaTailStep {
    match event {
        DelegationLambdaTailEvent::Trivia | DelegationLambdaTailEvent::SameLineClosedCall => {
            DelegationLambdaTailStep::Continue
        }
        DelegationLambdaTailEvent::OuterHeaderDelimiter => DelegationLambdaTailStep::Accept,
        DelegationLambdaTailEvent::LineBreakLeftParen
        | DelegationLambdaTailEvent::UnclosedLeftParen
        | DelegationLambdaTailEvent::Other => DelegationLambdaTailStep::Reject,
    }
}

#[cfg(any(test, kani))]
pub(super) fn run_delegation_lambda_tail(events: &[DelegationLambdaTailEvent]) -> bool {
    let mut index = 0usize;
    while index < events.len() {
        match step_delegation_lambda_tail(events[index]) {
            DelegationLambdaTailStep::Continue => index += 1,
            DelegationLambdaTailStep::Accept => return true,
            DelegationLambdaTailStep::Reject => return false,
        }
    }
    false
}

impl Parser<'_> {
    pub(super) fn parse_supertype_list(
        &mut self,
        kind: ClassifierKind,
        owner: ClassifierHeaderOwner,
    ) -> Result<(Option<Span>, Vec<SupertypeEntry>), ParserInternalError> {
        if !self.current_is_symbol(Symbol::Colon) {
            return Ok((None, Vec::new()));
        }
        let file_header = owner == ClassifierHeaderOwner::File;
        let colon_span = self.bump()?.span();
        let mut entries = Vec::new();
        let mut recovery_stops = DeclarationStops::EMPTY
            .with(DeclarationStops::COMMA)
            .with(DeclarationStops::LEFT_BRACE)
            .with(DeclarationStops::RIGHT_BRACE)
            .union(self.root_declaration_stops());
        let mut type_stops = TypeStops::empty()
            .with(TypeStops::COMMA)
            .with(TypeStops::CLASS_BODY)
            .with(TypeStops::RIGHT_BRACE)
            .with(TypeStops::FILE)
            .for_supertype(owner);
        if file_header {
            recovery_stops = recovery_stops.with(DeclarationStops::CLASS_FILE);
            type_stops = type_stops.with(TypeStops::CLASS_FILE);
        }
        let mut suffix_stops = recovery_stops.with(DeclarationStops::SUPERTYPE_SUFFIX);
        if owner == ClassifierHeaderOwner::Member {
            suffix_stops = suffix_stops.with(DeclarationStops::CLASS_MEMBER);
        }
        loop {
            let current = self.current()?;
            let file_boundary = file_header && self.at_class_header_file_boundary(true)?;
            if !file_boundary && self.can_start_type_ref(current) {
                let (type_ref, complete) = self.parse_type_ref_with_completion(type_stops)?;
                let type_span = self.type_span(type_ref)?;
                let mut trailing_lexical_error = None;
                let delegation = if complete && self.current_identifier_is("by")? {
                    let by_span = self.bump()?.span();
                    if !matches!(kind, ClassifierKind::Class { .. }) {
                        self.emit(
                            codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                            "unsupported class-family form",
                            by_span,
                        )?;
                    }
                    let (raw, current) = self
                        .significant(0)
                        .ok_or(ParserInternalError::InvalidLexemeStream)?;
                    let target_boundary = self.at_delegation_boundary(raw, current, owner, true)?;
                    let mut target = if self.current_is_identifier() && !target_boundary {
                        NameMarker::Present(self.bump()?.span())
                    } else if matches!(current.kind(), LexemeKind::Token(TokenKind::StringStart)) {
                        self.recover_string_delegation_target(current.span())?
                    } else {
                        let primary = if target_boundary {
                            self.empty_at(current.span().start())?
                        } else {
                            current.span()
                        };
                        if !self.is_poison_kind(current.kind()) {
                            self.emit(
                                codes::EXPECTED_DELEGATION_TARGET,
                                "expected delegation target",
                                primary,
                            )?;
                        }
                        if target_boundary {
                            NameMarker::Missing(primary)
                        } else if let Some(group) = self.consume_closed_supertype_group()? {
                            NameMarker::Error(group)
                        } else if self.current_is_symbol(Symbol::Less) {
                            // Keep the opener available to the error target's suffix scan.
                            NameMarker::Error(primary)
                        } else {
                            self.bump()?;
                            NameMarker::Error(primary)
                        }
                    };
                    let (raw, current) = self
                        .significant(0)
                        .ok_or(ParserInternalError::InvalidLexemeStream)?;
                    let lambda_suffix = matches!(target, NameMarker::Present(_))
                        && matches!(
                            current.kind(),
                            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
                        )
                        && !self.gap_has_line_break(
                            self.previous_significant_end(),
                            current.span().start(),
                        )?
                        && self.delegation_lambda_closer(raw)?.is_some();
                    if lambda_suffix || !self.at_delegation_boundary(raw, current, owner, false)? {
                        let poison = self.is_supertype_suffix_poison(raw, current);
                        let invalid_target = matches!(target, NameMarker::Error(_));
                        let recover_suffix = poison || invalid_target;
                        if poison {
                            self.consume_poison_with_closed_type_arguments(true)?;
                        } else if !invalid_target {
                            self.emit(
                                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                                "unsupported class-family form",
                                current.span(),
                            )?;
                        }
                        let _ = self.consume_closed_supertype_group()?;
                        let stops =
                            suffix_stops.with(delegation_suffix_extra_stop(invalid_target, poison));
                        self.recover_declaration_region(stops)?;
                        if recover_suffix {
                            let end = self.previous_significant_end();
                            match target {
                                NameMarker::Error(span) => {
                                    target = NameMarker::Error(self.span(span.start(), end)?);
                                }
                                NameMarker::Present(_) => {
                                    trailing_lexical_error =
                                        Some(self.span(current.span().start(), end)?);
                                }
                                NameMarker::Missing(_) => {}
                            }
                        }
                    }
                    let target_span = marker_span(target);
                    let delegation = DelegationClause {
                        span: self.span(
                            by_span.start(),
                            if target_span.is_empty() {
                                by_span.end()
                            } else {
                                target_span.end().max(by_span.end())
                            },
                        )?,
                        by_span,
                        target,
                    };
                    Some(delegation)
                } else {
                    None
                };
                if delegation.is_none() && complete {
                    let (raw, current) = self
                        .significant(0)
                        .ok_or(ParserInternalError::InvalidLexemeStream)?;
                    if self.is_supertype_suffix_poison(raw, current) {
                        let start = current.span().start();
                        let end = self.recover_declaration_region(suffix_stops)?;
                        trailing_lexical_error = Some(self.span(start, end)?);
                    }
                }
                entries.push(SupertypeEntry {
                    span: self.span(
                        type_span.start(),
                        trailing_lexical_error
                            .or_else(|| delegation.map(|clause| clause.span))
                            .map(Span::end)
                            .unwrap_or(type_span.end()),
                    )?,
                    type_ref,
                    delegation,
                    trailing_lexical_error,
                });
                if self.current_is_symbol(Symbol::LeftParen) {
                    let primary = self.current()?.span();
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        primary,
                    )?;
                    let _ = self.consume_closed_supertype_group()?;
                    self.recover_declaration_region(recovery_stops)?;
                }
            } else {
                let primary = if file_boundary
                    || self.current_is_symbol(Symbol::LeftBrace)
                    || self.current_is_symbol(Symbol::RightBrace)
                    || matches!(current.kind(), LexemeKind::Eof)
                    || self.is_file_declaration_boundary(current)
                {
                    self.empty_at(current.span().start())?
                } else {
                    current.span()
                };
                let poison = self.is_poison_kind(current.kind());
                if !poison {
                    self.emit(codes::EXPECTED_SUPERTYPE, "expected supertype", primary)?;
                }
                if !primary.is_empty() {
                    if poison {
                        self.consume_poison_with_closed_type_arguments(false)?;
                    }
                    self.recover_declaration_region(recovery_stops)?;
                    if poison {
                        let span = self.span(primary.start(), self.previous_significant_end())?;
                        entries.push(SupertypeEntry {
                            span,
                            type_ref: self.add_type_ref(span, TypeRef::Error)?,
                            delegation: None,
                            trailing_lexical_error: None,
                        });
                    }
                }
            }
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                if self.current_is_symbol(Symbol::LeftBrace)
                    || matches!(self.current()?.kind(), LexemeKind::Eof)
                {
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        comma,
                    )?;
                    break;
                }
                continue;
            }
            break;
        }
        Ok((Some(colon_span), entries))
    }

    fn at_delegation_boundary(
        &self,
        raw: usize,
        current: Lexeme,
        owner: ClassifierHeaderOwner,
        before_target: bool,
    ) -> Result<bool, ParserInternalError> {
        if is_direct_delegation_boundary_kind(current.kind())
            || self.is_file_declaration_boundary(current)
        {
            return Ok(true);
        }
        match owner {
            ClassifierHeaderOwner::File => self.at_class_header_file_boundary(before_target),
            ClassifierHeaderOwner::Member => {
                // A soft identifier can still be a delegate name across a newline.
                // Hand it to the enclosing member only for a recognized prefix.
                Ok(self.current_is_symbol(Symbol::Semicolon)
                    || (self.at_supertype_suffix_boundary(
                        raw,
                        current,
                        owner,
                        DeclarationStops::EMPTY
                            .with(DeclarationStops::DELEGATION_UNSUPPORTED_SUFFIX),
                    )? && self.gap_has_line_break(
                        self.previous_significant_end(),
                        current.span().start(),
                    )?))
            }
            ClassifierHeaderOwner::Declaration => Ok(false),
        }
    }

    pub(in crate::parser::engine) fn at_supertype_suffix_boundary(
        &self,
        raw: usize,
        current: Lexeme,
        owner: ClassifierHeaderOwner,
        stops: DeclarationStops,
    ) -> Result<bool, ParserInternalError> {
        let strict_member =
            owner == ClassifierHeaderOwner::Member && stops.in_delegation_recovery();
        if strict_member && self.current_identifier_is("deinit")? {
            let next_line =
                self.gap_has_line_break(self.previous_significant_end(), current.span().start())?;
            let named_suffix = stops.in_unsupported_delegation_suffix();
            let call_body_follows = (named_suffix == next_line)
                && self.peek_is_symbol(1, Symbol::LeftParen)
                && self.peek_is_symbol(2, Symbol::RightParen)
                && self.peek_is_symbol(3, Symbol::LeftBrace);
            if let Some(decision) = supertype_deinit_suffix_decision(
                owner,
                stops.in_delegation_recovery(),
                named_suffix,
                next_line,
                call_body_follows,
            ) {
                // A call-shaped suffix may immediately precede the classifier body.
                return Ok(decision);
            }
        }
        self.at_supertype_recovery_boundary(raw, current, owner, strict_member)
    }

    fn is_supertype_suffix_poison(&self, raw: usize, current: Lexeme) -> bool {
        self.is_poison_kind(current.kind())
            || (matches!(current.kind(), LexemeKind::Token(TokenKind::StringStart))
                && self
                    .lexical_recoveries
                    .is_poisoned_string_start(self.lexed, raw))
    }

    fn consume_poison_with_closed_type_arguments(
        &mut self,
        include_strings: bool,
    ) -> Result<(), ParserInternalError> {
        loop {
            let (raw, current) = self
                .significant(0)
                .ok_or(ParserInternalError::InvalidLexemeStream)?;
            if self.is_poison_kind(current.kind()) {
                self.bump()?;
            } else if include_strings && self.is_supertype_suffix_poison(raw, current) {
                self.recover_declaration_string()?;
            } else {
                break;
            }
        }
        if self.current_is_symbol(Symbol::Less)
            && let Some(closer) = self.strict_trials.recovery_type_list_closer(
                self.sources,
                self.lexed,
                self.current_raw()?,
                true,
            )?
        {
            self.index = closer + 1;
        }
        Ok(())
    }

    pub(in crate::parser::engine) fn delegation_lambda_closer(
        &self,
        raw: usize,
    ) -> Result<Option<usize>, ParserInternalError> {
        if !matches!(
            self.lambda_headers.query(raw)?,
            LambdaHeaderTrial::Header { .. }
        ) {
            return Ok(None);
        }
        let Some(lambda_closer) = self.lexical_recoveries.angle_group_closer(self.lexed, raw)
        else {
            return Ok(None);
        };
        // A paired call may follow the lambda. Still require an outer header
        // delimiter before spending a brace that might close the classifier.
        let mut previous_closer = lambda_closer;
        let mut next = lambda_closer + 1;
        while let Some(lexeme) = self.lexed.lexemes().get(next) {
            #[cfg(test)]
            self.significant_raw_visits
                .set(self.significant_raw_visits.get() + 1);
            let mut has_line_break = false;
            let mut call_closer = None;
            if matches!(
                lexeme.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen))
            ) {
                has_line_break = self.gap_has_line_break(
                    self.lexed.lexemes()[previous_closer].span().end(),
                    lexeme.span().start(),
                )?;
                if !has_line_break {
                    call_closer = self.lexical_recoveries.angle_group_closer(self.lexed, next);
                }
            }
            let event = classify_delegation_lambda_token(
                lexeme.kind(),
                has_line_break,
                call_closer.is_some(),
            );
            match step_delegation_lambda_tail(event) {
                DelegationLambdaTailStep::Continue => match event {
                    DelegationLambdaTailEvent::Trivia => next += 1,
                    DelegationLambdaTailEvent::SameLineClosedCall => {
                        let Some(closer) = call_closer else {
                            break;
                        };
                        previous_closer = closer;
                        next = closer + 1;
                    }
                    _ => break,
                },
                DelegationLambdaTailStep::Accept => return Ok(Some(lambda_closer)),
                DelegationLambdaTailStep::Reject => break,
            }
        }
        Ok(None)
    }

    fn consume_closed_supertype_group(&mut self) -> Result<Option<Span>, ParserInternalError> {
        let (raw, opener) = self
            .significant(0)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        if !matches!(
            opener.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket))
        ) {
            return Ok(None);
        }
        let Some(closer) = self.lexical_recoveries.group_closer(self.lexed, raw) else {
            return Ok(None);
        };
        self.index = closer + 1;
        Ok(Some(self.span(
            opener.span().start(),
            self.lexed.lexemes()[closer].span().end(),
        )?))
    }

    fn recover_string_delegation_target(
        &mut self,
        opener: Span,
    ) -> Result<NameMarker, ParserInternalError> {
        let (end, string_poison) = self.recover_declaration_string()?;
        if !string_poison {
            self.emit(
                codes::EXPECTED_DELEGATION_TARGET,
                "expected delegation target",
                opener,
            )?;
        }
        Ok(NameMarker::Error(self.span(opener.start(), end)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_delegation_boundary_matches_expected_kinds() {
        assert!(is_direct_delegation_boundary_kind(LexemeKind::Eof));
        assert!(is_direct_delegation_boundary_kind(LexemeKind::Token(
            TokenKind::Symbol(Symbol::Comma)
        )));
        assert!(is_direct_delegation_boundary_kind(LexemeKind::Token(
            TokenKind::Symbol(Symbol::LeftBrace)
        )));
        assert!(is_direct_delegation_boundary_kind(LexemeKind::Token(
            TokenKind::Symbol(Symbol::RightBrace)
        )));
        assert!(!is_direct_delegation_boundary_kind(LexemeKind::Token(
            TokenKind::Symbol(Symbol::Semicolon)
        )));
        assert!(!is_direct_delegation_boundary_kind(LexemeKind::Token(
            TokenKind::Identifier
        )));
    }

    #[test]
    fn delegation_suffix_extra_stop_prioritizes_invalid_target_then_poison() {
        assert_eq!(
            delegation_suffix_extra_stop(true, true),
            DeclarationStops::DELEGATION_ERROR_TARGET
        );
        assert_eq!(
            delegation_suffix_extra_stop(true, false),
            DeclarationStops::DELEGATION_ERROR_TARGET
        );
        assert_eq!(
            delegation_suffix_extra_stop(false, true),
            DeclarationStops::LEADING_UNPROVED_ANGLE_COMMA
        );
        assert_eq!(
            delegation_suffix_extra_stop(false, false),
            DeclarationStops::DELEGATION_UNSUPPORTED_SUFFIX
        );
    }

    #[test]
    fn supertype_deinit_suffix_decision_matches_member_recovery_matrix() {
        assert_eq!(
            supertype_deinit_suffix_decision(ClassifierHeaderOwner::File, true, true, true, true),
            None
        );
        assert_eq!(
            supertype_deinit_suffix_decision(
                ClassifierHeaderOwner::Member,
                false,
                true,
                true,
                true
            ),
            None
        );
        assert_eq!(
            supertype_deinit_suffix_decision(
                ClassifierHeaderOwner::Member,
                true,
                true,
                false,
                true
            ),
            Some(false)
        );
        assert_eq!(
            supertype_deinit_suffix_decision(ClassifierHeaderOwner::Member, true, true, true, true),
            Some(true)
        );
        assert_eq!(
            supertype_deinit_suffix_decision(
                ClassifierHeaderOwner::Member,
                true,
                false,
                false,
                true
            ),
            Some(true)
        );
        assert_eq!(
            supertype_deinit_suffix_decision(
                ClassifierHeaderOwner::Member,
                true,
                false,
                true,
                true
            ),
            None
        );
    }

    #[test]
    fn delegation_lambda_tail_requires_outer_delimiter_without_line_break_or_unclosed_call() {
        assert!(run_delegation_lambda_tail(&[
            DelegationLambdaTailEvent::Trivia,
            DelegationLambdaTailEvent::SameLineClosedCall,
            DelegationLambdaTailEvent::OuterHeaderDelimiter,
        ]));
        assert!(!run_delegation_lambda_tail(&[
            DelegationLambdaTailEvent::LineBreakLeftParen,
            DelegationLambdaTailEvent::OuterHeaderDelimiter,
        ]));
        assert!(!run_delegation_lambda_tail(&[
            DelegationLambdaTailEvent::UnclosedLeftParen,
            DelegationLambdaTailEvent::OuterHeaderDelimiter,
        ]));
        assert!(!run_delegation_lambda_tail(&[
            DelegationLambdaTailEvent::Trivia,
            DelegationLambdaTailEvent::SameLineClosedCall,
        ]));
    }
}
