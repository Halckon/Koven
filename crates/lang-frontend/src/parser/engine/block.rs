use super::*;

pub(super) const fn unsupported_block_element_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Const
                | Keyword::Fun
                | Keyword::For
                | Keyword::While
                | Keyword::Class
                | Keyword::Interface
                | Keyword::Enum
                | Keyword::Object
                | Keyword::Companion
        ))
    )
}

pub(super) const fn block_expression_line_break_boundary(
    block_elements: bool,
    kind: LexemeKind,
    has_line_break: bool,
) -> bool {
    block_elements
        && has_line_break
        && matches!(
            kind,
            LexemeKind::Token(TokenKind::Symbol(
                Symbol::LeftParen | Symbol::Plus | Symbol::Minus
            ))
        )
}

pub(super) const fn is_explicit_block_start(
    current_kind: LexemeKind,
    is_block_ident: bool,
    next_kind: Option<LexemeKind>,
    has_line_break: bool,
) -> bool {
    matches!(current_kind, LexemeKind::Token(TokenKind::Identifier))
        && is_block_ident
        && matches!(
            next_kind,
            Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)))
        )
        && !has_line_break
}

pub(super) const fn block_element_separator_required(
    current_kind: LexemeKind,
    outer_hard_stop: bool,
    is_poison: bool,
    is_unsupported_block_element: bool,
    can_start_expression: bool,
    prev_is_error: bool,
    has_line_break: bool,
) -> bool {
    let starts_loop = matches!(
        current_kind,
        LexemeKind::Token(TokenKind::Keyword(Keyword::While | Keyword::For))
    );
    let is_delim_or_eof = matches!(
        current_kind,
        LexemeKind::Eof
            | LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon | Symbol::RightBrace))
    );
    let starts_next = can_start_expression
        || matches!(
            current_kind,
            LexemeKind::Token(TokenKind::Keyword(
                Keyword::Val | Keyword::Var | Keyword::While | Keyword::For
            ))
        );
    !is_delim_or_eof
        && !outer_hard_stop
        && !is_poison
        && (!is_unsupported_block_element || starts_loop)
        && starts_next
        && !prev_is_error
        && !has_line_break
}

pub(super) const fn step_missing_block_root_end(
    current_end: usize,
    kind: LexemeKind,
    lexeme_end: usize,
) -> (usize, bool) {
    match kind {
        LexemeKind::Eof => (current_end, true),
        LexemeKind::Trivia(_) => (current_end, false),
        _ => (lexeme_end, false),
    }
}

