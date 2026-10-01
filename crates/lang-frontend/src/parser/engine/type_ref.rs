use super::*;

impl Parser<'_> {
    pub(super) fn parse_type_ref(
        &mut self,
        stops: TypeStops,
    ) -> Result<TypeRefId, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_type_ref_inner(stops);
        self.recursion_depth -= 1;
        result
    }

    pub(super) fn parse_type_ref_inner(
        &mut self,
        stops: TypeStops,
    ) -> Result<TypeRefId, ParserInternalError> {
        if (self.current_identifier_is("move")? && self.peek_is_symbol(1, Symbol::LeftParen))
            || self.current_is_symbol(Symbol::LeftParen)
        {
            return self.parse_function_type(stops);
        }
        let current = self.current()?;
        let string_start = matches!(current.kind(), LexemeKind::Token(TokenKind::StringStart));
        if string_start {
            let opener = current.span().start();
            let lexical_poison_end =
                self.lexical_recoveries
                    .string_recovery_end(opener)
                    .or_else(|| {
                        self.lexical_recoveries
                            .lexical_poison_string_recovery_end(opener)
                    });
            if let Some(recovery_end) = lexical_poison_end {
                return self.consume_segmented_string_type_poison(recovery_end);
            }
            if let Some(owner_end) = self.lexical_recoveries.string_owner_end(opener) {
                let span = current.span();
                self.emit(
                    codes::EXPECTED_TYPE_REFERENCE,
                    "expected type reference",
                    span,
                )?;
                return self.consume_segmented_string_type_poison(owner_end);
            }
        }
        if !self.current_is_identifier() {
            if stops.contains(current) || matches!(current.kind(), LexemeKind::Eof) {
                let span = self.empty_at(current.span().start())?;
                let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                    && self.lexical_recoveries.terminal_error_at_eof;
                if !terminal_lexer_root {
                    self.emit(
                        codes::EXPECTED_TYPE_REFERENCE,
                        "expected type reference",
                        span,
                    )?;
                }
                return self.add_type_ref(span, TypeRef::Error);
            }
            let span = self.bump()?.span();
            if !matches!(
                current.kind(),
                LexemeKind::Invalid(_) | LexemeKind::Token(TokenKind::ReservedWord(_))
            ) {
                self.emit(
                    codes::EXPECTED_TYPE_REFERENCE,
                    "expected type reference",
                    span,
                )?;
            }
            return self.add_type_ref(span, TypeRef::Error);
        }

        self.parse_qualified_type(stops)
    }

    pub(super) fn consume_segmented_string_type_poison(
        &mut self,
        recovery_end: usize,
    ) -> Result<TypeRefId, ParserInternalError> {
        let opener = self.bump()?.span();
        let start = opener.start();
        let mut end = opener.end();

        while end < recovery_end {
            let current = self.current()?;
            if matches!(current.kind(), LexemeKind::Eof) {
                break;
            }
            end = self.bump()?.span().end();
        }

        self.add_type_ref(self.span(start, end)?, TypeRef::Error)
    }

    pub(super) fn parse_qualified_type(
        &mut self,
        stops: TypeStops,
    ) -> Result<TypeRefId, ParserInternalError> {
        let first = self.bump()?.span();
        let mut segments = vec![TypePathSegment {
            name_span: first,
            arguments: Vec::new(),
        }];
        let mut end = first.end();

        while self.current_is_symbol(Symbol::Dot) {
            let dot = self.bump()?.span();
            if !self.current_is_identifier() {
                let current = self.current()?;
                let error_end = if self.is_poison() {
                    self.bump()?.span().end()
                } else {
                    let primary = self.type_boundary_span(current, stops)?;
                    self.emit(
                        codes::EXPECTED_TYPE_REFERENCE,
                        "expected type reference",
                        primary,
                    )?;
                    dot.end()
                };
                return self.add_type_ref(self.span(first.start(), error_end)?, TypeRef::Error);
            }
            let name_span = self.bump()?.span();
            end = name_span.end();
            segments.push(TypePathSegment {
                name_span,
                arguments: Vec::new(),
            });
        }

        if self.current_is_symbol(Symbol::Less) {
            let opener = self.bump()?.span();
            let mut arguments = Vec::new();
            let argument_stops = stops
                .without_block_elements()
                .with(TypeStops::COMMA)
                .with(TypeStops::GREATER);
            loop {
                arguments.push(self.parse_type_ref(argument_stops)?);
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                break;
            }
            if self.current_is_symbol(Symbol::Greater) {
                end = self.bump()?.span().end();
            } else {
                let current = self.current()?;
                if !self.is_poison() {
                    self.emit_closing(self.type_boundary_span(current, stops)?, opener)?;
                }
                if let Some(last) = arguments.last() {
                    end = self.type_span(*last)?.end();
                }
            }
            if let Some(last) = segments.last_mut() {
                last.arguments = arguments;
            }
        }

        let nullable_span = if self.current_is_symbol(Symbol::Question) {
            let span = self.bump()?.span();
            end = span.end();
            Some(span)
        } else {
            None
        };
        self.add_type_ref(
            self.span(first.start(), end)?,
            TypeRef::Qualified {
                segments,
                nullable_span,
            },
        )
    }

    pub(super) fn parse_function_type(
        &mut self,
        outer_stops: TypeStops,
    ) -> Result<TypeRefId, ParserInternalError> {
        let move_span = if self.current_identifier_is("move")? {
            Some(self.bump()?.span())
        } else {
            None
        };
        if !self.current_is_symbol(Symbol::LeftParen) {
            let current = self.current()?;
            let poison = self.is_poison();
            let start = move_span
                .map(Span::start)
                .unwrap_or_else(|| current.span().start());
            let error_span =
                if outer_stops.contains(current) || matches!(current.kind(), LexemeKind::Eof) {
                    match move_span {
                        Some(span) => span,
                        None => self.empty_at(current.span().start())?,
                    }
                } else {
                    let consumed = self.bump()?.span();
                    self.span(start, consumed.end())?
                };
            if !poison {
                let diagnostic_span = self.type_boundary_span(current, outer_stops)?;
                self.emit(
                    codes::EXPECTED_TYPE_REFERENCE,
                    "expected type reference",
                    diagnostic_span,
                )?;
            }
            return self.add_type_ref(error_span, TypeRef::Error);
        }
        let opener = self.bump()?.span();
        let start = move_span.unwrap_or(opener).start();
        let mut parameters = Vec::new();
        let parameter_stops = outer_stops
            .without_block_elements()
            .with(TypeStops::COMMA)
            .with(TypeStops::RIGHT_PAREN);
        if !self.current_is_symbol(Symbol::RightParen) {
            loop {
                let mode_marker =
                    self.parse_parameter_mode_marker(ParameterModeContext::FunctionType)?;
                let parameter_start = mode_marker
                    .map(parameter_mode_span)
                    .map(Span::start)
                    .unwrap_or(self.current()?.span().start());
                let type_ref = self.parse_type_ref(parameter_stops)?;
                let type_span = self.type_span(type_ref)?;
                let marker_end = mode_marker.map(|marker| {
                    self.previous_significant_end()
                        .max(parameter_mode_span(marker).end())
                });
                let parameter_end = if type_span.is_empty() {
                    marker_end.unwrap_or(type_span.end())
                } else {
                    marker_end.unwrap_or(0).max(type_span.end())
                };
                parameters.push(FunctionTypeParameter {
                    span: self.span(parameter_start, parameter_end)?,
                    mode_marker,
                    type_ref,
                });
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                break;
            }
        }
        let mut last_end = parameters
            .last()
            .map(|parameter| parameter.span.end())
            .unwrap_or(opener.end());
        if self.current_is_symbol(Symbol::RightParen) {
            last_end = self.bump()?.span().end();
        } else {
            let current = self.current()?;
            if !self.is_poison() {
                self.emit_closing(self.type_boundary_span(current, outer_stops)?, opener)?;
            }
        }

        let (arrow_span, return_type) = if self.current_is_symbol(Symbol::Arrow) {
            let span = self.bump()?.span();
            last_end = span.end();
            let return_type = self.parse_type_ref(outer_stops)?;
            (span, return_type)
        } else {
            let current = self.current()?;
            let primary = self.type_boundary_span(current, outer_stops)?;
            let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                && self.lexical_recoveries.terminal_error_at_eof;
            if !self.is_poison() && !terminal_lexer_root {
                self.emit(
                    codes::EXPECTED_TYPE_REFERENCE,
                    "expected type reference",
                    primary,
                )?;
            }
            (
                self.span(last_end, last_end)?,
                self.add_type_ref(primary, TypeRef::Error)?,
            )
        };
        let return_span = self.type_span(return_type)?;
        self.add_type_ref(
            self.span(start, return_span.end().max(last_end))?,
            TypeRef::Function {
                move_span,
                parameters,
                arrow_span,
                return_type,
            },
        )
    }
}
