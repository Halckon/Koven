use super::{class::ClassifierHeaderOwner, *};

impl Parser<'_> {
    pub(super) fn parse_type_ref(
        &mut self,
        stops: TypeStops,
    ) -> Result<TypeRefId, ParserInternalError> {
        self.parse_type_ref_with_completion(stops).map(|(id, _)| id)
    }

    /// Whether the TypeRef reached its own syntactic end before returning.
    /// A caller can use this to keep poison inside an incomplete type out of its own suffix.
    pub(super) fn parse_type_ref_with_completion(
        &mut self,
        stops: TypeStops,
    ) -> Result<(TypeRefId, bool), ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_type_ref_inner(stops);
        self.recursion_depth -= 1;
        result
    }

    fn parse_type_ref_inner(
        &mut self,
        stops: TypeStops,
    ) -> Result<(TypeRefId, bool), ParserInternalError> {
        if (self.current_identifier_is("move")? && self.peek_is_symbol(1, Symbol::LeftParen))
            || self.current_is_symbol(Symbol::LeftParen)
        {
            return self.parse_function_type(stops);
        }
        let (raw, current) = self
            .significant(0)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        if let Some(owner) = stops.supertype_owner() {
            if stops.proven_type_closer().is_none()
                && self.at_supertype_recovery_boundary(raw, current, owner, true)?
            {
                let span = self.empty_at(current.span().start())?;
                self.emit(
                    codes::EXPECTED_TYPE_REFERENCE,
                    "expected type reference",
                    span,
                )?;
                return self
                    .add_type_ref(span, TypeRef::Error)
                    .map(|id| (id, false));
            }
            if matches!(
                current.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBracket | Symbol::Less))
            ) {
                let end = self.recover_type_element(
                    stops.supertype_element_recovery(owner),
                    stops.proven_type_closer(),
                )?;
                self.emit(
                    codes::EXPECTED_TYPE_REFERENCE,
                    "expected type reference",
                    current.span(),
                )?;
                return self
                    .add_type_ref(self.span(current.span().start(), end)?, TypeRef::Error)
                    .map(|id| (id, false));
            }
        }
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
                return self
                    .consume_segmented_string_type_poison(recovery_end)
                    .map(|id| (id, false));
            }
            if let Some(owner_end) = self.lexical_recoveries.string_owner_end(opener) {
                let span = current.span();
                self.emit(
                    codes::EXPECTED_TYPE_REFERENCE,
                    "expected type reference",
                    span,
                )?;
                return self
                    .consume_segmented_string_type_poison(owner_end)
                    .map(|id| (id, false));
            }
        }
        let boundary = self.at_type_recovery_boundary(current, stops)?;
        if !self.current_is_identifier() || boundary {
            if boundary {
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
                return self
                    .add_type_ref(span, TypeRef::Error)
                    .map(|id| (id, false));
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
            return self
                .add_type_ref(span, TypeRef::Error)
                .map(|id| (id, false));
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

    fn parse_qualified_type(
        &mut self,
        stops: TypeStops,
    ) -> Result<(TypeRefId, bool), ParserInternalError> {
        let first = self.bump()?.span();
        let mut segments = vec![TypePathSegment {
            name_span: first,
            arguments: Vec::new(),
            argument_errors: Vec::new(),
        }];
        let mut end = first.end();

        while self.current_is_symbol(Symbol::Dot) {
            if stops.ends_at_extension_name()
                && self.peek_is_identifier(1)
                && self.peek_is_symbol(2, Symbol::LeftParen)
            {
                break;
            }
            let dot = self.bump()?.span();
            if !self.current_is_identifier()
                || (stops.has_class_file_boundary() && self.at_class_header_file_boundary(true)?)
            {
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
                return self
                    .add_type_ref(self.span(first.start(), error_end)?, TypeRef::Error)
                    .map(|id| (id, false));
            }
            let name_span = self.bump()?.span();
            end = name_span.end();
            segments.push(TypePathSegment {
                name_span,
                arguments: Vec::new(),
                argument_errors: Vec::new(),
            });
        }

        let mut complete = true;
        if self.current_is_symbol(Symbol::Less) {
            let opener = self.bump()?.span();
            let mut arguments = Vec::new();
            let mut argument_errors = Vec::new();
            let argument_stops = stops
                .without_block_elements()
                .without_extension_name()
                .with(TypeStops::COMMA)
                .with(TypeStops::GREATER);
            loop {
                let (argument, complete) = self.parse_type_ref_with_completion(argument_stops)?;
                arguments.push(argument);
                if let Some(owner) = argument_stops.supertype_owner()
                    && let Some(span) =
                        self.recover_supertype_type_element_tail(argument_stops, owner, complete)?
                {
                    argument_errors.push(span);
                }
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                break;
            }
            if self.current_is_symbol(Symbol::Greater) {
                end = self.bump()?.span().end();
            } else {
                complete = false;
                let current = self.current()?;
                if !self.is_poison() {
                    self.emit_closing(self.type_boundary_span(current, stops)?, opener)?;
                }
                if stops.has_class_body_boundary() {
                    end = self.previous_significant_end();
                } else if let Some(last) = arguments.last() {
                    end = self.type_span(*last)?.end();
                }
            }
            if let Some(last) = segments.last_mut() {
                last.arguments = arguments;
                last.argument_errors = argument_errors;
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
        .map(|id| (id, complete))
    }

    fn recover_supertype_type_element_tail(
        &mut self,
        argument_stops: TypeStops,
        owner: ClassifierHeaderOwner,
        complete: bool,
    ) -> Result<Option<Span>, ParserInternalError> {
        let current = self.current()?;
        if self.at_type_recovery_boundary(current, argument_stops)? {
            return Ok(None);
        }
        let start = current.span().start();
        let end = self.recover_type_element(
            argument_stops.supertype_element_recovery(owner),
            argument_stops.proven_type_closer(),
        )?;
        if start == end {
            return Ok(None);
        }
        if complete && !self.is_declaration_trailing_poison(current) {
            self.emit(
                codes::EXPECTED_TYPE_REFERENCE,
                "expected type reference",
                current.span(),
            )?;
        }
        self.span(start, end).map(Some)
    }

    fn parse_function_type(
        &mut self,
        outer_stops: TypeStops,
    ) -> Result<(TypeRefId, bool), ParserInternalError> {
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
            return self
                .add_type_ref(error_span, TypeRef::Error)
                .map(|id| (id, false));
        }
        let proven_parameter_close = if outer_stops.supertype_owner().is_some() {
            self.lexical_recoveries
                .angle_group_closer(self.lexed, self.current_raw()?)
                .filter(|&closer| {
                    outer_stops
                        .proven_type_closer()
                        .is_none_or(|limit| closer < limit)
                })
        } else {
            None
        };
        let opener = self.bump()?.span();
        let start = move_span.unwrap_or(opener).start();
        let mut parameters = Vec::new();
        let parameter_stops = outer_stops
            .without_block_elements()
            .with(TypeStops::COMMA)
            .with(TypeStops::RIGHT_PAREN);
        let parameter_stops = if let Some(closer) = proven_parameter_close {
            parameter_stops.for_proven_function_parameters(closer)
        } else if parameter_stops.supertype_owner().is_some() {
            parameter_stops.with(TypeStops::ARROW)
        } else {
            parameter_stops
        };
        if !self.current_is_symbol(Symbol::RightParen) {
            loop {
                if parameter_stops.proven_type_closer().is_none()
                    && let Some(owner) = parameter_stops.supertype_owner()
                {
                    let (raw, current) = self
                        .significant(0)
                        .ok_or(ParserInternalError::InvalidLexemeStream)?;
                    if self.at_supertype_recovery_boundary(raw, current, owner, true)? {
                        break;
                    }
                }
                let mode_marker =
                    self.parse_parameter_mode_marker(ParameterModeContext::FunctionType)?;
                let parameter_start = mode_marker
                    .map(parameter_mode_span)
                    .map(Span::start)
                    .unwrap_or(self.current()?.span().start());
                let (type_ref, complete) = self.parse_type_ref_with_completion(parameter_stops)?;
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
                let trailing_error = match parameter_stops.supertype_owner() {
                    Some(owner) => {
                        self.recover_supertype_type_element_tail(parameter_stops, owner, complete)?
                    }
                    None => None,
                };
                let parameter_end = trailing_error.map_or(parameter_end, Span::end);
                parameters.push(FunctionTypeParameter {
                    span: self.span(parameter_start, parameter_end)?,
                    mode_marker,
                    type_ref,
                    trailing_error,
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
        let has_right_paren = self.current_is_symbol(Symbol::RightParen)
            && (outer_stops.proven_type_closer().is_none()
                || Some(self.current_raw()?) != outer_stops.proven_type_closer());
        if has_right_paren {
            last_end = self.bump()?.span().end();
        } else {
            let current = self.current()?;
            if !self.is_poison() {
                self.emit_closing(self.type_boundary_span(current, outer_stops)?, opener)?;
            }
        }

        let (arrow_span, return_type, has_complete_return_type) =
            if self.current_is_symbol(Symbol::Arrow) {
                let span = self.bump()?.span();
                last_end = span.end();
                let (return_type, complete) = self.parse_type_ref_with_completion(outer_stops)?;
                (span, return_type, complete)
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
                    false,
                )
            };
        let return_span = self.type_span(return_type)?;
        let end = if outer_stops.has_class_body_boundary() {
            self.previous_significant_end()
        } else {
            return_span.end().max(last_end)
        };
        self.add_type_ref(
            self.span(start, end)?,
            TypeRef::Function {
                move_span,
                parameters,
                arrow_span,
                return_type,
            },
        )
        .map(|id| (id, has_right_paren && has_complete_return_type))
    }
}
