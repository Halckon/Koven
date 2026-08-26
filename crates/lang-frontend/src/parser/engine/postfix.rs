use super::*;

impl Parser<'_> {
    pub(super) fn parse_postfix(
        &mut self,
        mut receiver: ExpressionId,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        loop {
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
                    self.parse_call(receiver, type_arguments, Some(type_arguments_span), stops)?
                } else {
                    self.index = checkpoint;
                    break;
                }
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
        let first = self.bump()?;
        let mut end = first.span().end();
        match first.kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => {
                end = self.consume_balanced_suffix(Symbol::RightParen, end, stops)?;
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBracket)) => {
                end = self.consume_balanced_suffix(Symbol::RightBracket, end, stops)?;
            }
            LexemeKind::Token(TokenKind::Symbol(
                Symbol::Dot | Symbol::QuestionDot | Symbol::ColonColon,
            )) if !matches!(self.current()?.kind(), LexemeKind::Eof) => {
                end = self.bump()?.span().end();
            }
            _ => {}
        }
        self.add_expression(self.span(receiver_span.start(), end)?, Expression::Error)
    }

    pub(super) fn consume_balanced_suffix(
        &mut self,
        closer: Symbol,
        end: usize,
        stops: Stops,
    ) -> Result<usize, ParserInternalError> {
        let closer_stop = match closer {
            Symbol::RightParen => DeclarationStops::RIGHT_PAREN,
            Symbol::RightBracket => DeclarationStops::RIGHT_BRACKET,
            _ => return Err(ParserInternalError::InvalidLexemeStream),
        };
        let recovery_stops =
            DeclarationStops::from_expression_hard(stops.without_lambda_body_soft_stops())
                .with(closer_stop);
        let mut end = self.recover_declaration_region(recovery_stops)?.max(end);
        if self.current_is_symbol(closer) {
            end = self.bump()?.span().end();
        }
        Ok(end)
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
        // `value` remains a hard keyword everywhere else; v0.30 reserves only the
        // postfix member spelling needed by the compiler-bound `Rc<T>.value` contract.
        if !self.current_is_identifier() && !self.current_is_keyword(Keyword::Value) {
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
        let outer_stops = outer_stops.without_file_declaration_stop();
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
                if self.call_argument_boundary(current, outer_stops) {
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

        let end = if self.current_is_symbol(Symbol::RightParen) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !self.is_poison() {
                self.emit_closing(self.boundary_span(current, outer_stops)?, opener)?;
            }
            last_consumed_end
        };
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
        let (value, consumed_separator) = if self.call_argument_boundary(current, outer_stops) {
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
        let marker = if self.current_is_keyword(Keyword::Borrow) {
            Some(ParameterModeMarker::Borrow(self.bump()?.span()))
        } else if self.current_is_symbol(Symbol::Ampersand) {
            Some(ParameterModeMarker::Inout(self.bump()?.span()))
        } else {
            None
        };
        if marker.is_none() {
            return Ok(None);
        }
        while self.current_is_keyword(Keyword::Borrow) || self.current_is_symbol(Symbol::Ampersand)
        {
            let duplicate = self.bump()?.span();
            self.emit(
                codes::DUPLICATE_ARGUMENT_MODE,
                "duplicate argument mode",
                duplicate,
            )?;
        }
        Ok(marker)
    }

    pub(super) fn can_start_call_argument(&self, lexeme: Lexeme) -> bool {
        self.can_start_expression(lexeme)
            || self.current_is_keyword(Keyword::Borrow)
            || self.current_is_symbol(Symbol::Ampersand)
    }

    pub(super) fn call_argument_boundary(&self, lexeme: Lexeme, outer_stops: Stops) -> bool {
        matches!(lexeme.kind(), LexemeKind::Eof)
            || self.current_is_symbol(Symbol::Comma)
            || self.current_is_symbol(Symbol::RightParen)
            || (outer_stops.contains(lexeme)
                && !self.can_start_expression(lexeme)
                && !self.current_is_symbol(Symbol::Comma)
                && !self.current_is_symbol(Symbol::RightParen))
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

    /// 以只读严格识别器判断 `<...>(`，成功后才使用正式 TypeRef parser提交节点。
    pub(super) fn try_parse_call_type_arguments(
        &mut self,
    ) -> Result<Option<(Vec<TypeRefId>, Span)>, ParserInternalError> {
        let trial_start = self
            .significant(0)
            .map(|(raw, _)| raw)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        let ast_type_len = self.ast.type_refs().len();
        let diagnostic_len = self.diagnostics.len();
        let closing_raw = match self
            .strict_trials
            .query(trial_start, self.recursion_depth)?
        {
            CallTrial::Match { closing_raw, .. } => closing_raw,
            CallTrial::NoMatch { .. } => return Ok(None),
        };

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
            || !self.current_is_symbol(Symbol::LeftParen)
            || self.ast.type_refs().len() <= ast_type_len
            || self.diagnostics.len() != diagnostic_len
        {
            return Err(ParserInternalError::InvalidLexemeStream);
        }
        Ok(Some((arguments, self.span(opener.start(), closer.end())?)))
    }

    pub(super) fn parse_index(
        &mut self,
        receiver: ExpressionId,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let outer_stops = outer_stops.without_file_declaration_stop();
        let opener = self.bump()?.span();
        let receiver_span = self.expression_span(receiver)?;
        let inner_stops = outer_stops
            .without_lambda_body_soft_stops()
            .with(Stops::RIGHT_BRACKET)
            .with(Stops::COMMA);
        let index = self.parse_expression_bp(0, inner_stops)?;
        let index_span = self.expression_span(index)?;
        let mut last_consumed_end = index_span.end().max(opener.end());
        let mut reported_missing_closer = false;

        if self.current_is_symbol(Symbol::Comma) {
            let comma = self.bump()?.span();
            self.emit_closing(comma, opener)?;
            reported_missing_closer = true;
            last_consumed_end = comma.end();
            while !self.current_is_symbol(Symbol::RightBracket)
                && !outer_stops.contains(self.current()?)
            {
                last_consumed_end = self.bump()?.span().end();
            }
        }

        let end = if self.current_is_symbol(Symbol::RightBracket) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !reported_missing_closer && !self.is_poison() {
                self.emit_closing(self.boundary_span(current, outer_stops)?, opener)?;
            }
            last_consumed_end
        };
        self.add_expression(
            self.span(receiver_span.start(), end)?,
            Expression::Index { receiver, index },
        )
    }
}
