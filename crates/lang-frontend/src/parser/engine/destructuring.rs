use super::*;

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
        self.current_is_keyword(keyword)
            && self.peek(1).is_some_and(|lexeme| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen))
                )
            })
    }

    pub(super) fn const_local_destructuring_start(&self) -> bool {
        self.current_is_keyword(Keyword::Const)
            && self.peek(1).is_some_and(|lexeme| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Keyword(Keyword::Val))
                )
            })
            && self.peek(2).is_some_and(|lexeme| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen))
                )
            })
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
        let mut comma_already_reported_as_empty = false;
        let mut needs_binding = true;

        loop {
            let current = self.current()?;
            if self.current_is_symbol(Symbol::RightParen) {
                if bindings.is_empty() {
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
                if needs_binding {
                    let missing = self.empty_at(current.span().start())?;
                    if !comma_already_reported_as_empty {
                        self.emit(
                            codes::EXPECTED_DESTRUCTURING_BINDING,
                            "expected destructuring binding",
                            missing,
                        )?;
                        bindings.push(NameMarker::Missing(missing));
                    }
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
                comma_already_reported_as_empty = true;
                needs_binding = true;
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
            comma_already_reported_as_empty = false;
            needs_binding = false;

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
                needs_binding = true;
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
                needs_binding = true;
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
                    needs_binding = true;
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
                needs_binding = true;
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
        if self.current_is_symbol(Symbol::Equal) {
            let equals = self.bump()?.span();
            let current = self.current()?;
            if stops.contains(current) || matches!(current.kind(), LexemeKind::Eof) {
                let empty = self.empty_at(current.span().start())?;
                let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                    && self.lexical_recoveries.terminal_error_at_eof;
                if !terminal_lexer_root && !self.is_poison() {
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
            let end = self.recover_declaration_region(
                DeclarationStops::from_expression_hard(stops).with(DeclarationStops::BLOCK_ELEMENT),
            )?;
            let span = self.span(start, end.max(current.span().end()))?;
            return Ok((Some(equals), self.add_expression(span, Expression::Error)?));
        }

        let current = self.current()?;
        let boundary = stops.contains(current) || matches!(current.kind(), LexemeKind::Eof);
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
        let end = self.recover_declaration_region(
            DeclarationStops::from_expression_hard(stops).with(DeclarationStops::BLOCK_ELEMENT),
        )?;
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
