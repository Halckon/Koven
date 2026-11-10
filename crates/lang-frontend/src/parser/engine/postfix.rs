use super::*;

impl Parser<'_> {
    pub(super) fn parse_postfix(
        &mut self,
        mut receiver: ExpressionId,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        loop {
            if self.at_block_expression_line_break(receiver, stops)? {
                break;
            }
            if matches!(
                self.ast.expressions().get(receiver)?.payload(),
                Expression::Error
            ) && self.is_postfix_start()
            {
                receiver = self.consume_error_postfix(receiver, stops)?;
                continue;
            }
            receiver = if self.current_is_symbol(Symbol::Dot)
                || self.current_is_symbol(Symbol::QuestionDot)
            {
                self.parse_member(receiver, stops)?
            } else if self.current_is_symbol(Symbol::LeftParen) {
                self.parse_call(receiver, Vec::new(), None, stops)?
            } else if self.current_is_symbol(Symbol::Less) {
                let checkpoint = self.index;
                if let Some((type_arguments, type_arguments_span)) =
                    self.try_parse_call_type_arguments()?
                {
                    if self.current_is_symbol(Symbol::LeftParen) {
                        self.parse_call(receiver, type_arguments, Some(type_arguments_span), stops)?
                    } else if self.current_is_symbol(Symbol::LeftBrace) {
                        self.parse_trailing_lambda_call(
                            receiver,
                            type_arguments,
                            Some(type_arguments_span),
                            stops,
                        )?
                    } else {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    }
                } else {
                    self.index = checkpoint;
                    break;
                }
            } else if self.current_is_symbol(Symbol::LeftBrace)
                && !matches!(
                    self.ast.expressions().get(receiver)?.payload(),
                    Expression::Call { .. }
                )
                && self.trailing_lambda_follows(self.expression_span(receiver)?.end())?
            {
                self.parse_trailing_lambda_call(receiver, Vec::new(), None, stops)?
            } else if self.current_is_symbol(Symbol::LeftBracket) {
                self.parse_index(receiver, stops)?
            } else if matches!(
                self.peek(0).map(Lexeme::kind),
                Some(LexemeKind::Token(TokenKind::Symbol(
                    Symbol::BangBang | Symbol::Question
                )))
            ) {
                let operator = self.bump()?;
                let operator_span = operator.span();
                let receiver_span = self.expression_span(receiver)?;
                let payload = match operator.kind() {
                    LexemeKind::Token(TokenKind::Symbol(Symbol::BangBang)) => {
                        Expression::NonNullAssert {
                            operand: receiver,
                            operator_span,
                        }
                    }
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Question)) => {
                        Expression::Propagate {
                            value: receiver,
                            question_span: operator_span,
                        }
                    }
                    _ => return Err(ParserInternalError::InvalidLexemeStream),
                };
                self.add_expression(
                    self.span(receiver_span.start(), operator_span.end())?,
                    payload,
                )?
            } else if self.current_is_symbol(Symbol::ColonColon) {
                self.parse_bound_reference(receiver, stops)?
            } else {
                break;
            };
        }
        Ok(receiver)
    }

    pub(super) fn is_postfix_start(&self) -> bool {
        matches!(
            self.peek(0).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Symbol(
                Symbol::Dot
                    | Symbol::QuestionDot
                    | Symbol::LeftParen
                    | Symbol::LeftBracket
                    | Symbol::BangBang
                    | Symbol::Question
                    | Symbol::ColonColon
            )))
        )
    }

    pub(super) fn consume_error_postfix(
        &mut self,
        receiver: ExpressionId,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let receiver_span = self.expression_span(receiver)?;
        let opener_raw = self.current_raw()?;
        let first = self.bump()?;
        let mut end = first.span().end();
        match first.kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket)) => {
                let preserve_block_separator = stops.block_elements
                    || stops.delimiters & (Stops::SEMICOLON | Stops::BLOCK_STATEMENT_SEPARATOR)
                        != 0;
                if let Some(closer) = self.lexical_recoveries.postfix_recovery_closer(
                    self.lexed,
                    opener_raw,
                    opener_raw,
                    preserve_block_separator,
                ) {
                    self.index = closer + 1;
                    end = self.lexed.lexemes()[closer].span().end();
                } else {
                    self.recover_declaration_region(DeclarationStops::for_postfix_suffix(stops))?;
                    end = self.previous_significant_end();
                }
            }
            LexemeKind::Token(TokenKind::Symbol(
                Symbol::Dot | Symbol::QuestionDot | Symbol::ColonColon,
            )) if !stops.contains(self.current()?)
                && !self.current_is_symbol(Symbol::Semicolon)
                && !matches!(self.current()?.kind(), LexemeKind::Eof) =>
            {
                end = self.bump()?.span().end();
            }
            _ => {}
        }
        self.add_expression(self.span(receiver_span.start(), end)?, Expression::Error)
    }

    /// Return the consumed raw token and any ordinary group whose body still needs scanning.
    pub(super) fn proven_postfix_recovery_step(
        &self,
        raw: usize,
        symbol: Option<Symbol>,
        recovery_start: usize,
        preserve_block_semicolon: bool,
    ) -> Result<Option<(usize, Option<usize>)>, ParserInternalError> {
        let group_closer = |opener| {
            self.lexical_recoveries.postfix_recovery_closer(
                self.lexed,
                opener,
                opener,
                preserve_block_semicolon,
            )
        };
        match symbol {
            Some(Symbol::LeftParen | Symbol::LeftBracket | Symbol::LeftBrace) => {
                Ok(Some((raw, group_closer(raw))))
            }
            Some(Symbol::Less) if self.has_recovery_callee_end(recovery_start, raw) => {
                let Some(CallTrial::Match {
                    closing_raw: closing,
                    suffix_raw: suffix,
                    ..
                }) = self.strict_trials.trial_at_raw(raw)?
                else {
                    return Ok(None);
                };
                if !self.typed_call_suffix_continues(closing, suffix)? {
                    return Ok(None);
                }
                Ok(Some((closing, group_closer(suffix))))
            }
            _ => Ok(None),
        }
    }

    fn has_recovery_callee_end(&self, start: usize, angle: usize) -> bool {
        // Walk only the trivia and postfix markers immediately before this angle.
        // A marker without an operand in this recovery region proves no callee.
        let terminal = self.lexed.lexemes()[start..angle]
            .iter()
            .rev()
            .map(|lexeme| lexeme.kind())
            .find(|kind| {
                !matches!(
                    kind,
                    LexemeKind::Trivia(_)
                        | LexemeKind::Token(TokenKind::Symbol(Symbol::Question | Symbol::BangBang))
                )
            });
        matches!(
            terminal,
            Some(LexemeKind::Token(
                TokenKind::Identifier
                    | TokenKind::IntegerLiteral(_)
                    | TokenKind::FloatLiteral(_)
                    | TokenKind::CharLiteral
                    | TokenKind::StringEnd
                    | TokenKind::Keyword(
                        Keyword::True | Keyword::False | Keyword::Null | Keyword::This
                    )
                    | TokenKind::Symbol(
                        Symbol::RightParen | Symbol::RightBracket | Symbol::RightBrace
                    )
            ))
        )
    }

    /// An explicit lambda header distinguishes a local brace from a caller's block.
    pub(super) fn has_paired_lambda_header(&self, raw: usize) -> Result<bool, ParserInternalError> {
        Ok(matches!(
            self.lambda_headers.query(raw)?,
            LambdaHeaderTrial::Header { .. }
        ) && self
            .lexical_recoveries
            .group_closer(self.lexed, raw)
            .is_some())
    }

    pub(super) fn parse_member(
        &mut self,
        receiver: ExpressionId,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let operator = self.bump()?;
        let safe = matches!(
            operator.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::QuestionDot))
        );
        let operator_span = operator.span();
        if !self.current_is_identifier() {
            let current = self.current()?;
            let end = if self.is_poison() {
                self.bump()?.span().end()
            } else {
                let primary = self.boundary_span(current, stops)?;
                self.emit(
                    codes::EXPECTED_MEMBER_NAME,
                    "expected member or reference name",
                    primary,
                )?;
                operator_span.end()
            };
            let receiver_span = self.expression_span(receiver)?;
            return self.add_expression(self.span(receiver_span.start(), end)?, Expression::Error);
        }
        let name_span = self.bump()?.span();
        let receiver_span = self.expression_span(receiver)?;
        self.add_expression(
            self.span(receiver_span.start(), name_span.end())?,
            Expression::Member {
                receiver,
                operator_span,
                name_span,
                safe,
            },
        )
    }

    pub(super) fn parse_bound_reference(
        &mut self,
        receiver: ExpressionId,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let operator_span = self.bump()?.span();
        if !self.current_is_identifier() {
            let current = self.current()?;
            let end = if self.is_poison() {
                self.bump()?.span().end()
            } else {
                let primary = self.boundary_span(current, stops)?;
                self.emit(
                    codes::EXPECTED_MEMBER_NAME,
                    "expected member or reference name",
                    primary,
                )?;
                operator_span.end()
            };
            let receiver_span = self.expression_span(receiver)?;
            return self.add_expression(self.span(receiver_span.start(), end)?, Expression::Error);
        }
        let name_span = self.bump()?.span();
        let receiver_span = self.expression_span(receiver)?;
        self.add_expression(
            self.span(receiver_span.start(), name_span.end())?,
            Expression::CallableReference {
                receiver: Some(receiver),
                operator_span,
                name_span,
            },
        )
    }

    pub(super) fn parse_call(
        &mut self,
        callee: ExpressionId,
        type_arguments: Vec<TypeRefId>,
        type_arguments_span: Option<Span>,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let block_separator =
            if outer_stops.block_elements || outer_stops.delimiters & Stops::SEMICOLON != 0 {
                Stops::BLOCK_STATEMENT_SEPARATOR
            } else {
                0
            };
        let outer_stops = outer_stops
            .without_file_and_semicolon_stops()
            .with(block_separator);
        let opener = self.bump()?.span();
        let callee_span = self.expression_span(callee)?;
        let argument_stops = outer_stops
            .without_lambda_body_soft_stops()
            .with(Stops::COMMA)
            .with(Stops::RIGHT_PAREN);
        let mut arguments = Vec::new();
        let mut last_consumed_end = opener.end();

        if !self.current_is_symbol(Symbol::RightParen) {
            loop {
                if self.current_is_symbol(Symbol::Comma) {
                    let comma = self.bump()?.span();
                    last_consumed_end = last_consumed_end.max(comma.end());
                    self.emit(
                        codes::UNSUPPORTED_ARGUMENT_EMPTY_ELEMENT,
                        "unsupported argument empty element",
                        comma,
                    )?;
                    let empty = self.empty_at(comma.start())?;
                    let value = self.add_expression(empty, Expression::Error)?;
                    arguments.push(CallArgument {
                        span: empty,
                        named_prefix: None,
                        mode_marker: None,
                        value,
                    });
                    if self.current_is_symbol(Symbol::RightParen) {
                        break;
                    }
                    continue;
                }

                let (argument, consumed_separator) =
                    self.parse_call_argument(argument_stops, outer_stops)?;
                if !argument.span.is_empty() {
                    last_consumed_end = argument.span.end().max(last_consumed_end);
                }
                arguments.push(argument);
                if consumed_separator {
                    last_consumed_end = self.previous_significant_end().max(last_consumed_end);
                    if self.current_is_symbol(Symbol::RightParen) {
                        break;
                    }
                    continue;
                }
                if self.current_is_symbol(Symbol::Comma) {
                    let comma = self.bump()?.span();
                    last_consumed_end = last_consumed_end.max(comma.end());
                    if self.current_is_symbol(Symbol::RightParen) {
                        let span = self.empty_at(self.current()?.span().start())?;
                        self.emit(
                            codes::UNSUPPORTED_ARGUMENT_TRAILING_COMMA,
                            "unsupported argument trailing comma",
                            comma,
                        )?;
                        let value = self.add_expression(span, Expression::Error)?;
                        arguments.push(CallArgument {
                            span,
                            named_prefix: None,
                            mode_marker: None,
                            value,
                        });
                        break;
                    }
                    continue;
                }
                let current = self.current()?;
                if self.call_argument_boundary(current, outer_stops)? {
                    break;
                }
                if self.can_start_call_argument(current) {
                    self.emit(
                        codes::EXPECTED_ARGUMENT_SEPARATOR,
                        "expected argument separator",
                        self.empty_at(current.span().start())?,
                    )?;
                    continue;
                }
                let diagnostic_span = current.span();
                let recovery_end = self.recover_call_region(outer_stops)?;
                self.emit(
                    codes::EXPECTED_ARGUMENT_SEPARATOR,
                    "expected argument separator",
                    diagnostic_span,
                )?;
                last_consumed_end = recovery_end.max(last_consumed_end);
                if self.current_is_symbol(Symbol::Comma) {
                    last_consumed_end = self.bump()?.span().end().max(last_consumed_end);
                    if self.current_is_symbol(Symbol::RightParen) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        let (mut end, closed) = if self.current_is_symbol(Symbol::RightParen) {
            (self.bump()?.span().end(), true)
        } else {
            let current = self.current()?;
            if !self.is_poison() {
                self.emit_closing(self.boundary_span(current, outer_stops)?, opener)?;
            }
            (last_consumed_end, false)
        };
        if closed && self.trailing_lambda_follows(end)? {
            let argument = self.parse_trailing_lambda_argument(outer_stops)?;
            end = argument.span.end();
            arguments.push(argument);
        }
        self.add_expression(
            self.span(callee_span.start(), end)?,
            Expression::Call {
                callee,
                type_arguments,
                type_arguments_span,
                arguments,
            },
        )
    }

    pub(super) fn parse_trailing_lambda_call(
        &mut self,
        callee: ExpressionId,
        type_arguments: Vec<TypeRefId>,
        type_arguments_span: Option<Span>,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let callee_span = self.expression_span(callee)?;
        let argument = self.parse_trailing_lambda_argument(outer_stops)?;
        self.add_expression(
            self.span(callee_span.start(), argument.span.end())?,
            Expression::Call {
                callee,
                type_arguments,
                type_arguments_span,
                arguments: vec![argument],
            },
        )
    }

    pub(super) fn parse_trailing_lambda_argument(
        &mut self,
        outer_stops: Stops,
    ) -> Result<CallArgument, ParserInternalError> {
        let value = self.parse_lambda(None, outer_stops)?;
        let span = self.expression_span(value)?;
        Ok(CallArgument {
            span,
            named_prefix: None,
            mode_marker: None,
            value,
        })
    }

    pub(super) fn trailing_lambda_follows(
        &self,
        previous_end: usize,
    ) -> Result<bool, ParserInternalError> {
        Ok(self.current_is_symbol(Symbol::LeftBrace)
            && !self.gap_has_line_break(previous_end, self.current()?.span().start())?)
    }

    pub(super) fn parse_call_argument(
        &mut self,
        argument_stops: Stops,
        outer_stops: Stops,
    ) -> Result<(CallArgument, bool), ParserInternalError> {
        let named_prefix = if self.current_is_identifier()
            && self.peek(1).is_some_and(|lexeme| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Equal))
                )
            }) {
            let name_span = self.bump()?.span();
            let equals_span = self.bump()?.span();
            Some(NamedArgumentPrefix {
                name_span,
                equals_span,
            })
        } else {
            None
        };
        let mut first_start = named_prefix.map(|prefix| prefix.name_span.start());
        let mut last_consumed_end = named_prefix.map(|prefix| prefix.equals_span.end());

        let mode_marker = self.parse_argument_mode_marker()?;
        if let Some(marker) = mode_marker {
            let span = parameter_mode_span(marker);
            first_start.get_or_insert(span.start());
            last_consumed_end = Some(self.previous_significant_end().max(span.end()));
        }

        if mode_marker.is_some()
            && self.current_is_identifier()
            && self.peek(1).is_some_and(|lexeme| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Equal))
                )
            })
        {
            self.bump()?;
            let equals = self.bump()?.span();
            self.emit(
                codes::INVALID_ARGUMENT_MODE_ORDERING,
                "invalid argument mode ordering",
                equals,
            )?;
            last_consumed_end = Some(equals.end());
        }

        let current = self.current()?;
        let (value, consumed_separator) = if self.call_argument_boundary(current, outer_stops)? {
            let empty = self.empty_at(current.span().start())?;
            self.emit(
                codes::EXPECTED_ARGUMENT_VALUE,
                "expected argument value",
                empty,
            )?;
            let value = self.add_expression(empty, Expression::Error)?;
            let consumed_separator = self.current_is_symbol(Symbol::Comma);
            if consumed_separator {
                self.bump()?;
            }
            (value, consumed_separator)
        } else if self.can_start_expression(current) {
            (self.parse_expression_bp(0, argument_stops)?, false)
        } else {
            let error_start = current.span().start();
            let diagnostic_span = current.span();
            let poison = self.is_poison();
            let error_end = self.recover_call_region(outer_stops)?;
            let error_span = self.span(error_start, error_end.max(error_start))?;
            if !poison {
                self.emit(
                    codes::EXPECTED_ARGUMENT_VALUE,
                    "expected argument value",
                    diagnostic_span,
                )?;
            }
            let value = self.add_expression(error_span, Expression::Error)?;
            let consumed_separator = self.current_is_symbol(Symbol::Comma);
            if consumed_separator {
                self.bump()?;
            }
            (value, consumed_separator)
        };
        let value_span = self.expression_span(value)?;
        let start = first_start.unwrap_or(value_span.start());
        if !value_span.is_empty() {
            last_consumed_end = Some(last_consumed_end.unwrap_or(0).max(value_span.end()));
        }
        let end = last_consumed_end.unwrap_or(value_span.end());
        Ok((
            CallArgument {
                span: self.span(start, end)?,
                named_prefix,
                mode_marker,
                value,
            },
            consumed_separator,
        ))
    }

    pub(super) fn parse_argument_mode_marker(
        &mut self,
    ) -> Result<Option<ParameterModeMarker>, ParserInternalError> {
        if !self.current_is_symbol(Symbol::Ampersand) {
            return Ok(None);
        }
        let marker = ParameterModeMarker::Inout(self.bump()?.span());
        while self.current_is_symbol(Symbol::Ampersand) {
            let duplicate = self.bump()?.span();
            self.emit(
                codes::DUPLICATE_ARGUMENT_MODE,
                "duplicate argument mode",
                duplicate,
            )?;
        }
        Ok(Some(marker))
    }

    pub(super) fn can_start_call_argument(&self, lexeme: Lexeme) -> bool {
        self.can_start_expression(lexeme) || self.current_is_symbol(Symbol::Ampersand)
    }

    pub(super) fn call_argument_boundary(
        &self,
        lexeme: Lexeme,
        outer_stops: Stops,
    ) -> Result<bool, ParserInternalError> {
        Ok(matches!(lexeme.kind(), LexemeKind::Eof)
            || self.current_is_symbol(Symbol::Comma)
            || self.current_is_symbol(Symbol::RightParen)
            || (outer_stops.contains(lexeme)
                && !self.can_start_expression(lexeme)
                && !self.current_is_symbol(Symbol::Comma)
                && !self.current_is_symbol(Symbol::RightParen))
            || self.at_block_statement_separator(lexeme, outer_stops)?)
    }

    pub(super) fn recover_call_region(
        &mut self,
        outer_stops: Stops,
    ) -> Result<usize, ParserInternalError> {
        self.recover_declaration_region(
            DeclarationStops::from_expression_hard(outer_stops)
                .with(DeclarationStops::COMMA)
                .with(DeclarationStops::RIGHT_PAREN),
        )
    }

    /// 以只读严格识别器判断 `<...>(` / `<...>{`，成功后才使用正式 TypeRef parser 提交节点。
    pub(super) fn try_parse_call_type_arguments(
        &mut self,
    ) -> Result<Option<(Vec<TypeRefId>, Span)>, ParserInternalError> {
        let trial_start = self
            .significant(0)
            .map(|(raw, _)| raw)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        let ast_type_len = self.ast.type_refs().len();
        let diagnostic_len = self.diagnostics.len();
        let (closing_raw, suffix_raw) = match self
            .strict_trials
            .query(trial_start, self.recursion_depth)?
        {
            CallTrial::Match {
                closing_raw,
                suffix_raw,
                ..
            } => (closing_raw, suffix_raw),
            CallTrial::NoMatch { .. } => return Ok(None),
        };
        if !self.typed_call_suffix_continues(closing_raw, suffix_raw)? {
            return Ok(None);
        }

        let opener = self.bump()?.span();
        let mut arguments = Vec::new();
        let stops = TypeStops::empty()
            .with(TypeStops::COMMA)
            .with(TypeStops::GREATER);
        loop {
            arguments.push(self.parse_type_ref(stops)?);
            if self.current_is_symbol(Symbol::Comma) {
                self.bump()?;
                continue;
            }
            break;
        }
        let closer = self.bump()?.span();
        if self.index <= closing_raw
            || self.current_raw()? != suffix_raw
            || !(self.current_is_symbol(Symbol::LeftParen)
                || self.current_is_symbol(Symbol::LeftBrace))
            || self.ast.type_refs().len() <= ast_type_len
            || self.diagnostics.len() != diagnostic_len
        {
            return Err(ParserInternalError::InvalidLexemeStream);
        }
        Ok(Some((arguments, self.span(opener.start(), closer.end())?)))
    }

    fn typed_call_suffix_continues(
        &self,
        closing_raw: usize,
        suffix_raw: usize,
    ) -> Result<bool, ParserInternalError> {
        let closing = self
            .lexed
            .lexemes()
            .get(closing_raw)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        let suffix = self
            .lexed
            .lexemes()
            .get(suffix_raw)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        Ok(!matches!(
            suffix.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
        ) || !self.gap_has_line_break(closing.span().end(), suffix.span().start())?)
    }

    pub(super) fn parse_index(
        &mut self,
        receiver: ExpressionId,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let outer_stops = if outer_stops.block_elements {
            outer_stops.with(Stops::SEMICOLON)
        } else {
            outer_stops
        };
        let block_stop = outer_stops.delimiters & Stops::SEMICOLON;
        let index_stops = outer_stops.without_file_and_semicolon_stops();
        let opener_raw = self.current_raw()?;
        let opener = self.bump()?.span();
        let receiver_span = self.expression_span(receiver)?;
        let inner_stops = index_stops
            .without_lambda_body_soft_stops()
            .with(Stops::RIGHT_BRACKET)
            .with(Stops::COMMA | block_stop);
        let leading_poison = self.is_poison();
        let diagnostic_count = self.diagnostics.len();
        let mut index = self.parse_expression_bp(0, inner_stops)?;
        let index_span = self.expression_span(index)?;
        let current = self.current()?;
        let at_bracket = matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket))
        );
        // A failed key parse can leave a nested bracket at the cursor.
        let mut at_index_closer = at_bracket
            && (self.diagnostics.len() == diagnostic_count
                || self.lexical_recoveries.group_closer(self.lexed, opener_raw)
                    == Some(self.current_raw()?));
        let mut last_consumed_end = index_span.end().max(opener.end());
        if !at_index_closer {
            let comma = matches!(
                current.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::Comma))
            );
            let at_proven_separator = self.at_block_statement_separator(current, outer_stops)?;
            let caller_boundary = outer_stops.contains(current) || at_proven_separator;
            let preserve_separator =
                outer_stops.delimiters & (Stops::SEMICOLON | Stops::BLOCK_STATEMENT_SEPARATOR) != 0;
            let at_block_separator = at_proven_separator
                || (block_stop != 0 && self.current_is_symbol(Symbol::Semicolon))
                || (preserve_separator
                    && self.is_poison_kind(current.kind())
                    && self
                        .adjacent_poison_tail_boundary()?
                        .is_some_and(|(raw, _)| self.is_index_block_separator(outer_stops, raw)));
            let paired_end = if !at_block_separator
                && (comma || at_bracket || leading_poison || !caller_boundary)
            {
                let closer = self.lexical_recoveries.postfix_recovery_closer(
                    self.lexed,
                    opener_raw,
                    self.current_raw()?,
                    preserve_separator,
                );
                self.skip_before_closer(closer)?
            } else {
                None
            };

            let poison_end = if paired_end.is_none() && self.is_poison_kind(current.kind()) {
                self.recover_index_poison_tail(outer_stops, opener)?
            } else {
                None
            };
            let already_diagnosed = self.is_poison_kind(current.kind())
                || (paired_end.is_some() && !comma && (leading_poison || at_bracket));
            if !already_diagnosed {
                let diagnostic_span = if comma && paired_end.is_some() {
                    current.span()
                } else {
                    self.boundary_span(current, outer_stops)?
                };
                self.emit_closing(diagnostic_span, opener)?;
            }
            if let Some(end) = paired_end.or(poison_end) {
                last_consumed_end = end;
                at_index_closer = paired_end.is_some();
                if !comma {
                    index = self
                        .add_expression(self.span(index_span.start(), end)?, Expression::Error)?;
                }
            } else if !caller_boundary {
                if at_bracket {
                    last_consumed_end = self.bump()?.span().end();
                } else if comma {
                    self.bump()?;
                    self.recover_declaration_region(
                        DeclarationStops::for_postfix_suffix(outer_stops)
                            .with(DeclarationStops::RIGHT_BRACKET),
                    )?;
                    last_consumed_end = self.previous_significant_end();
                }
            }
        }

        let end = if at_index_closer {
            self.bump()?.span().end()
        } else {
            last_consumed_end
        };
        self.add_expression(
            self.span(receiver_span.start(), end)?,
            Expression::Index { receiver, index },
        )
    }

    fn recover_index_poison_tail(
        &mut self,
        stops: Stops,
        opener: Span,
    ) -> Result<Option<usize>, ParserInternalError> {
        let Some((boundary_raw, end)) = self.adjacent_poison_tail_boundary()? else {
            return Ok(None);
        };
        let boundary = self.lexed.lexemes()[boundary_raw];
        if matches!(boundary.kind(), LexemeKind::Eof)
            || !(stops.contains(boundary) || self.is_index_block_separator(stops, boundary_raw))
        {
            return Ok(None);
        }
        self.emit_closing(self.empty_at(boundary.span().start())?, opener)?;
        self.index = boundary_raw;
        Ok(Some(end))
    }

    fn is_index_block_separator(&self, stops: Stops, raw: usize) -> bool {
        matches!(
            self.lexed.lexemes()[raw].kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
        ) && (stops.delimiters & Stops::SEMICOLON != 0
            || (stops.delimiters & Stops::BLOCK_STATEMENT_SEPARATOR != 0
                && self
                    .lexical_recoveries
                    .block_statement_follows(self.lexed, raw)))
    }
}

impl DeclarationStops {
    fn for_postfix_suffix(stops: Stops) -> Self {
        let stops = if stops.block_elements {
            stops.with(Stops::SEMICOLON)
        } else {
            stops
        };
        Self::from_expression(stops).with(Self::PROVEN_GROUPS)
    }
}
