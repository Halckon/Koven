use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DestructuringEvent {
    EmptyComma,
    Binding,
    SeparatorComma,
    MissingSeparatorIdent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DestructuringExitKind {
    RightParen,
    Boundary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DestructuringLoopState {
    pub(super) has_bindings: bool,
    pub(super) comma_already_reported_as_empty: bool,
    pub(super) needs_binding: bool,
}

impl DestructuringLoopState {
    pub(super) const fn new() -> Self {
        Self {
            has_bindings: false,
            comma_already_reported_as_empty: false,
            needs_binding: true,
        }
    }
}

pub(super) const fn is_local_destructuring_start_kinds(
    keyword: Keyword,
    first: LexemeKind,
    second: Option<LexemeKind>,
) -> bool {
    matches!(first, LexemeKind::Token(TokenKind::Keyword(k)) if k as u8 == keyword as u8)
        && matches!(
            second,
            Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)))
        )
}

pub(super) const fn is_const_local_destructuring_start_kinds(
    first: LexemeKind,
    second: Option<LexemeKind>,
    third: Option<LexemeKind>,
) -> bool {
    matches!(first, LexemeKind::Token(TokenKind::Keyword(Keyword::Const)))
        && matches!(
            second,
            Some(LexemeKind::Token(TokenKind::Keyword(Keyword::Val)))
        )
        && matches!(
            third,
            Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)))
        )
}

pub(super) const fn destructuring_exit_emits_missing_binding(
    exit: DestructuringExitKind,
    state: DestructuringLoopState,
) -> bool {
    match exit {
        DestructuringExitKind::RightParen => !state.has_bindings,
        DestructuringExitKind::Boundary => {
            state.needs_binding && !state.comma_already_reported_as_empty
        }
    }
}

pub(super) const fn step_destructuring_loop(
    state: DestructuringLoopState,
    event: DestructuringEvent,
) -> DestructuringLoopState {
    match event {
        DestructuringEvent::EmptyComma => DestructuringLoopState {
            has_bindings: true,
            comma_already_reported_as_empty: true,
            needs_binding: true,
        },
        DestructuringEvent::Binding => DestructuringLoopState {
            has_bindings: true,
            comma_already_reported_as_empty: false,
            needs_binding: false,
        },
        DestructuringEvent::SeparatorComma | DestructuringEvent::MissingSeparatorIdent => {
            DestructuringLoopState {
                needs_binding: true,
                ..state
            }
        }
    }
}

#[cfg(any(test, kani))]
pub(super) fn run_destructuring_loop(
    initial: DestructuringLoopState,
    events: &[DestructuringEvent],
) -> DestructuringLoopState {
    let mut state = initial;
    let mut index = 0usize;
    while index < events.len() {
        state = step_destructuring_loop(state, events[index]);
        index += 1;
    }
    state
}

pub(super) const fn destructuring_initializer_missing_diagnostic(
    has_equals: bool,
    boundary: bool,
    starts_primary: bool,
    terminal_lexer_root: bool,
    is_poison: bool,
) -> bool {
    has_equals && boundary && !starts_primary && !terminal_lexer_root && !is_poison
}

