use super::*;

impl Parser<'_> {
    pub(super) fn parse_expression_bp(
        &mut self,
        minimum_precedence: u8,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_expression_bp_inner(minimum_precedence, stops);
        self.recursion_depth -= 1;
        result
    }

    pub(super) fn parse_expression_bp_inner(
        &mut self,
        minimum_precedence: u8,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let mut left = self.parse_prefix(stops)?;
        let mut seen_non_associative = [false; 4];

        loop {
            let current = self.current()?;
            if stops.contains(current) {
                break;
            }
            if stops.when_entry_body
                && matches!(
                    current.kind(),
                    LexemeKind::Token(TokenKind::Keyword(Keyword::Is | Keyword::In))
                        | LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs | Symbol::BangIn))
                )
                && self
                    .gap_has_line_break(self.expression_span(left)?.end(), current.span().start())?
            {
                break;
            }

            if let Some(combination) = self.unsupported_operator()? {
                let left_span = self.expression_span(left)?;
                self.emit(
                    codes::UNSUPPORTED_OPERATOR,
                    "unsupported operator",
                    combination,
                )?;
                self.bump()?;
                self.bump()?;
                let end = if self.can_start_expression(self.current()?) {
                    // 只消费当前组合紧邻的一个 prefix/postfix operand；外层 Pratt loop 继续
                    // 处理后续组合，避免平坦错误序列被实现成递归链。
                    let right = self.parse_prefix(stops)?;
                    self.expression_span(right)?.end()
                } else {
                    combination.end()
                };
                left =
                    self.add_expression(self.span(left_span.start(), end)?, Expression::Error)?;
                continue;
            }

            let Some(rule) = self.infix_rule(current)? else {
                break;
            };
            if rule.precedence < minimum_precedence {
                break;
            }

            if let Some(group) = rule.non_associative_group {
                let slot = group as usize;
                if seen_non_associative[slot] {
                    let operator = self.bump()?;
                    self.emit(
                        codes::NON_ASSOCIATIVE_CHAIN,
                        "non-associative operator chain",
                        operator.span(),
                    )?;
                    let right = self.consume_rule_right(rule, stops)?;
                    let right_end = match right {
                        ParsedRight::Expression(id) => self.expression_span(id)?.end(),
                        ParsedRight::Type(id) => self.type_span(id)?.end(),
                    };
                    let error_span = self.span(operator.span().start(), right_end)?;
                    self.add_expression(error_span, Expression::Error)?;
                    let left_span = self.expression_span(left)?;
                    left = self.add_expression(
                        self.span(left_span.start(), right_end)?,
                        Expression::Error,
                    )?;
                    continue;
                }
                seen_non_associative[slot] = true;
            }

            let operator = self.bump()?;
            left = self.build_infix(left, operator.span(), rule, stops)?;
        }
        Ok(left)
    }

    pub(super) fn parse_prefix(
        &mut self,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_prefix_inner(stops);
        self.recursion_depth -= 1;
        result
    }

    pub(super) fn parse_prefix_inner(
        &mut self,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        if let Some(combination) = self.unsupported_operator()? {
            self.emit(
                codes::UNSUPPORTED_OPERATOR,
                "unsupported operator",
                combination,
            )?;
            self.bump()?;
            self.bump()?;
            let end = if self.can_start_expression(self.current()?) {
                let operand = self.parse_expression_bp(PREC_PREFIX, stops)?;
                self.expression_span(operand)?.end()
            } else {
                combination.end()
            };
            return self.add_expression(self.span(combination.start(), end)?, Expression::Error);
        }

        let current = self.current()?;
        let operator = match current.kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::Bang)) => Some(PrefixOperator::Not),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)) => Some(PrefixOperator::Plus),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)) => Some(PrefixOperator::Minus),
            _ => None,
        };
        if let Some(operator) = operator {
            let operator_span = self.bump()?.span();
            let operand = self.parse_expression_bp(PREC_PREFIX, stops)?;
            let operand_span = self.expression_span(operand)?;
            return self.add_expression(
                self.span(operator_span.start(), operand_span.end())?,
                Expression::Prefix {
                    operator,
                    operator_span,
                    operand,
                },
            );
        }

        let primary = self.parse_primary(stops)?;
        self.parse_postfix(primary, stops)
    }

    pub(super) fn parse_primary(
        &mut self,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let current = self.current()?;
        if matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
        ) {
            return self.parse_lambda(None, stops);
        }
        if self.current_identifier_is("move")? && self.peek_is_symbol(1, Symbol::LeftBrace) {
            let move_span = self.bump()?.span();
            return self.parse_lambda(Some(move_span), stops);
        }
        if stops.contains(current) && !control_expression_start_kind(current.kind()) {
            let span = self.empty_at(current.span().start())?;
            self.emit(codes::EXPECTED_EXPRESSION, "expected expression", span)?;
            return self.add_expression(span, Expression::Error);
        }

        match current.kind() {
            LexemeKind::Token(TokenKind::Keyword(Keyword::If)) => self.parse_if(stops),
            LexemeKind::Token(TokenKind::Keyword(Keyword::When)) => self.parse_when(stops),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Return)) => self.parse_return(stops),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Break)) => {
                let keyword_span = self.bump()?.span();
                self.add_expression(keyword_span, Expression::Break { keyword_span })
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::Continue)) => {
                let keyword_span = self.bump()?.span();
                self.add_expression(keyword_span, Expression::Continue { keyword_span })
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::Super)) => self.parse_super(stops),
            LexemeKind::Token(TokenKind::Identifier) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Name)
            }
            LexemeKind::Token(TokenKind::IntegerLiteral(suffix)) => {
                let span = self.bump()?.span();
                let kind = match suffix {
                    IntegerLiteralSuffix::None => IntegerLiteralKind::Unsuffixed,
                    IntegerLiteralSuffix::Long => IntegerLiteralKind::Long,
                    IntegerLiteralSuffix::Unsigned => IntegerLiteralKind::Unsigned,
                    IntegerLiteralSuffix::UnsignedLong => IntegerLiteralKind::UnsignedLong,
                };
                self.add_expression(span, Expression::Literal(LiteralKind::Integer(kind)))
            }
            LexemeKind::Token(TokenKind::FloatLiteral(suffix)) => {
                let span = self.bump()?.span();
                let kind = match suffix {
                    FloatLiteralSuffix::None => FloatLiteralKind::Double,
                    FloatLiteralSuffix::Float => FloatLiteralKind::Float,
                };
                self.add_expression(span, Expression::Literal(LiteralKind::Float(kind)))
            }
            LexemeKind::Token(TokenKind::CharLiteral) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Literal(LiteralKind::Char))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::True)) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Literal(LiteralKind::Boolean(true)))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::False)) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Literal(LiteralKind::Boolean(false)))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::Null)) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Literal(LiteralKind::Null))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::This)) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::This)
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => self.parse_group(stops),
            LexemeKind::Token(TokenKind::Symbol(Symbol::ColonColon)) => {
                self.parse_unbound_reference(stops)
            }
            LexemeKind::Token(TokenKind::StringStart) => self.parse_string(),
            LexemeKind::Invalid(_) | LexemeKind::Token(TokenKind::ReservedWord(_)) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Error)
            }
            _ => {
                let span = self.bump()?.span();
                self.emit(codes::EXPECTED_EXPRESSION, "expected expression", span)?;
                self.add_expression(span, Expression::Error)
            }
        }
    }

    pub(super) fn parse_if(
        &mut self,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let condition = self.parse_parenthesized_condition(keyword_span, outer_stops)?;
        let then_branch = self.parse_control_body(outer_stops.with(Stops::ELSE))?;
        let (else_span, else_branch) = if self.current_is_keyword(Keyword::Else) {
            let else_span = self.bump()?.span();
            let branch = self.parse_control_body(outer_stops)?;
            (Some(else_span), Some(branch))
        } else {
            (None, None)
        };
        let end = else_branch
            .map(|branch| self.statement_span(branch).map(Span::end))
            .transpose()?
            .unwrap_or(self.statement_span(then_branch)?.end());
        self.add_expression(
            self.span(keyword_span.start(), end)?,
            Expression::If {
                keyword_span,
                condition,
                then_branch,
                else_span,
                else_branch,
            },
        )
    }

    pub(super) fn parse_parenthesized_condition(
        &mut self,
        keyword_span: Span,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        if !self.current_is_symbol(Symbol::LeftParen) {
            let current = self.current()?;
            let span = self.boundary_span(current, outer_stops)?;
            self.emit(codes::EXPECTED_CONDITION, "expected condition", span)?;
            return self.add_expression(self.empty_at(keyword_span.end())?, Expression::Error);
        }
        let opener = self.bump()?.span();
        let condition = if self.current_is_symbol(Symbol::RightParen) {
            let empty = self.empty_at(self.current()?.span().start())?;
            self.emit(codes::EXPECTED_CONDITION, "expected condition", empty)?;
            self.add_expression(empty, Expression::Error)?
        } else {
            self.parse_expression_bp(
                0,
                outer_stops
                    .without_lambda_body_soft_stops()
                    .with(Stops::RIGHT_PAREN),
            )?
        };
        if self.current_is_symbol(Symbol::RightParen) {
            self.bump()?;
        } else if !self.is_poison() {
            self.emit_closing(self.boundary_span(self.current()?, outer_stops)?, opener)?;
        }
        Ok(condition)
    }

    pub(super) fn parse_control_body(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        if self.current_is_symbol(Symbol::LeftBrace) {
            return self.parse_control_block(outer_stops);
        }
        let current = self.current()?;
        if self.can_start_expression(current) {
            let expression = self.parse_expression_bp(0, outer_stops)?;
            let span = self.expression_span(expression)?;
            return self.add_statement(span, Statement::Expression { expression });
        }
        let span = self.boundary_span(current, outer_stops)?;
        if !self.is_poison() {
            self.emit(codes::EXPECTED_CONTROL_BODY, "expected control body", span)?;
        }
        self.add_statement(span, Statement::Error)
    }

    pub(super) fn parse_control_block(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let opener = self.bump()?.span();
        let mut elements = Vec::new();
        while !self.current_is_symbol(Symbol::RightBrace)
            && !outer_stops.contains_hard(self.current()?)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            while self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
            }
            if self.current_is_symbol(Symbol::RightBrace)
                || outer_stops.contains_hard(self.current()?)
                || matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                break;
            }
            let before = self.index;
            elements.push(self.parse_block_element(outer_stops)?);
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            while self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
            }
        }
        let end = if self.current_is_symbol(Symbol::RightBrace) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !self.lexical_recoveries.terminal_error_at_eof && !self.is_poison() {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
            self.previous_significant_end().max(opener.end())
        };
        self.add_statement(
            self.span(opener.start(), end)?,
            Statement::ControlBody { elements },
        )
    }

    pub(super) fn parse_when(
        &mut self,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let subject = if self.current_is_symbol(Symbol::LeftParen) {
            let opener = self.bump()?.span();
            let subject = if self.current_is_symbol(Symbol::RightParen) {
                let empty = self.empty_at(self.current()?.span().start())?;
                self.emit(codes::EXPECTED_CONDITION, "expected condition", empty)?;
                self.add_expression(empty, Expression::Error)?
            } else {
                self.parse_expression_bp(0, Stops::ROOT.with(Stops::RIGHT_PAREN))?
            };
            if self.current_is_symbol(Symbol::RightParen) {
                self.bump()?;
            } else if !self.is_poison() {
                self.emit_closing(self.boundary_span(self.current()?, outer_stops)?, opener)?;
            }
            Some(subject)
        } else {
            None
        };
        if !self.current_is_symbol(Symbol::LeftBrace) {
            let current = self.current()?;
            let span = self.boundary_span(current, outer_stops)?;
            self.emit(codes::EXPECTED_CONTROL_BODY, "expected control body", span)?;
            return self.add_expression(
                self.span(keyword_span.start(), self.previous_significant_end())?,
                Expression::When {
                    keyword_span,
                    subject,
                    entries: Vec::new(),
                },
            );
        }
        let opener = self.bump()?.span();
        let mut entries = Vec::new();
        while !self.current_is_symbol(Symbol::RightBrace)
            && !outer_stops.contains_hard(self.current()?)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            let before = self.index;
            entries.push(self.parse_when_entry(outer_stops)?);
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            if self.current_is_symbol(Symbol::RightBrace) {
                break;
            }
            if self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
            } else if !self.gap_has_line_break(
                self.previous_significant_end(),
                self.current()?.span().start(),
            )? {
                self.emit(
                    codes::EXPECTED_WHEN_ENTRY_SEPARATOR,
                    "expected when entry separator",
                    self.current()?.span(),
                )?;
            }
        }
        let end = if self.current_is_symbol(Symbol::RightBrace) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !self.lexical_recoveries.terminal_error_at_eof && !self.is_poison() {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
            self.previous_significant_end().max(opener.end())
        };
        self.add_expression(
            self.span(keyword_span.start(), end)?,
            Expression::When {
                keyword_span,
                subject,
                entries,
            },
        )
    }

    pub(super) fn parse_when_entry(
        &mut self,
        outer_stops: Stops,
    ) -> Result<WhenEntry, ParserInternalError> {
        let start = self.current()?.span().start();
        let (conditions, else_span) = if self.current_is_keyword(Keyword::Else) {
            (Vec::new(), Some(self.bump()?.span()))
        } else {
            let mut conditions = Vec::new();
            loop {
                conditions.push(self.parse_when_condition(outer_stops)?);
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                break;
            }
            (conditions, None)
        };
        let arrow_span = if self.current_is_symbol(Symbol::Arrow) {
            self.bump()?.span()
        } else {
            let span = self.boundary_span(self.current()?, outer_stops.with(Stops::RIGHT_BRACE))?;
            self.emit(codes::EXPECTED_WHEN_ARROW, "expected when arrow", span)?;
            self.empty_at(self.previous_significant_end())?
        };
        let body =
            self.parse_control_body(outer_stops.with(Stops::RIGHT_BRACE).as_when_entry_body())?;
        let end = self.statement_span(body)?.end().max(arrow_span.end());
        Ok(WhenEntry {
            span: self.span(start, end)?,
            conditions,
            else_span,
            arrow_span,
            body,
        })
    }

    pub(super) fn parse_when_condition(
        &mut self,
        outer_stops: Stops,
    ) -> Result<WhenCondition, ParserInternalError> {
        let condition_stops = outer_stops
            .without_lambda_body_soft_stops()
            .with(Stops::COMMA)
            .with(Stops::ARROW)
            .with(Stops::RIGHT_BRACE);
        if self.current_is_keyword(Keyword::Is) || self.current_is_symbol(Symbol::BangIs) {
            let operator = self.bump()?;
            let negated = matches!(
                operator.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs))
            );
            let type_ref = self.parse_type_ref(TypeStops::from_expression(condition_stops))?;
            return Ok(WhenCondition::TypeTest {
                operator_span: operator.span(),
                negated,
                type_ref,
            });
        }
        if self.current_is_keyword(Keyword::In) || self.current_is_symbol(Symbol::BangIn) {
            let operator = self.bump()?;
            let negated = matches!(
                operator.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::BangIn))
            );
            let expression = if self.can_start_expression(self.current()?) {
                self.parse_expression_bp(0, condition_stops)?
            } else {
                let span = self.boundary_span(self.current()?, condition_stops)?;
                self.emit(codes::EXPECTED_CONDITION, "expected condition", span)?;
                self.add_expression(span, Expression::Error)?
            };
            return Ok(WhenCondition::Contains {
                operator_span: operator.span(),
                negated,
                expression,
            });
        }
        if !self.can_start_expression(self.current()?) {
            let span = self.boundary_span(self.current()?, condition_stops)?;
            self.emit(codes::EXPECTED_WHEN_ENTRY, "expected when entry", span)?;
            if !condition_stops.contains(self.current()?) {
                self.bump()?;
            }
            let expression = self.add_expression(span, Expression::Error)?;
            return Ok(WhenCondition::Expression(expression));
        }
        Ok(WhenCondition::Expression(
            self.parse_expression_bp(0, condition_stops)?,
        ))
    }

    pub(super) fn parse_return(
        &mut self,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let current = self.current()?;
        let value = if self.can_start_expression(current)
            && !self.gap_has_line_break(keyword_span.end(), current.span().start())?
            && !stops.contains(current)
        {
            Some(self.parse_expression_bp(0, stops)?)
        } else {
            None
        };
        let end = value
            .map(|value| self.expression_span(value).map(Span::end))
            .transpose()?
            .unwrap_or(keyword_span.end());
        self.add_expression(
            self.span(keyword_span.start(), end)?,
            Expression::Return {
                keyword_span,
                value,
            },
        )
    }

    pub(super) fn parse_super(
        &mut self,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let interface = if self.current_is_symbol(Symbol::Less) {
            let opener = self.bump()?.span();
            let type_ref = self.parse_type_ref(TypeStops::empty().with(TypeStops::GREATER))?;
            if self.current_is_symbol(Symbol::Greater) {
                self.bump()?;
            } else if !self.is_poison() {
                self.emit_closing(
                    self.type_boundary_span(self.current()?, TypeStops::empty())?,
                    opener,
                )?;
            }
            type_ref
        } else {
            let span = self.boundary_span(self.current()?, outer_stops)?;
            self.emit(
                codes::EXPECTED_SUPER_INTERFACE,
                "expected super interface",
                span,
            )?;
            self.add_type_ref(span, TypeRef::Error)?
        };
        let dot_span = if self.current_is_symbol(Symbol::Dot) {
            self.bump()?.span()
        } else {
            let span = self.boundary_span(self.current()?, outer_stops)?;
            self.emit(
                codes::EXPECTED_SUPER_MEMBER_SEPARATOR,
                "expected super member separator",
                span,
            )?;
            self.empty_at(self.previous_significant_end())?
        };
        let name_span = if self.current_is_identifier() {
            self.bump()?.span()
        } else {
            let span = self.boundary_span(self.current()?, outer_stops)?;
            self.emit(
                codes::EXPECTED_MEMBER_NAME,
                "expected member or reference name",
                span,
            )?;
            self.empty_at(self.previous_significant_end())?
        };
        let end = name_span
            .end()
            .max(dot_span.end())
            .max(self.type_span(interface)?.end())
            .max(keyword_span.end());
        self.add_expression(
            self.span(keyword_span.start(), end)?,
            Expression::SuperMember {
                keyword_span,
                interface,
                dot_span,
                name_span,
            },
        )
    }

    pub(super) fn parse_lambda(
        &mut self,
        move_span: Option<Span>,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_lambda_inner(move_span, outer_stops);
        self.recursion_depth -= 1;
        result
    }

    pub(super) fn parse_lambda_inner(
        &mut self,
        move_span: Option<Span>,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let opener_raw = self.current_raw()?;
        let header = self.lambda_headers.query(opener_raw)?.clone();
        let opener = self.bump()?.span();
        let start = move_span.unwrap_or(opener).start();

        let (parameters, arrow_span) = match header {
            LambdaHeaderTrial::NoHeader => (Vec::new(), None),
            LambdaHeaderTrial::Header {
                parameter_raw,
                arrow_raw,
            } => {
                let mut parameters = Vec::with_capacity(parameter_raw.len());
                for (ordinal, expected_raw) in parameter_raw.iter().copied().enumerate() {
                    if self.current_raw()? != expected_raw || !self.current_is_identifier() {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    }
                    parameters.push(self.bump()?.span());
                    if ordinal + 1 < parameter_raw.len() {
                        if !self.current_is_symbol(Symbol::Comma) {
                            return Err(ParserInternalError::InvalidLexemeStream);
                        }
                        self.bump()?;
                    }
                }
                if self.current_raw()? != arrow_raw || !self.current_is_symbol(Symbol::Arrow) {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                let arrow = self.bump()?.span();
                (parameters, Some(arrow))
            }
        };

        let body = self.parse_lambda_body(opener, outer_stops)?;
        let end = self.statement_span(body)?.end();
        self.add_expression(
            self.span(start, end)?,
            Expression::Lambda {
                move_span,
                opener_span: opener,
                parameters,
                arrow_span,
                body,
            },
        )
    }

    pub(super) fn parse_lambda_body(
        &mut self,
        opener: Span,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let mut elements = Vec::new();
        let expression_stops = Stops::lambda_expression(outer_stops);

        while !self.current_is_symbol(Symbol::RightBrace)
            && !outer_stops.contains_hard(self.current()?)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            while self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
            }
            if self.current_is_symbol(Symbol::RightBrace)
                || outer_stops.contains_hard(self.current()?)
                || matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                break;
            }
            #[cfg(test)]
            {
                self.lambda_body_dispatch_iterations += 1;
            }
            let before = self.index;
            let element = if self.current_is_symbol(Symbol::LeftBrace) {
                self.parse_block_statement(outer_stops)?
            } else if self.local_destructuring_start(Keyword::Val) {
                let val_span = self.bump()?.span();
                self.parse_local_destructuring(val_span, expression_stops)?
            } else if self.local_destructuring_start(Keyword::Var)
                || self.const_local_destructuring_start()
            {
                self.parse_unsupported_local_destructuring(expression_stops)?
            } else if self.current_is_keyword(Keyword::While)
                || self.current_is_keyword(Keyword::For)
                || (self.current_identifier_is("loop")? && self.peek_is_symbol(1, Symbol::LeftBrace))
            {
                self.parse_loop_statement(outer_stops)?
            } else if self.current_is_keyword(Keyword::Val) || self.current_is_keyword(Keyword::Var)
            {
                let keyword = self.bump()?;
                let kind = match keyword.kind() {
                    LexemeKind::Token(TokenKind::Keyword(Keyword::Val)) => VariableKind::Val,
                    LexemeKind::Token(TokenKind::Keyword(Keyword::Var)) => VariableKind::Var,
                    _ => return Err(ParserInternalError::InvalidLexemeStream),
                };
                let declaration =
                    self.parse_local_variable_declaration(keyword.span(), kind, expression_stops)?;
                let span = self.ast.items().get(declaration)?.span();
                self.add_statement(span, Statement::LocalVariable { declaration })?
            } else if self.is_unsupported_block_element()? {
                self.parse_unsupported_lambda_body_form()?
            } else {
                let current = self.current()?;
                if self.is_poison_kind(current.kind()) {
                    let span = self.bump()?.span();
                    self.add_statement(span, Statement::Error)?
                } else if self.current_is_symbol(Symbol::Comma)
                    || self.current_is_symbol(Symbol::Arrow)
                {
                    self.parse_unsupported_lambda_body_form()?
                } else if self.can_start_expression(current) {
                    let expression = self.parse_expression_bp(0, expression_stops)?;
                    let expression = if self.expression_statement_boundary(expression)? {
                        expression
                    } else {
                        self.consume_expression_tail(expression, expression_stops)?
                    };
                    let span = self.expression_span(expression)?;
                    self.add_statement(span, Statement::Expression { expression })?
                } else {
                    let span = self.bump()?.span();
                    self.emit(
                        codes::EXPECTED_LAMBDA_BODY_ELEMENT,
                        "expected lambda body element",
                        span,
                    )?;
                    self.add_statement(span, Statement::Error)?
                }
            };
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            elements.push(element);
            while self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
            }
        }

        let end = if self.current_is_symbol(Symbol::RightBrace) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !self.lexical_recoveries.terminal_error_at_eof && !self.is_poison() {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
            self.previous_significant_end().max(opener.end())
        };
        self.add_statement(
            self.span(opener.start(), end)?,
            Statement::LambdaBody { elements },
        )
    }

    pub(super) fn parse_unsupported_lambda_body_form(
        &mut self,
    ) -> Result<StatementId, ParserInternalError> {
        let first_lexeme = self.bump()?;
        let first = first_lexeme.span();
        let mut end = first.end();
        if self.current_is_keyword(Keyword::Val)
            && matches!(
                first_lexeme.kind(),
                LexemeKind::Token(TokenKind::Keyword(Keyword::Const))
            )
        {
            end = self.bump()?.span().end();
        }
        let span = self.span(first.start(), end)?;
        self.emit(
            codes::UNSUPPORTED_LAMBDA_BODY_FORM,
            "unsupported lambda body form",
            span,
        )?;
        self.add_statement(span, Statement::Error)
    }

    pub(super) fn parse_group(
        &mut self,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let outer_stops = outer_stops.without_file_declaration_stop();
        let opener = self.bump()?.span();
        let mut inner = self.parse_expression_bp(
            0,
            outer_stops
                .without_lambda_body_soft_stops()
                .with(Stops::RIGHT_PAREN),
        )?;
        let current = self.current()?;
        if !self.current_is_symbol(Symbol::RightParen)
            && (matches!(
                current.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
            ) || (self.file_mode && file_construct_start_kind(current.kind())))
        {
            self.emit(
                codes::UNEXPECTED_TRAILING_TOKEN,
                "unexpected trailing token",
                current.span(),
            )?;
            let start = self.expression_span(inner)?.start();
            let end = self.recover_declaration_region(
                DeclarationStops::from_expression_hard(outer_stops)
                    .with(DeclarationStops::RIGHT_PAREN),
            )?;
            inner = self.add_expression(self.span(start, end)?, Expression::Error)?;
        }
        let inner_span = self.expression_span(inner)?;
        let end = if self.current_is_symbol(Symbol::RightParen) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !self.is_poison() {
                self.emit_closing(self.boundary_span(current, outer_stops)?, opener)?;
            }
            inner_span.end().max(opener.end())
        };
        self.add_expression(
            self.span(opener.start(), end)?,
            Expression::Group { expression: inner },
        )
    }

    pub(super) fn parse_unbound_reference(
        &mut self,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let operator_span = self.bump()?.span();
        if self.current_is_identifier() {
            let name_span = self.bump()?.span();
            return self.add_expression(
                self.span(operator_span.start(), name_span.end())?,
                Expression::CallableReference {
                    receiver: None,
                    operator_span,
                    name_span,
                },
            );
        }

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
        self.add_expression(self.span(operator_span.start(), end)?, Expression::Error)
    }

    pub(super) fn parse_string(&mut self) -> Result<ExpressionId, ParserInternalError> {
        let opener = self.bump()?.span();
        let mut parts = Vec::new();
        let mut end = opener.end();
        loop {
            let current = self.current()?;
            match current.kind() {
                LexemeKind::Token(TokenKind::StringText) => {
                    let span = self.bump()?.span();
                    end = span.end();
                    parts.push(StringPart::Text(span));
                }
                LexemeKind::Token(TokenKind::InterpolationStart) => {
                    let interpolation_start = self.bump()?.span();
                    let expression =
                        self.parse_expression_bp(0, Stops::ROOT.with(Stops::INTERPOLATION_END))?;
                    let expression = self.consume_expression_tail(
                        expression,
                        Stops::ROOT.with(Stops::INTERPOLATION_END),
                    )?;
                    let expression_span = self.expression_span(expression)?;
                    let interpolation_end = if matches!(
                        self.current()?.kind(),
                        LexemeKind::Token(TokenKind::InterpolationEnd)
                    ) {
                        self.bump()?.span().end()
                    } else {
                        let boundary = self.current()?.span();
                        let lexer_owns_boundary = self
                            .lexical_recoveries
                            .unterminated_interpolation_starts
                            .binary_search(&interpolation_start.start())
                            .is_ok()
                            || (boundary.end() == self.lexical_recoveries.source_len
                                && self.lexical_recoveries.terminal_error_at_eof);
                        if !lexer_owns_boundary {
                            self.emit_closing(boundary, interpolation_start)?;
                        }
                        expression_span.end().max(interpolation_start.end())
                    };
                    let span = self.span(interpolation_start.start(), interpolation_end)?;
                    end = span.end();
                    parts.push(StringPart::Interpolation { span, expression });
                }
                LexemeKind::Token(TokenKind::StringEnd) => {
                    end = self.bump()?.span().end();
                    break;
                }
                LexemeKind::Invalid(_) => {
                    let span = self.bump()?.span();
                    end = span.end();
                    parts.push(StringPart::Error(span));
                    if self
                        .lexical_recoveries
                        .string_recovery_end(opener.start())
                        .is_some_and(|recovery_end| span.end() >= recovery_end)
                    {
                        break;
                    }
                }
                _ => {
                    let lexer_owns_boundary = self
                        .lexical_recoveries
                        .string_recovery_end(opener.start())
                        .is_some()
                        || (current.span().end() == self.lexical_recoveries.source_len
                            && self.lexical_recoveries.terminal_error_at_eof);
                    if !lexer_owns_boundary {
                        self.emit_closing(self.boundary_span(current, Stops::ROOT)?, opener)?;
                    }
                    break;
                }
            }
        }
        self.add_expression(
            self.span(opener.start(), end)?,
            Expression::String { parts },
        )
    }
}
