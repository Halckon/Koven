use super::*;

impl Parser<'_> {
    pub(super) fn root_expression_stops(&self) -> Stops {
        if self.file_mode {
            Stops::FILE
        } else {
            Stops::ROOT
        }
    }

    pub(super) fn root_declaration_stops(&self) -> DeclarationStops {
        if self.file_mode {
            DeclarationStops::FILE
        } else {
            DeclarationStops::EMPTY
        }
    }

    pub(super) fn is_file_declaration_boundary(&self, lexeme: Lexeme) -> bool {
        self.file_mode
            && (file_construct_start_kind(lexeme.kind())
                || matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
                ))
    }

    pub(super) fn file_separator_region_has_line_break(&self) -> Result<bool, ParserInternalError> {
        let start = self.previous_significant_end();
        let end = self.current()?.span().start();
        self.gap_has_line_break(start, end)
    }

    pub(super) fn gap_has_line_break(
        &self,
        start: usize,
        end: usize,
    ) -> Result<bool, ParserInternalError> {
        // 恢复节点可以覆盖当前尚未消费的错误 token；重叠范围之间没有 trivia gap。
        // 直接判 false，让调用方继续既有 tail recovery，而不是构造反向 Span。
        if end <= start {
            return Ok(false);
        }
        Ok(self.sources.slice(self.span(start, end)?)?.contains('\n'))
    }

    pub(super) fn parse_block_root(&mut self) -> Result<StatementId, ParserInternalError> {
        if !self.current_is_symbol(Symbol::LeftBrace) {
            return self.parse_missing_block_root();
        }
        let root = self.parse_block_statement(Stops::ROOT)?;
        if !matches!(self.current()?.kind(), LexemeKind::Eof) {
            let first = self.current()?.span();
            self.emit(
                codes::UNEXPECTED_TRAILING_TOKEN,
                "unexpected trailing token",
                first,
            )?;
            self.recover_declaration_region(DeclarationStops::EMPTY)?;
        }
        Ok(root)
    }

    pub(super) fn parse_missing_block_root(&mut self) -> Result<StatementId, ParserInternalError> {
        let current = self.current()?;
        let start = current.span().start();
        if matches!(current.kind(), LexemeKind::Eof) {
            let span = self.empty_at(start)?;
            self.emit(codes::EXPECTED_BLOCK, "expected block", span)?;
            return self.add_statement(span, Statement::Error);
        }
        if !self.is_poison_kind(current.kind()) {
            self.emit(codes::EXPECTED_BLOCK, "expected block", current.span())?;
        }

        // 独立入口一旦缺 opener，整个剩余输入属于同一个 error root。这里按 raw lexeme 前进，
        // 因而 trailing trivia 也被消费，但节点范围只止于最后一个实际 token / invalid。
        let mut end = current.span().end();
        while self.index < self.lexed.lexemes().len() {
            let lexeme = self.lexed.lexemes()[self.index];
            self.index += 1;
            if !matches!(lexeme.kind(), LexemeKind::Trivia(_) | LexemeKind::Eof) {
                end = lexeme.span().end();
            }
            if matches!(lexeme.kind(), LexemeKind::Eof) {
                break;
            }
        }
        self.add_statement(self.span(start, end)?, Statement::Error)
    }

    pub(super) fn parse_block_statement(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_block_statement_inner(outer_stops);
        self.recursion_depth -= 1;
        result
    }

    pub(super) fn parse_block_statement_inner(
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
            #[cfg(test)]
            {
                self.block_dispatch_iterations += 1;
            }
            let before = self.index;
            let element = self.parse_block_element(outer_stops)?;
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
            if !self.lexical_recoveries.terminal_error_at_eof {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
            self.previous_significant_end().max(opener.end())
        };
        self.add_statement(
            self.span(opener.start(), end)?,
            Statement::Block { elements },
        )
    }

    pub(super) fn parse_block_element(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        if self.current_is_symbol(Symbol::LeftBrace) {
            return self.parse_block_statement(outer_stops);
        }
        let expression_stops = Stops::block_expression(outer_stops);
        if self.local_destructuring_start(Keyword::Val) {
            let val_span = self.bump()?.span();
            return self.parse_local_destructuring(val_span, expression_stops);
        }
        if self.local_destructuring_start(Keyword::Var) || self.const_local_destructuring_start() {
            return self.parse_unsupported_local_destructuring(expression_stops);
        }
        if self.current_is_keyword(Keyword::While)
            || self.current_is_keyword(Keyword::For)
            || (self.current_identifier_is("loop")? && self.peek_is_symbol(1, Symbol::LeftBrace))
        {
            return self.parse_loop_statement(outer_stops);
        }
        if self.current_is_keyword(Keyword::Val) || self.current_is_keyword(Keyword::Var) {
            let keyword = self.bump()?;
            let kind = match keyword.kind() {
                LexemeKind::Token(TokenKind::Keyword(Keyword::Val)) => VariableKind::Val,
                LexemeKind::Token(TokenKind::Keyword(Keyword::Var)) => VariableKind::Var,
                _ => return Err(ParserInternalError::InvalidLexemeStream),
            };
            let declaration =
                self.parse_local_variable_declaration(keyword.span(), kind, expression_stops)?;
            let span = self.ast.items().get(declaration)?.span();
            return self.add_statement(span, Statement::LocalVariable { declaration });
        }
        if self.is_unsupported_block_element()? {
            return self.parse_unsupported_block_element();
        }

        let current = self.current()?;
        if self.is_poison_kind(current.kind()) {
            let span = self.bump()?.span();
            return self.add_statement(span, Statement::Error);
        }
        if self.can_start_expression(current) {
            let stops = Stops::block_expression(outer_stops);
            let expression = self.parse_expression_bp(0, stops)?;
            let expression = if self.expression_statement_boundary(expression)? {
                expression
            } else {
                self.consume_expression_tail(expression, stops)?
            };
            let span = self.expression_span(expression)?;
            return self.add_statement(span, Statement::Expression { expression });
        }

        let span = self.bump()?.span();
        self.emit(
            codes::EXPECTED_BLOCK_ELEMENT,
            "expected block element",
            span,
        )?;
        self.add_statement(span, Statement::Error)
    }

    pub(super) fn parse_loop_statement(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        if self.current_is_keyword(Keyword::While) {
            let keyword_span = self.bump()?.span();
            let condition = self.parse_parenthesized_condition(keyword_span, outer_stops)?;
            let body = self.parse_required_loop_body(outer_stops)?;
            let end = self
                .statement_span(body)?
                .end()
                .max(self.expression_span(condition)?.end());
            return self.add_statement(
                self.span(keyword_span.start(), end)?,
                Statement::While {
                    keyword_span,
                    condition,
                    body,
                },
            );
        }
        if self.current_identifier_is("loop")? {
            let keyword_span = self.bump()?.span();
            let body = self.parse_required_loop_body(outer_stops)?;
            let end = self.statement_span(body)?.end();
            return self.add_statement(
                self.span(keyword_span.start(), end)?,
                Statement::Loop { keyword_span, body },
            );
        }

        let keyword_span = self.bump()?.span();
        let opener = if self.current_is_symbol(Symbol::LeftParen) {
            Some(self.bump()?.span())
        } else {
            let span = self.boundary_span(self.current()?, outer_stops)?;
            self.emit(codes::EXPECTED_FOR_BINDING, "expected for binding", span)?;
            None
        };
        let binding = self.parse_for_binding(outer_stops)?;
        let in_span = if self.current_is_keyword(Keyword::In) {
            self.bump()?.span()
        } else {
            let span = self.boundary_span(self.current()?, outer_stops)?;
            self.emit(codes::EXPECTED_FOR_IN, "expected in", span)?;
            self.empty_at(self.previous_significant_end())?
        };
        let source = if self.can_start_expression(self.current()?) {
            self.parse_expression_bp(0, outer_stops.with(Stops::RIGHT_PAREN))?
        } else {
            let span = self.boundary_span(self.current()?, outer_stops.with(Stops::RIGHT_PAREN))?;
            self.emit(codes::EXPECTED_CONDITION, "expected condition", span)?;
            self.add_expression(span, Expression::Error)?
        };
        if self.current_is_symbol(Symbol::RightParen) {
            self.bump()?;
        } else if let Some(opener) = opener
            && !self.is_poison()
        {
            self.emit_closing(self.boundary_span(self.current()?, outer_stops)?, opener)?;
        }
        let body = self.parse_required_loop_body(outer_stops)?;
        let end = self
            .statement_span(body)?
            .end()
            .max(self.expression_span(source)?.end());
        self.add_statement(
            self.span(keyword_span.start(), end)?,
            Statement::For {
                keyword_span,
                binding,
                in_span,
                source,
                body,
            },
        )
    }

    pub(super) fn parse_for_binding(
        &mut self,
        outer_stops: Stops,
    ) -> Result<ForBinding, ParserInternalError> {
        if self.current_is_identifier() {
            return Ok(ForBinding::Name(NameMarker::Present(self.bump()?.span())));
        }
        if !self.current_is_symbol(Symbol::LeftParen) {
            let current = self.current()?;
            let span = self.boundary_span(current, outer_stops.with(Stops::RIGHT_PAREN))?;
            self.emit(codes::EXPECTED_FOR_BINDING, "expected for binding", span)?;
            return Ok(ForBinding::Name(NameMarker::Missing(span)));
        }
        let left_paren_span = self.bump()?.span();
        let mut names = Vec::new();
        while !self.current_is_symbol(Symbol::RightParen)
            && !self.current_is_keyword(Keyword::In)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            if self.current_is_identifier() {
                names.push(NameMarker::Present(self.bump()?.span()));
            } else {
                let span = self.current()?.span();
                if !self.is_poison() {
                    self.emit(codes::EXPECTED_FOR_BINDING, "expected for binding", span)?;
                }
                names.push(NameMarker::Error(self.bump()?.span()));
            }
            if self.current_is_symbol(Symbol::Comma) {
                self.bump()?;
                continue;
            }
            break;
        }
        if names.is_empty() {
            let span = self.empty_at(self.current()?.span().start())?;
            self.emit(codes::EXPECTED_FOR_BINDING, "expected for binding", span)?;
            names.push(NameMarker::Missing(span));
        }
        let right_paren_span = if self.current_is_symbol(Symbol::RightParen) {
            Some(self.bump()?.span())
        } else {
            None
        };
        Ok(ForBinding::Destructuring {
            left_paren_span,
            names,
            right_paren_span,
        })
    }

    pub(super) fn parse_required_loop_body(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        if self.current_is_symbol(Symbol::LeftBrace) {
            return self.parse_block_statement(outer_stops);
        }
        let span = self.boundary_span(self.current()?, outer_stops)?;
        if !self.is_poison() {
            self.emit(codes::EXPECTED_LOOP_BODY, "expected loop body", span)?;
        }
        self.add_statement(span, Statement::Error)
    }

    pub(super) fn parse_unsupported_block_element(
        &mut self,
    ) -> Result<StatementId, ParserInternalError> {
        let is_value_class =
            self.current_identifier_is("value")? && self.peek_is_keyword(1, Keyword::Class);
        let first_lexeme = self.bump()?;
        let first = first_lexeme.span();
        let mut end = first.end();
        if is_value_class
            || (self.current_is_keyword(Keyword::Val)
                && matches!(
                    first_lexeme.kind(),
                    LexemeKind::Token(TokenKind::Keyword(Keyword::Const))
                ))
        {
            end = self.bump()?.span().end();
        }
        let span = self.span(first.start(), end)?;
        self.emit(
            codes::UNSUPPORTED_BLOCK_ELEMENT,
            "unsupported block element",
            span,
        )?;
        self.add_statement(span, Statement::Error)
    }

    pub(super) fn is_unsupported_block_element(&self) -> Result<bool, ParserInternalError> {
        if self.current_identifier_is("value")? && self.peek_is_keyword(1, Keyword::Class) {
            return Ok(true);
        }
        Ok(self
            .peek(0)
            .is_some_and(|lexeme| unsupported_block_element_kind(lexeme.kind())))
    }
}
