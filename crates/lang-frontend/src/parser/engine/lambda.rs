//! Lambda header、body 与恢复的完整语法入口。
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LambdaBodyLoopAction {
    Stop,
    SkipSemicolon,
    ParseElement,
}

pub(super) const fn lambda_body_loop_action(
    kind: LexemeKind,
    outer_hard_stop: bool,
) -> LambdaBodyLoopAction {
    if matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace))
    ) || outer_hard_stop
    {
        LambdaBodyLoopAction::Stop
    } else if matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
    ) {
        LambdaBodyLoopAction::SkipSemicolon
    } else {
        LambdaBodyLoopAction::ParseElement
    }
}

#[cfg(any(test, kani))]
pub(super) fn skip_lambda_body_semicolons_in<T: Copy>(
    items: &[T],
    kind_of: impl Fn(T) -> LexemeKind,
    is_outer_hard_stop: impl Fn(T) -> bool,
) -> usize {
    let mut index = 0usize;
    while index < items.len() {
        let item = items[index];
        if lambda_body_loop_action(kind_of(item), is_outer_hard_stop(item))
            == LambdaBodyLoopAction::SkipSemicolon
        {
            index += 1;
        } else {
            break;
        }
    }
    index
}

pub(super) const fn is_lambda_body_unsupported_symbol_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::Comma | Symbol::Arrow))
    )
}

pub(super) const fn lambda_body_missing_closer_emits_diagnostic(
    current_kind: LexemeKind,
    terminal_error_at_eof: bool,
    is_poison: bool,
) -> bool {
    !matches!(
        current_kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace))
    ) && !terminal_error_at_eof
        && !is_poison
}

pub(super) const fn unsupported_lambda_body_consumes_val_kind(
    first_kind: LexemeKind,
    second_kind: Option<LexemeKind>,
) -> bool {
    matches!(
        first_kind,
        LexemeKind::Token(TokenKind::Keyword(Keyword::Const))
    ) && matches!(
        second_kind,
        Some(LexemeKind::Token(TokenKind::Keyword(Keyword::Val)))
    )
}

impl Parser<'_> {
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

        let current = loop {
            let current = self.current()?;
            let action = if matches!(
                current.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace))
            ) {
                lambda_body_loop_action(current.kind(), false)
            } else {
                lambda_body_loop_action(current.kind(), outer_stops.contains_hard(current))
            };
            match action {
                LambdaBodyLoopAction::Stop => break current,
                LambdaBodyLoopAction::SkipSemicolon => {
                    self.bump()?;
                    continue;
                }
                LambdaBodyLoopAction::ParseElement => {}
            }
            #[cfg(test)]
            {
                self.lambda_body_dispatch_iterations += 1;
            }
            let before = self.index;
            let diagnostics_before = self.diagnostics.len();
            let element = if self.explicit_block_start(current)? {
                self.parse_explicit_block_statement(outer_stops)?
            } else if matches!(
                current.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
            ) {
                self.parse_bare_block_error(codes::UNSUPPORTED_LAMBDA_BODY_FORM, outer_stops)?
            } else if self.borrow_local_start()? {
                self.parse_borrow_local(expression_stops)?
            } else if self.local_destructuring_start(Keyword::Val) {
                let val_span = self.bump()?.span();
                self.parse_local_destructuring(val_span, expression_stops)?
            } else if self.local_destructuring_start(Keyword::Var)
                || self.const_local_destructuring_start()
            {
                self.parse_unsupported_local_destructuring(expression_stops)?
            } else if matches!(
                current.kind(),
                LexemeKind::Token(TokenKind::Keyword(Keyword::While | Keyword::For))
            ) || (self.current_identifier_is("loop")?
                && self.peek_is_symbol(1, Symbol::LeftBrace))
            {
                self.parse_loop_statement(outer_stops)?
            } else if matches!(
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
                self.add_statement(span, Statement::LocalVariable { declaration })?
            } else if self.is_unsupported_block_element()? {
                self.parse_unsupported_lambda_body_form()?
            } else {
                if self.is_poison_kind(current.kind()) {
                    let span = self.bump()?.span();
                    self.add_statement(span, Statement::Error)?
                } else if is_lambda_body_unsupported_symbol_kind(current.kind()) {
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
            if self.diagnostics.len() == diagnostics_before {
                self.check_block_element_separator(element, outer_stops)?;
            }
            elements.push(element);
        };

        let end = if matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace))
        ) {
            self.bump()?.span().end()
        } else {
            if lambda_body_missing_closer_emits_diagnostic(
                current.kind(),
                self.lexical_recoveries.terminal_error_at_eof,
                self.is_poison_kind(current.kind()),
            ) {
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
        let second_kind = if matches!(
            first_lexeme.kind(),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Const))
        ) {
            self.peek(0).map(Lexeme::kind)
        } else {
            None
        };
        if unsupported_lambda_body_consumes_val_kind(first_lexeme.kind(), second_kind) {
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
}