impl Parser<'_> {
    pub(super) fn parse_local_variable_declaration(
        &mut self,
        keyword: Span,
        kind: VariableKind,
        initializer_stops: Stops,
    ) -> Result<ItemId, ParserInternalError> {
        let name = self.parse_name_marker_with_stops(
            codes::EXPECTED_DECLARATION_NAME,
            "expected declaration name",
            NameContext::LocalDeclaration,
            DeclarationStops::from_expression_hard(initializer_stops),
        )?;
        let (colon_span, type_ref) = if self.current_is_symbol(Symbol::Colon) {
            let colon = self.bump()?.span();
            let type_ref = self.parse_type_ref(
                TypeStops::from_expression(initializer_stops)
                    .with(TypeStops::LEFT_BRACE)
                    .with(TypeStops::EQUAL),
            )?;
            (Some(colon), Some(type_ref))
        } else {
            (None, None)
        };
        let (equals_span, initializer) =
            self.parse_required_block_initializer(initializer_stops)?;
        let end = self
            .expression_span(initializer)?
            .end()
            .max(marker_span(name).end())
            .max(
                type_ref
                    .map(|id| self.type_span(id).map(Span::end))
                    .transpose()?
                    .unwrap_or(keyword.end()),
            );
        self.add_item(
            self.span(keyword.start(), end)?,
            Item::Variable {
                kind,
                name,
                colon_span,
                type_ref,
                equals_span,
                initializer,
            },
        )
    }

    pub(super) fn local_destructuring_start(&self, keyword: Keyword) -> bool {
        let Some(first) = self.peek(0) else {
            return false;
        };
        if !matches!(first.kind(), LexemeKind::Token(TokenKind::Keyword(k)) if k == keyword) {
            return false;
        }
        is_local_destructuring_start_kinds(keyword, first.kind(), self.peek(1).map(Lexeme::kind))
    }

    pub(super) fn const_local_destructuring_start(&self) -> bool {
        let Some(first) = self.peek(0) else {
            return false;
        };
        if !matches!(
            first.kind(),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Const))
        ) {
            return false;
        }
        let second = self.peek(1).map(Lexeme::kind);
        if !matches!(
            second,
            Some(LexemeKind::Token(TokenKind::Keyword(Keyword::Val)))
        ) {
            return false;
        }
        is_const_local_destructuring_start_kinds(
            first.kind(),
            second,
            self.peek(2).map(Lexeme::kind),
        )
    }

    pub(super) fn parse_unsupported_destructuring_context(
        &mut self,
    ) -> Result<ItemId, ParserInternalError> {
        let first = self.bump()?.span();
        if self.current_is_keyword(Keyword::Val) {
            self.bump()?;
        }
        let opener = self.bump()?.span();
        self.emit(
            codes::UNSUPPORTED_DESTRUCTURING_CONTEXT,
            "unsupported destructuring context",
            opener,
        )?;
        let end = self.recover_declaration_region(self.root_declaration_stops())?;
        self.add_item(
            self.span(first.start(), end.max(opener.end()))?,
            Item::Error,
        )
    }

    pub(super) fn parse_unsupported_local_destructuring(
        &mut self,
        element_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let first = self.bump()?.span();
        let mut primary_end = first.end();
        if self.current_is_keyword(Keyword::Val) {
            primary_end = self.bump()?.span().end();
        }
        let primary = self.span(first.start(), primary_end)?;
        self.emit(
            codes::UNSUPPORTED_DESTRUCTURING_FORM,
            "unsupported destructuring form",
            primary,
        )?;
        let end = self.recover_declaration_region(
            DeclarationStops::from_expression_hard(element_stops)
                .with(DeclarationStops::BLOCK_ELEMENT),
        )?;
        self.add_statement(
            self.span(first.start(), end.max(primary_end))?,
            Statement::Error,
        )
    }

    pub(super) fn parse_local_destructuring(
        &mut self,
        val_span: Span,
        initializer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let left_paren_span = self.bump()?.span();
        let mut bindings = Vec::new();
        let mut loop_state = DestructuringLoopState::new();

        loop {
            let current = self.current()?;
            if self.current_is_symbol(Symbol::RightParen) {
                if destructuring_exit_emits_missing_binding(
                    DestructuringExitKind::RightParen,
                    loop_state,
                ) {
                    let missing = self.empty_at(current.span().start())?;
                    self.emit(
                        codes::EXPECTED_DESTRUCTURING_BINDING,
                        "expected destructuring binding",
                        missing,
                    )?;
                    bindings.push(NameMarker::Missing(missing));
                }
                break;
            }
            if self.current_is_symbol(Symbol::Equal)
                || initializer_stops.contains(current)
                || matches!(current.kind(), LexemeKind::Eof)
            {
                if destructuring_exit_emits_missing_binding(
                    DestructuringExitKind::Boundary,
                    loop_state,
                ) {
                    let missing = self.empty_at(current.span().start())?;
                    self.emit(
                        codes::EXPECTED_DESTRUCTURING_BINDING,
                        "expected destructuring binding",
                        missing,
                    )?;
                    bindings.push(NameMarker::Missing(missing));
                }
                break;
            }
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                self.emit(
                    codes::EXPECTED_DESTRUCTURING_BINDING,
                    "expected destructuring binding",
                    comma,
                )?;
                bindings.push(NameMarker::Missing(self.empty_at(comma.start())?));
                loop_state = step_destructuring_loop(loop_state, DestructuringEvent::EmptyComma);
                if self.current_is_symbol(Symbol::RightParen) {
                    break;
                }
                continue;
            }

            let binding = if self.current_is_identifier() {
                let span = self.bump()?.span();
                if self.sources.slice(span)? == "_" {
                    self.emit(
                        codes::UNSUPPORTED_DESTRUCTURING_FORM,
                        "unsupported destructuring form",
                        span,
                    )?;
                    NameMarker::Error(span)
                } else {
                    NameMarker::Present(span)
                }
            } else if self.is_declaration_trailing_poison(current) {
                let first = current.span();
                let end = self.recover_destructuring_region(initializer_stops)?;
                NameMarker::Error(self.span(first.start(), end.max(first.end()))?)
            } else {
                let first = current.span();
                self.emit(
                    codes::UNSUPPORTED_DESTRUCTURING_FORM,
                    "unsupported destructuring form",
                    first,
                )?;
                let end = self.recover_destructuring_region(initializer_stops)?;
                NameMarker::Error(self.span(first.start(), end.max(first.end()))?)
            };
            bindings.push(binding);
            loop_state = step_destructuring_loop(loop_state, DestructuringEvent::Binding);

            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                if self.current_is_symbol(Symbol::RightParen) {
                    self.emit(
                        codes::UNSUPPORTED_DESTRUCTURING_TRAILING_COMMA,
                        "unsupported destructuring trailing comma",
                        comma,
                    )?;
                    break;
                }
                loop_state =
                    step_destructuring_loop(loop_state, DestructuringEvent::SeparatorComma);
                continue;
            }
            let current = self.current()?;
            if self.current_is_symbol(Symbol::RightParen)
                || self.current_is_symbol(Symbol::Equal)
                || initializer_stops.contains(current)
                || matches!(current.kind(), LexemeKind::Eof)
            {
                break;
            }
            if self.current_is_identifier() {
                let missing = self.empty_at(current.span().start())?;
                self.emit(
                    codes::EXPECTED_DESTRUCTURING_SEPARATOR,
                    "expected destructuring separator",
                    missing,
                )?;
                loop_state =
                    step_destructuring_loop(loop_state, DestructuringEvent::MissingSeparatorIdent);
                continue;
            }
            if self.current_is_symbol(Symbol::Colon) || self.current_is_symbol(Symbol::LeftParen) {
                self.emit(
                    codes::UNSUPPORTED_DESTRUCTURING_FORM,
                    "unsupported destructuring form",
                    current.span(),
                )?;
                self.recover_destructuring_region(initializer_stops)?;
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    loop_state =
                        step_destructuring_loop(loop_state, DestructuringEvent::SeparatorComma);
                }
                continue;
            }
            if !self.is_declaration_trailing_poison(current) {
                self.emit(
                    codes::EXPECTED_DESTRUCTURING_SEPARATOR,
                    "expected destructuring separator",
                    current.span(),
                )?;
            }
            self.recover_destructuring_region(initializer_stops)?;
            if self.current_is_symbol(Symbol::Comma) {
                self.bump()?;
                loop_state =
                    step_destructuring_loop(loop_state, DestructuringEvent::SeparatorComma);
            }
        }

        let right_paren_span = if self.current_is_symbol(Symbol::RightParen) {
            Some(self.bump()?.span())
        } else {
            let current = self.current()?;
            let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                && self.lexical_recoveries.terminal_error_at_eof;
            if !terminal_lexer_root && !self.is_poison() {
                self.emit_closing(self.empty_at(current.span().start())?, left_paren_span)?;
            }
            None
        };

        if self.current_is_symbol(Symbol::Colon) {
            let colon = self.current()?.span();
            self.emit(
                codes::UNSUPPORTED_DESTRUCTURING_FORM,
                "unsupported destructuring form",
                colon,
            )?;
            self.recover_destructuring_region(initializer_stops)?;
        }

        let suppress_missing_separator =
            right_paren_span.is_none() && !self.current_is_symbol(Symbol::Equal);
        let (equals_span, initializer) =
            self.parse_destructuring_initializer(initializer_stops, suppress_missing_separator)?;
        let initializer_span = self.expression_span(initializer)?;
        let end = if initializer_span.is_empty() {
            self.previous_significant_end().max(val_span.end())
        } else {
            initializer_span.end()
        };
        self.add_statement(
            self.span(val_span.start(), end)?,
            Statement::LocalDestructuring {
                val_span,
                left_paren_span,
                bindings,
                right_paren_span,
                equals_span,
                initializer,
            },
        )
    }

    pub(super) fn recover_destructuring_region(
        &mut self,
        outer_stops: Stops,
    ) -> Result<usize, ParserInternalError> {
        self.recover_declaration_region(
            DeclarationStops::from_expression_hard(outer_stops)
                .with(DeclarationStops::COMMA)
                .with(DeclarationStops::RIGHT_PAREN)
                .with(DeclarationStops::EQUAL),
        )
    }

    pub(super) fn parse_destructuring_initializer(
        &mut self,
        stops: Stops,
        suppress_missing_separator: bool,
    ) -> Result<(Option<Span>, ExpressionId), ParserInternalError> {
        let stops = stops.with(Stops::SEMICOLON);
        let recovery_stops = DeclarationStops::from_expression_hard(stops)
            .with(DeclarationStops::BLOCK_ELEMENT | DeclarationStops::SEMICOLON);
        let equals = if self.current_is_symbol(Symbol::Equal) {
            Some(self.bump()?.span())
        } else {
            None
        };
        let current = self.current()?;
        let boundary = stops.contains(current);
        if let Some(equals) = equals {
            // These block element stops also start a primary after a real '='.
            let starts_primary = control_expression_start_kind(current.kind())
                || matches!(
                    current.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
                );
            if boundary && !starts_primary {
                let empty = self.empty_at(current.span().start())?;
                let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                    && self.lexical_recoveries.terminal_error_at_eof;
                if destructuring_initializer_missing_diagnostic(
                    true,
                    boundary,
                    starts_primary,
                    terminal_lexer_root,
                    self.is_poison(),
                ) {
                    self.emit(
                        codes::EXPECTED_DESTRUCTURING_INITIALIZER,
                        "expected destructuring initializer",
                        empty,
                    )?;
                }
                return Ok((Some(equals), self.add_expression(empty, Expression::Error)?));
            }
            if self.can_start_expression(current) {
                let initializer = self.parse_expression_bp(0, stops)?;
                let initializer = if self.initializer_boundary(initializer)? {
                    initializer
                } else {
                    self.consume_expression_tail(initializer, stops)?
                };
                return Ok((Some(equals), initializer));
            }
            if self.is_poison() {
                let span = self.bump()?.span();
                return Ok((Some(equals), self.add_expression(span, Expression::Error)?));
            }
            self.emit(
                codes::EXPECTED_DESTRUCTURING_INITIALIZER,
                "expected destructuring initializer",
                current.span(),
            )?;
            let start = current.span().start();
            let end = self.recover_declaration_region(recovery_stops)?;
            let span = self.span(start, end.max(current.span().end()))?;
            return Ok((Some(equals), self.add_expression(span, Expression::Error)?));
        }

        let primary = if boundary || self.can_start_expression(current) {
            self.empty_at(current.span().start())?
        } else {
            current.span()
        };
        let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
            && self.lexical_recoveries.terminal_error_at_eof;
        if !suppress_missing_separator && !self.is_poison() && !terminal_lexer_root {
            self.emit(
                codes::EXPECTED_DESTRUCTURING_INITIALIZER_SEPARATOR,
                "expected destructuring initializer separator",
                primary,
            )?;
        }
        if self.can_start_expression(current) {
            let initializer = self.parse_expression_bp(0, stops)?;
            let initializer = self.consume_expression_tail(initializer, stops)?;
            return Ok((None, initializer));
        }
        if boundary {
            return Ok((None, self.add_expression(primary, Expression::Error)?));
        }
        if self.is_poison() {
            let span = self.bump()?.span();
            return Ok((None, self.add_expression(span, Expression::Error)?));
        }
        let start = current.span().start();
        let end = self.recover_declaration_region(recovery_stops)?;
        let span = self.span(start, end.max(current.span().end()))?;
        Ok((None, self.add_expression(span, Expression::Error)?))
    }

    pub(super) fn parse_required_block_initializer(
        &mut self,
        stops: Stops,
    ) -> Result<(Span, ExpressionId), ParserInternalError> {
        if self.current_is_symbol(Symbol::Equal) {
            let equals = self.bump()?.span();
            let initializer = self.parse_expression_bp(0, stops)?;
            let initializer = if self.initializer_boundary(initializer)? {
                initializer
            } else {
                self.consume_expression_tail(initializer, stops)?
            };
            return Ok((equals, initializer));
        }

        let current = self.current()?;
        let boundary = stops.contains(current);
        let primary = if boundary {
            self.empty_at(current.span().start())?
        } else {
            current.span()
        };
        let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
            && self.lexical_recoveries.terminal_error_at_eof;
        if !self.is_poison_kind(current.kind()) && !terminal_lexer_root {
            self.emit(codes::EXPECTED_INITIALIZER, "expected initializer", primary)?;
        }
        let equals = self.empty_at(current.span().start())?;
        if self.can_start_expression(current) {
            let initializer = self.parse_expression_bp(0, stops)?;
            let initializer = self.consume_expression_tail(initializer, stops)?;
            return Ok((equals, initializer));
        }
        if boundary {
            return Ok((equals, self.add_expression(primary, Expression::Error)?));
        }
        let span = self.bump()?.span();
        Ok((equals, self.add_expression(span, Expression::Error)?))
    }
}
