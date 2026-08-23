use super::*;

impl Parser<'_> {
    pub(super) fn parse_variable_declaration(
        &mut self,
        keyword: Span,
        kind: VariableKind,
        declaration_stops: Stops,
    ) -> Result<ItemId, ParserInternalError> {
        let name = self.parse_name_marker(
            codes::EXPECTED_DECLARATION_NAME,
            "expected declaration name",
            NameContext::Declaration,
        )?;
        let (colon_span, type_ref) = self.parse_optional_type_annotation(declaration_stops)?;
        let (equals_span, initializer) = self.parse_required_initializer(declaration_stops)?;
        let end = self.expression_span(initializer)?.end().max(keyword.end());
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

    pub(super) fn parse_constant_declaration(
        &mut self,
        declaration_stops: Stops,
    ) -> Result<ItemId, ParserInternalError> {
        let const_span = self.bump()?.span();
        let val_marker = if self.current_is_keyword(Keyword::Val) {
            NameMarker::Present(self.bump()?.span())
        } else {
            let current = self.current()?;
            let primary = if matches!(current.kind(), LexemeKind::Eof)
                || self.is_file_declaration_boundary(current)
            {
                self.empty_at(current.span().start())?
            } else {
                current.span()
            };
            if !self.is_poison_kind(current.kind()) {
                self.emit(
                    codes::EXPECTED_VAL_AFTER_CONST,
                    "expected 'val' after 'const'",
                    primary,
                )?;
            }
            if self.file_mode && self.is_file_declaration_boundary(current) {
                NameMarker::Missing(self.empty_at(current.span().start())?)
            } else if self.current_is_keyword(Keyword::Var) {
                NameMarker::Error(self.bump()?.span())
            } else if self.current_is_identifier()
                || matches!(current.kind(), LexemeKind::Token(TokenKind::StringStart))
                || self.current_is_symbol(Symbol::Colon)
                || self.current_is_symbol(Symbol::Equal)
                || matches!(current.kind(), LexemeKind::Eof)
            {
                // 保留 StringStart，让名称恢复一次消费完整 lexical owner。
                NameMarker::Missing(self.empty_at(current.span().start())?)
            } else {
                NameMarker::Error(self.bump()?.span())
            }
        };
        // 嵌套 constant 的名称恢复不能越过成员调用方拥有的 hard closer。
        let name_stops = DeclarationStops::from_expression_hard(declaration_stops);
        let name_stops = if self.file_mode {
            name_stops.union(DeclarationStops::FILE)
        } else {
            name_stops
        };
        let name = self.parse_name_marker_with_stops(
            codes::EXPECTED_DECLARATION_NAME,
            "expected declaration name",
            NameContext::Declaration,
            name_stops,
        )?;
        let (colon_span, type_ref) = self.parse_optional_type_annotation(declaration_stops)?;
        let (equals_span, initializer) = self.parse_required_initializer(declaration_stops)?;
        let end = self
            .expression_span(initializer)?
            .end()
            .max(const_span.end());
        self.add_item(
            self.span(const_span.start(), end)?,
            Item::Constant {
                const_span,
                val_marker,
                name,
                colon_span,
                type_ref,
                equals_span,
                initializer,
            },
        )
    }

    pub(super) fn parse_function_declaration(
        &mut self,
        declaration_stops: Stops,
    ) -> Result<ItemId, ParserInternalError> {
        let fun_span = self.bump()?.span();
        let (type_parameters, type_parameter_list_span) = self.parse_type_parameters()?;
        let name = self.parse_name_marker(
            codes::EXPECTED_DECLARATION_NAME,
            "expected declaration name",
            NameContext::Declaration,
        )?;
        let parameters = self.parse_value_parameters()?;
        let parameter_end = self.previous_significant_end().max(fun_span.end());
        let (form, end) = self.parse_function_form(parameter_end, declaration_stops)?;
        self.add_item(
            self.span(fun_span.start(), end)?,
            Item::Function {
                name,
                type_parameters,
                type_parameter_list_span,
                parameters,
                form,
            },
        )
    }

    pub(super) fn parse_function_form(
        &mut self,
        parameter_end: usize,
        declaration_stops: Stops,
    ) -> Result<(FunctionForm, usize), ParserInternalError> {
        if self.current_is_symbol(Symbol::Colon) {
            let colon_span = self.bump()?.span();
            let type_ref = self.parse_type_ref(
                TypeStops::from_expression(declaration_stops)
                    .with(TypeStops::EQUAL)
                    .with(TypeStops::LEFT_BRACE),
            )?;
            return self.finish_explicit_function_form(colon_span, type_ref, declaration_stops);
        }

        if self.current_is_symbol(Symbol::Equal) {
            let insertion = self.empty_at(self.current()?.span().start())?;
            self.emit(
                codes::EXPECTED_RETURN_TYPE,
                "expected explicit return type",
                insertion,
            )?;
            let type_ref = self.add_type_ref(insertion, TypeRef::Error)?;
            return self.finish_explicit_function_form(insertion, type_ref, declaration_stops);
        }

        if self.current_is_symbol(Symbol::LeftBrace) {
            let block = self.parse_block_statement(Stops::ROOT)?;
            let end = self.statement_span(block)?.end();
            return Ok((FunctionForm::ImplicitUnitBlock(block), end));
        }

        let current = self.current()?;
        if self.can_start_type_ref(current) {
            let colon_span = self.empty_at(current.span().start())?;
            self.emit(
                codes::EXPECTED_RETURN_TYPE,
                "expected explicit return type",
                current.span(),
            )?;
            let type_ref = self.parse_type_ref(
                TypeStops::from_expression(declaration_stops)
                    .with(TypeStops::EQUAL)
                    .with(TypeStops::LEFT_BRACE),
            )?;
            return self.finish_explicit_function_form(colon_span, type_ref, declaration_stops);
        }

        Ok((FunctionForm::ImplicitUnitAbsent, parameter_end))
    }

    pub(super) fn finish_explicit_function_form(
        &mut self,
        colon_span: Span,
        type_ref: TypeRefId,
        declaration_stops: Stops,
    ) -> Result<(FunctionForm, usize), ParserInternalError> {
        let body = if self.current_is_symbol(Symbol::Equal) {
            let equals_span = self.bump()?.span();
            let expression = self.parse_expression_bp(0, declaration_stops)?;
            FunctionBody::Expression {
                equals_span,
                expression,
            }
        } else if self.current_is_symbol(Symbol::LeftBrace) {
            FunctionBody::Block(self.parse_block_statement(Stops::ROOT)?)
        } else {
            FunctionBody::Absent
        };
        let end = match body {
            FunctionBody::Absent => self.previous_significant_end(),
            FunctionBody::Expression { expression, .. } => self.expression_span(expression)?.end(),
            FunctionBody::Block(statement) => self.statement_span(statement)?.end(),
        };
        Ok((
            FunctionForm::Explicit {
                colon_span,
                type_ref,
                body,
            },
            end,
        ))
    }

    pub(super) fn parse_name_marker(
        &mut self,
        code: &str,
        message: &'static str,
        context: NameContext,
    ) -> Result<NameMarker, ParserInternalError> {
        let stops = if self.file_mode && matches!(context, NameContext::Declaration) {
            DeclarationStops::FILE
        } else {
            DeclarationStops::EMPTY
        };
        self.parse_name_marker_with_stops(code, message, context, stops)
    }

    pub(super) fn parse_name_marker_with_stops(
        &mut self,
        code: &str,
        message: &'static str,
        context: NameContext,
        additional_stops: DeclarationStops,
    ) -> Result<NameMarker, ParserInternalError> {
        if self.current_is_identifier() {
            return Ok(NameMarker::Present(self.bump()?.span()));
        }
        let current = self.current()?;
        if self.is_poison() {
            return Ok(NameMarker::Error(self.bump()?.span()));
        }
        let symbol = match current.kind() {
            LexemeKind::Token(TokenKind::Symbol(symbol)) => Some(symbol),
            _ => None,
        };
        let boundary = matches!(current.kind(), LexemeKind::Eof)
            || context.is_stop(current)
            || additional_stops.contains_hard(current, symbol)
            || additional_stops.contains_soft(current, symbol);
        let primary = if boundary {
            self.empty_at(current.span().start())?
        } else {
            current.span()
        };
        self.emit(code, message, primary)?;
        if boundary {
            Ok(NameMarker::Missing(primary))
        } else {
            let start = current.span().start();
            let end =
                self.recover_declaration_region(context.recovery_stops().union(additional_stops))?;
            Ok(NameMarker::Error(self.span(start, end.max(start))?))
        }
    }

    pub(super) fn parse_optional_type_annotation(
        &mut self,
        declaration_stops: Stops,
    ) -> Result<(Option<Span>, Option<TypeRefId>), ParserInternalError> {
        if !self.current_is_symbol(Symbol::Colon) {
            return Ok((None, None));
        }
        let colon = self.bump()?.span();
        let type_ref = self
            .parse_type_ref(TypeStops::from_expression(declaration_stops).with(TypeStops::EQUAL))?;
        Ok((Some(colon), Some(type_ref)))
    }

    pub(super) fn parse_required_initializer(
        &mut self,
        declaration_stops: Stops,
    ) -> Result<(Span, ExpressionId), ParserInternalError> {
        if self.current_is_symbol(Symbol::Equal) {
            let equals = self.bump()?.span();
            let initializer = self.parse_expression_bp(0, declaration_stops)?;
            return Ok((equals, initializer));
        }
        let current = self.current()?;
        let boundary = declaration_stops.contains(current);
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
            let initializer = self.parse_expression_bp(0, declaration_stops)?;
            return Ok((equals, initializer));
        }
        let error_span = if boundary {
            primary
        } else {
            let start = current.span().start();
            let recovery_stops = self
                .root_declaration_stops()
                .union(DeclarationStops::from_expression_hard(declaration_stops));
            let end = self.recover_declaration_region(recovery_stops)?;
            self.span(start, end.max(start))?
        };
        let initializer = self.add_expression(error_span, Expression::Error)?;
        Ok((equals, initializer))
    }

    pub(super) fn parse_type_parameters(
        &mut self,
    ) -> Result<(Vec<TypeParameter>, Option<Span>), ParserInternalError> {
        if !self.current_is_symbol(Symbol::Less) {
            return Ok((Vec::new(), None));
        }
        let opener = self.bump()?.span();
        let mut parameters = Vec::new();
        let stops = TypeStops::empty()
            .with(TypeStops::COMMA)
            .with(TypeStops::GREATER);
        if self.current_is_symbol(Symbol::Greater) {
            self.emit(
                codes::EXPECTED_LIST_ELEMENT,
                "expected list element",
                self.current()?.span(),
            )?;
        } else {
            loop {
                if self.current_is_symbol(Symbol::Comma) {
                    let comma = self.bump()?.span();
                    self.emit(codes::EXPECTED_LIST_ELEMENT, "expected list element", comma)?;
                    if self.current_is_symbol(Symbol::Greater) {
                        break;
                    }
                    continue;
                }
                let name = self.parse_name_marker(
                    codes::EXPECTED_PARAMETER_NAME,
                    "expected parameter name",
                    NameContext::TypeParameter,
                )?;
                let start = marker_span(name).start();
                let (colon_span, bound) = if self.current_is_symbol(Symbol::Colon) {
                    let colon = self.bump()?.span();
                    let bound = self.parse_type_ref(stops)?;
                    (Some(colon), Some(bound))
                } else {
                    (None, None)
                };
                let end = bound
                    .map(|id| self.type_span(id).map(Span::end))
                    .transpose()?
                    .unwrap_or(marker_span(name).end());
                parameters.push(TypeParameter {
                    span: self.span(start, end)?,
                    name,
                    colon_span,
                    bound,
                });
                if self.current_is_symbol(Symbol::Comma) {
                    let comma = self.bump()?.span();
                    if self.current_is_symbol(Symbol::Greater) {
                        self.emit(
                            codes::UNSUPPORTED_TRAILING_COMMA,
                            "unsupported trailing comma",
                            comma,
                        )?;
                        break;
                    }
                    continue;
                }
                if self.current_is_symbol(Symbol::Greater) {
                    break;
                }
                if self.current_is_identifier()
                    && matches!(
                        self.peek(1).map(Lexeme::kind),
                        Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)))
                    )
                {
                    let current = self.current()?;
                    self.emit_closing(self.empty_at(current.span().start())?, opener)?;
                    let end = parameters
                        .last()
                        .map(|parameter| parameter.span.end())
                        .unwrap_or(opener.end());
                    return Ok((parameters, Some(self.span(opener.start(), end)?)));
                }
                if self.current_is_identifier() {
                    self.emit(
                        codes::EXPECTED_LIST_SEPARATOR,
                        "expected list separator",
                        self.current()?.span(),
                    )?;
                    continue;
                }
                if matches!(self.current()?.kind(), LexemeKind::Eof) {
                    break;
                }
                if self.is_poison() {
                    self.bump()?;
                    if self.current_is_identifier() {
                        self.emit(
                            codes::EXPECTED_LIST_SEPARATOR,
                            "expected list separator",
                            self.current()?.span(),
                        )?;
                        continue;
                    }
                    if matches!(self.current()?.kind(), LexemeKind::Eof)
                        || self.current_is_symbol(Symbol::Greater)
                    {
                        break;
                    }
                }
                let primary = self.current()?.span();
                self.emit(
                    codes::EXPECTED_LIST_SEPARATOR,
                    "expected list separator",
                    primary,
                )?;
                self.recover_declaration_region(
                    DeclarationStops::EMPTY
                        .with(DeclarationStops::COMMA)
                        .with(DeclarationStops::GREATER),
                )?;
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                break;
            }
        }
        let end = if self.current_is_symbol(Symbol::Greater) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            parameters
                .last()
                .map(|parameter| parameter.span.end())
                .unwrap_or(opener.end())
        };
        Ok((parameters, Some(self.span(opener.start(), end)?)))
    }

    pub(super) fn parse_parameter_mode_marker(
        &mut self,
    ) -> Result<Option<ParameterModeMarker>, ParserInternalError> {
        let marker = if self.current_is_keyword(Keyword::Own) {
            Some(ParameterModeMarker::Own(self.bump()?.span()))
        } else if self.current_is_keyword(Keyword::Borrow) {
            Some(ParameterModeMarker::Borrow(self.bump()?.span()))
        } else if self.current_is_keyword(Keyword::Inout) {
            Some(ParameterModeMarker::Inout(self.bump()?.span()))
        } else {
            None
        };
        if marker.is_none() {
            return Ok(None);
        }
        while self.current_is_keyword(Keyword::Own)
            || self.current_is_keyword(Keyword::Borrow)
            || self.current_is_keyword(Keyword::Inout)
        {
            let duplicate = self.bump()?.span();
            self.emit(
                codes::DUPLICATE_PARAMETER_MODE,
                "duplicate parameter mode",
                duplicate,
            )?;
        }
        Ok(marker)
    }

    pub(super) fn parse_value_parameters(
        &mut self,
    ) -> Result<Vec<ValueParameter>, ParserInternalError> {
        if !self.current_is_symbol(Symbol::LeftParen) {
            let current = self.current()?;
            let span = self.empty_at(current.span().start())?;
            self.emit_closing(span, span)?;
            return Ok(Vec::new());
        }
        let opener = self.bump()?.span();
        let mut parameters = Vec::new();
        let stops = TypeStops::empty()
            .with(TypeStops::COMMA)
            .with(TypeStops::RIGHT_PAREN);
        while !self.current_is_symbol(Symbol::RightParen)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                self.emit(codes::EXPECTED_LIST_ELEMENT, "expected list element", comma)?;
                continue;
            }
            let mode_marker = self.parse_parameter_mode_marker()?;
            let name = self.parse_name_marker(
                codes::EXPECTED_PARAMETER_NAME,
                "expected parameter name",
                NameContext::ValueParameter,
            )?;
            let start = mode_marker
                .map(parameter_mode_span)
                .unwrap_or_else(|| marker_span(name))
                .start();
            let mut last_consumed_end = mode_marker.map(|marker| {
                self.previous_significant_end()
                    .max(parameter_mode_span(marker).end())
            });
            let name_span = marker_span(name);
            if !name_span.is_empty() {
                last_consumed_end = Some(last_consumed_end.unwrap_or(0).max(name_span.end()));
            }
            let colon_span = if self.current_is_symbol(Symbol::Colon) {
                self.bump()?.span()
            } else {
                let current = self.current()?;
                let insertion = self.empty_at(current.span().start())?;
                let primary =
                    if self.can_start_type_ref(current) || self.current_is_symbol(Symbol::Equal) {
                        current.span()
                    } else if self.current_is_symbol(Symbol::Comma)
                        || self.current_is_symbol(Symbol::RightParen)
                        || matches!(current.kind(), LexemeKind::Eof)
                    {
                        insertion
                    } else {
                        current.span()
                    };
                if !self.is_poison_kind(current.kind()) {
                    self.emit(
                        codes::EXPECTED_PARAMETER_COLON,
                        "expected ':' after parameter name",
                        primary,
                    )?;
                }
                insertion
            };
            if !colon_span.is_empty() {
                last_consumed_end = Some(last_consumed_end.unwrap_or(0).max(colon_span.end()));
            }
            let missing_colon_default =
                self.current_is_symbol(Symbol::Equal) && colon_span.is_empty();
            let type_ref = if self.current_is_symbol(Symbol::Equal)
                || self.current_is_symbol(Symbol::Comma)
                || self.current_is_symbol(Symbol::RightParen)
                || matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                self.add_type_ref(
                    self.empty_at(self.current()?.span().start())?,
                    TypeRef::Error,
                )?
            } else if self.can_start_type_ref(self.current()?) {
                self.parse_type_ref(stops)?
            } else {
                let start = self.current()?.span().start();
                let end = self.recover_declaration_region(
                    DeclarationStops::EMPTY
                        .with(DeclarationStops::COMMA)
                        .with(DeclarationStops::RIGHT_PAREN)
                        .with(DeclarationStops::LEFT_BRACE),
                )?;
                self.add_type_ref(self.span(start, end.max(start))?, TypeRef::Error)?
            };
            let type_span = self.type_span(type_ref)?;
            if !type_span.is_empty() {
                last_consumed_end = Some(last_consumed_end.unwrap_or(0).max(type_span.end()));
            }
            let end = last_consumed_end.unwrap_or(type_span.end());
            parameters.push(ValueParameter {
                span: self.span(start, end)?,
                mode_marker,
                name,
                colon_span,
                type_ref,
            });
            if self.current_is_symbol(Symbol::Equal) {
                let equals = self.current()?.span();
                let end = self.recover_declaration_region(
                    DeclarationStops::EMPTY
                        .with(DeclarationStops::COMMA)
                        .with(DeclarationStops::RIGHT_PAREN),
                )?;
                if !missing_colon_default {
                    self.emit(
                        codes::UNSUPPORTED_PARAMETER_DEFAULT,
                        "unsupported parameter default",
                        self.span(equals.start(), end.max(equals.end()))?,
                    )?;
                }
            }
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                if self.current_is_symbol(Symbol::RightParen) {
                    self.emit(
                        codes::UNSUPPORTED_TRAILING_COMMA,
                        "unsupported trailing comma",
                        comma,
                    )?;
                    break;
                }
                continue;
            }
            if self.current_is_identifier() {
                self.emit(
                    codes::EXPECTED_LIST_SEPARATOR,
                    "expected list separator",
                    self.current()?.span(),
                )?;
                continue;
            }
            if self.is_poison() {
                self.bump()?;
                if self.current_is_identifier() {
                    self.emit(
                        codes::EXPECTED_LIST_SEPARATOR,
                        "expected list separator",
                        self.current()?.span(),
                    )?;
                    continue;
                }
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                if self.current_is_symbol(Symbol::RightParen)
                    || matches!(self.current()?.kind(), LexemeKind::Eof)
                {
                    break;
                }
            }
            if !self.current_is_symbol(Symbol::RightParen)
                && !matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                self.emit(
                    codes::EXPECTED_LIST_SEPARATOR,
                    "expected list separator",
                    self.current()?.span(),
                )?;
                self.recover_declaration_region(
                    DeclarationStops::EMPTY
                        .with(DeclarationStops::COMMA)
                        .with(DeclarationStops::RIGHT_PAREN),
                )?;
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
            }
            break;
        }
        if self.current_is_symbol(Symbol::RightParen) {
            self.bump()?;
        } else {
            let current = self.current()?;
            let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                && self.lexical_recoveries.terminal_error_at_eof;
            if !terminal_lexer_root {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
        }
        Ok(parameters)
    }

    /// 消费声明恢复区，保留调用方拥有的 hard closer，并只在局部 delimiter / owner
    /// 回到 baseline 后识别逗号 soft stop。
    pub(super) fn recover_declaration_region(
        &mut self,
        stops: DeclarationStops,
    ) -> Result<usize, ParserInternalError> {
        let mut delimiters = Vec::new();
        let mut owners: Vec<(RecoveryOwner, usize)> = Vec::new();
        let first = self
            .significant(0)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        let recovery_start = first.1.span().start();
        while let Some(event) = self
            .lexical_recoveries
            .terminal_owner_events
            .get(self.next_terminal_recovery_event)
        {
            #[cfg(test)]
            {
                self.declaration_recovery_event_queries_and_applications += 1;
            }
            if event.offset > recovery_start {
                break;
            }
            self.next_terminal_recovery_event += 1;
        }
        let mut next_terminal_event = self.next_terminal_recovery_event;
        let mut end = recovery_start;
        let mut next_significant = Some(first);

        'recovery: loop {
            let (raw_index, current) = if let Some(first) = next_significant.take() {
                first
            } else {
                self.significant(0)
                    .ok_or(ParserInternalError::InvalidLexemeStream)?
            };
            #[cfg(test)]
            {
                self.declaration_recovery_raw_visits += raw_index - self.index + 1;
            }
            self.index = raw_index;

            while let Some(event) = {
                #[cfg(test)]
                {
                    self.declaration_recovery_event_queries_and_applications += 1;
                }
                self.lexical_recoveries
                    .terminal_owner_events
                    .get(next_terminal_event)
                    .copied()
            } {
                if event.offset > current.span().start() {
                    break;
                }
                #[cfg(test)]
                {
                    self.declaration_recovery_event_queries_and_applications += 1;
                }
                let Some((owner, delimiter_baseline)) = owners.last().copied() else {
                    // 恢复可以从一个由调用方拥有的 string / interpolation
                    // 内部开始。其 terminal event 是该调用方的 hard boundary，
                    // 不是当前错误区域缺少局部 owner。
                    break 'recovery;
                };
                if owner.kind() != event.kind || owner.opener() != event.opener {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                owners.pop();
                delimiters.truncate(delimiter_baseline);
                next_terminal_event += 1;
            }
            if matches!(current.kind(), LexemeKind::Eof) {
                break;
            }

            let symbol = match current.kind() {
                LexemeKind::Token(TokenKind::Symbol(symbol)) => Some(symbol),
                _ => None,
            };
            if let Some(expected) = delimiters.last().copied()
                && symbol == Some(expected)
            {
                self.index = raw_index + 1;
                end = current.span().end();
                delimiters.pop();
                continue;
            }
            let outside_owner = owners.is_empty();
            let hard_stop = outside_owner && stops.contains_hard(current, symbol);
            if hard_stop
                || (outside_owner && delimiters.is_empty() && stops.contains_soft(current, symbol))
            {
                break;
            }

            match current.kind() {
                LexemeKind::Token(TokenKind::StringStart) => {
                    let opener = current.span();
                    owners.push((
                        RecoveryOwner::String {
                            opener: opener.start(),
                        },
                        delimiters.len(),
                    ))
                }
                LexemeKind::Token(TokenKind::StringEnd) => {
                    let Some((RecoveryOwner::String { .. }, delimiter_baseline)) = owners.pop()
                    else {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    };
                    delimiters.truncate(delimiter_baseline);
                }
                LexemeKind::Token(TokenKind::InterpolationStart) => owners.push((
                    RecoveryOwner::Interpolation {
                        opener: current.span().start(),
                    },
                    delimiters.len(),
                )),
                LexemeKind::Token(TokenKind::InterpolationEnd) => {
                    let Some((RecoveryOwner::Interpolation { .. }, delimiter_baseline)) =
                        owners.pop()
                    else {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    };
                    delimiters.truncate(delimiter_baseline);
                }
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => {
                    delimiters.push(Symbol::RightParen)
                }
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBracket)) => {
                    delimiters.push(Symbol::RightBracket)
                }
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) => {
                    delimiters.push(Symbol::RightBrace)
                }
                _ => {}
            }
            self.index = raw_index + 1;
            end = current.span().end();
        }
        self.next_terminal_recovery_event = next_terminal_event;
        Ok(end)
    }

    pub(super) fn consume_declaration_tail(
        &mut self,
        item: ItemId,
    ) -> Result<ItemId, ParserInternalError> {
        if matches!(self.current()?.kind(), LexemeKind::Eof) {
            return Ok(item);
        }
        let current = self.current()?;
        let implicit_function = matches!(
            self.ast.items().get(item)?.payload(),
            Item::Function {
                form: FunctionForm::ImplicitUnitAbsent,
                ..
            }
        );
        if !implicit_function || !self.is_declaration_trailing_poison(current) {
            self.emit(
                codes::UNEXPECTED_TRAILING_TOKEN,
                "unexpected trailing token",
                current.span(),
            )?;
        }
        self.recover_declaration_region(DeclarationStops::EMPTY)?;
        Ok(item)
    }
}