#[cfg(any(test, kani))]
pub(super) fn run_missing_block_root_end(
    initial_end: usize,
    lexemes: &[(LexemeKind, usize)],
) -> usize {
    let mut end = initial_end;
    let mut index = 0usize;
    while index < lexemes.len() {
        let (kind, lexeme_end) = lexemes[index];
        let (next_end, stop) = step_missing_block_root_end(end, kind, lexeme_end);
        end = next_end;
        if stop {
            break;
        }
        index += 1;
    }
    end
}

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

    pub(super) fn at_block_expression_line_break(
        &self,
        left: ExpressionId,
        stops: Stops,
    ) -> Result<bool, ParserInternalError> {
        // All block owners share this boundary; group/call/index clear their soft stops.
        if !stops.block_elements {
            return Ok(false);
        }
        let current = self.current()?;
        if !block_expression_line_break_boundary(true, current.kind(), true) {
            return Ok(false);
        }
        let has_line_break =
            self.gap_has_line_break(self.expression_span(left)?.end(), current.span().start())?;
        Ok(block_expression_line_break_boundary(
            stops.block_elements,
            current.kind(),
            has_line_break,
        ))
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
            let (next_end, stop) =
                step_missing_block_root_end(end, lexeme.kind(), lexeme.span().end());
            end = next_end;
            if stop {
                break;
            }
        }
        self.add_statement(self.span(start, end)?, Statement::Error)
    }

    pub(super) fn explicit_block_start(
        &self,
        current: Lexeme,
    ) -> Result<bool, ParserInternalError> {
        if !matches!(current.kind(), LexemeKind::Token(TokenKind::Identifier)) {
            return Ok(false);
        }
        let is_block_ident = self.sources.slice(current.span())? == "block";
        if !is_block_ident {
            return Ok(false);
        }
        let Some(next) = self.peek(1) else {
            return Ok(false);
        };
        if !is_explicit_block_start(current.kind(), true, Some(next.kind()), false) {
            return Ok(false);
        }
        let has_line_break = self.gap_has_line_break(current.span().end(), next.span().start())?;
        Ok(is_explicit_block_start(
            current.kind(),
            is_block_ident,
            Some(next.kind()),
            has_line_break,
        ))
    }

    pub(super) fn parse_explicit_block_statement(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let keyword = self.bump()?.span();
        self.parse_block_statement_with_keyword(Some(keyword), outer_stops)
    }

    pub(super) fn parse_bare_block_error(
        &mut self,
        code: &'static str,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let brace = self.current()?.span();
        self.emit(
            code,
            "bare block statement is not allowed; use `block { ... }`",
            brace,
        )?;
        let recovered = self.parse_block_statement(outer_stops)?;
        let span = self.statement_span(recovered)?;
        self.add_statement(span, Statement::Error)
    }

    pub(super) fn parse_block_statement(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        self.parse_block_statement_with_keyword(None, outer_stops)
    }

    pub(super) fn parse_block_statement_with_keyword(
        &mut self,
        keyword: Option<Span>,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_block_statement_inner(keyword, outer_stops);
        self.recursion_depth -= 1;
        result
    }

    pub(super) fn parse_block_statement_inner(
        &mut self,
        keyword: Option<Span>,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let opener = self.bump()?.span();
        let mut elements = Vec::new();

        let current = loop {
            let current = self.current()?;
            if matches!(
                current.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace))
            ) || outer_stops.contains_hard(current)
            {
                break current;
            }
            if matches!(
                current.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
            ) {
                self.bump()?;
                continue;
            }
            #[cfg(test)]
            {
                self.block_dispatch_iterations += 1;
            }
            let before = self.index;
            let diagnostics_before = self.diagnostics.len();
            let element = self.parse_block_element(outer_stops)?;
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            if self.diagnostics.len() == diagnostics_before {
                self.check_block_element_separator(element, outer_stops)?;
            }
            elements.push(element);
        };

        let (end, recovery) = if matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace))
        ) {
            (self.bump()?.span().end(), None)
        } else {
            let recovery = self.empty_at(current.span().start())?;
            if !self.lexical_recoveries.terminal_error_at_eof {
                self.emit_closing(recovery, opener)?;
            }
            (
                self.previous_significant_end().max(opener.end()),
                Some(recovery),
            )
        };
        let start = keyword.map_or(opener.start(), Span::start);
        let block = self.add_statement(self.span(start, end)?, Statement::Block { elements })?;
        if let Some(recovery) = recovery {
            self.missing_block_closers.push((block, recovery));
        }
        Ok(block)
    }

    /// Report a missing separator without consuming the next real element or its owner.
    pub(super) fn check_block_element_separator(
        &mut self,
        element: StatementId,
        outer_stops: Stops,
    ) -> Result<(), ParserInternalError> {
        let current = self.current()?;
        let outer_hard_stop = outer_stops.contains_hard(current);
        let is_poison = self.is_poison_kind(current.kind());
        if !block_element_separator_required(
            current.kind(),
            outer_hard_stop,
            is_poison,
            false,
            true,
            false,
            false,
        ) {
            return Ok(());
        }
        let is_unsupported = self.is_unsupported_block_element()?;
        let can_start_expr = self.can_start_expression(current);
        if !block_element_separator_required(
            current.kind(),
            outer_hard_stop,
            is_poison,
            is_unsupported,
            can_start_expr,
            false,
            false,
        ) {
            return Ok(());
        }
        let node = self.ast.statements().get(element)?;
        let expression = match node.payload() {
            Statement::Error => return Ok(()),
            Statement::Expression { expression } => Some(*expression),
            Statement::LocalVariable { declaration } => {
                match self.ast.items().get(*declaration)?.payload() {
                    Item::Variable { initializer, .. } => Some(*initializer),
                    _ => None,
                }
            }
            _ => None,
        };
        let prev_is_error = if let Some(id) = expression {
            matches!(self.ast.expressions().get(id)?.payload(), Expression::Error)
        } else {
            false
        };
        if prev_is_error {
            return Ok(());
        }
        let has_line_break = self.gap_has_line_break(node.span().end(), current.span().start())?;
        if !block_element_separator_required(
            current.kind(),
            outer_hard_stop,
            is_poison,
            is_unsupported,
            can_start_expr,
            prev_is_error,
            has_line_break,
        ) {
            return Ok(());
        }
        self.emit(
            codes::EXPECTED_STATEMENT_SEPARATOR,
            "expected statement separator",
            self.empty_at(current.span().start())?,
        )
    }

    pub(super) fn parse_block_element(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let current = self.current()?;
        if self.explicit_block_start(current)? {
            return self.parse_explicit_block_statement(outer_stops);
        }
        if matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
        ) {
            return self.parse_bare_block_error(codes::UNSUPPORTED_BLOCK_ELEMENT, outer_stops);
        }
        let expression_stops = Stops::block_expression(outer_stops);
        if self.borrow_local_start()? {
            return self.parse_borrow_local(expression_stops);
        }
        if self.local_destructuring_start(Keyword::Val) {
            let val_span = self.bump()?.span();
            return self.parse_local_destructuring(val_span, expression_stops);
        }
        if self.local_destructuring_start(Keyword::Var) || self.const_local_destructuring_start() {
            return self.parse_unsupported_local_destructuring(expression_stops);
        }
        if matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Keyword(Keyword::While | Keyword::For))
        ) || (self.current_identifier_is("loop")? && self.peek_is_symbol(1, Symbol::LeftBrace))
        {
            return self.parse_loop_statement(outer_stops);
        }
        if matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
        ) {
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
        let boundary_stops = outer_stops.with(Stops::RIGHT_BRACE | Stops::SEMICOLON);
        let opener = if self.current_is_symbol(Symbol::LeftParen) {
            Some(self.bump()?.span())
        } else {
            let span = self.boundary_span(self.current()?, boundary_stops)?;
            self.emit(codes::EXPECTED_FOR_BINDING, "expected for binding", span)?;
            None
        };
        let binding = self.parse_for_binding(boundary_stops)?;
        let in_span = if self.current_is_keyword(Keyword::In) {
            self.bump()?.span()
        } else {
            let span = self.boundary_span(self.current()?, boundary_stops)?;
            self.emit(codes::EXPECTED_FOR_IN, "expected in", span)?;
            self.empty_at(self.previous_significant_end())?
        };
        let source = if self.can_start_expression(self.current()?) {
            let source_stops = if opener.is_some() {
                outer_stops.without_lambda_body_soft_stops()
            } else {
                outer_stops
            };
            self.parse_expression_bp(0, source_stops.with(Stops::RIGHT_PAREN))?
        } else {
            let span =
                self.boundary_span(self.current()?, boundary_stops.with(Stops::RIGHT_PAREN))?;
            self.emit(codes::EXPECTED_CONDITION, "expected condition", span)?;
            self.add_expression(span, Expression::Error)?
        };
        if self.current_is_symbol(Symbol::RightParen) {
            self.bump()?;
        } else if let Some(opener) = opener
            && !self.is_poison()
        {
            self.emit_closing(self.boundary_span(self.current()?, boundary_stops)?, opener)?;
        }
        let body_stops = if self.current_is_symbol(Symbol::LeftBrace) {
            outer_stops
        } else {
            boundary_stops
        };
        let body = self.parse_required_loop_body(body_stops)?;
        // Insertion points at an enclosing delimiter do not extend the loop's source range.
        let end = self.previous_significant_end();
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
        loop {
            let current = self.current()?;
            if outer_stops.contains_hard(current)
                || matches!(
                    current.kind(),
                    LexemeKind::Token(
                        TokenKind::Symbol(
                            Symbol::RightParen | Symbol::RightBrace | Symbol::Semicolon
                        ) | TokenKind::Keyword(Keyword::In)
                    )
                )
            {
                break;
            }
            if self.current_is_identifier() {
                names.push(NameMarker::Present(self.bump()?.span()));
            } else {
                let span = current.span();
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
            let current = self.current()?;
            let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                && self.lexical_recoveries.terminal_error_at_eof;
            if !self.is_poison() && !terminal_lexer_root {
                let span = if self.current_is_keyword(Keyword::In) {
                    self.empty_at(current.span().start())?
                } else {
                    self.boundary_span(current, outer_stops)?
                };
                self.emit_closing(span, left_paren_span)?;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::TriviaKind;

    #[test]
    fn unsupported_block_element_kind_matches_exact_keyword_set() {
        for keyword in [
            Keyword::Const,
            Keyword::Fun,
            Keyword::For,
            Keyword::While,
            Keyword::Class,
            Keyword::Interface,
            Keyword::Enum,
            Keyword::Object,
            Keyword::Companion,
        ] {
            assert!(unsupported_block_element_kind(LexemeKind::Token(
                TokenKind::Keyword(keyword)
            )));
        }
        assert!(!unsupported_block_element_kind(LexemeKind::Token(
            TokenKind::Keyword(Keyword::Val)
        )));
        assert!(!unsupported_block_element_kind(LexemeKind::Token(
            TokenKind::Identifier
        )));
    }

    #[test]
    fn block_expression_line_break_boundary_requires_owner_newline_and_ambiguous_symbol() {
        for symbol in [Symbol::LeftParen, Symbol::Plus, Symbol::Minus] {
            let kind = LexemeKind::Token(TokenKind::Symbol(symbol));
            assert!(block_expression_line_break_boundary(true, kind, true));
            assert!(!block_expression_line_break_boundary(false, kind, true));
            assert!(!block_expression_line_break_boundary(true, kind, false));
        }
        assert!(!block_expression_line_break_boundary(
            true,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Dot)),
            true
        ));
    }

    #[test]
    fn explicit_block_start_requires_same_line_block_ident_and_left_brace() {
        let ident = LexemeKind::Token(TokenKind::Identifier);
        let lbrace = Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)));
        assert!(is_explicit_block_start(ident, true, lbrace, false));
        assert!(!is_explicit_block_start(ident, false, lbrace, false));
        assert!(!is_explicit_block_start(ident, true, lbrace, true));
        assert!(!is_explicit_block_start(ident, true, None, false));
    }

    #[test]
    fn block_element_separator_exempts_same_line_while_and_for_from_unsupported_suppression() {
        let kw_while = LexemeKind::Token(TokenKind::Keyword(Keyword::While));
        let kw_for = LexemeKind::Token(TokenKind::Keyword(Keyword::For));
        let kw_fun = LexemeKind::Token(TokenKind::Keyword(Keyword::Fun));
        assert!(block_element_separator_required(
            kw_while, false, false, true, false, false, false
        ));
        assert!(block_element_separator_required(
            kw_for, false, false, true, false, false, false
        ));
        assert!(!block_element_separator_required(
            kw_fun, false, false, true, true, false, false
        ));
        assert!(!block_element_separator_required(
            kw_while, false, false, true, false, false, true
        ));
    }

    #[test]
    fn missing_block_root_end_stops_at_eof_and_ignores_trailing_trivia() {
        let end = run_missing_block_root_end(
            3,
            &[
                (LexemeKind::Token(TokenKind::Identifier), 8),
                (LexemeKind::Trivia(TriviaKind::Whitespace), 10),
                (LexemeKind::Eof, 10),
                (LexemeKind::Token(TokenKind::Identifier), 25),
            ],
        );
        assert_eq!(end, 8);
    }
}
