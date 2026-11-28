use super::*;

#[inline]
pub(super) const fn try_enter_recursion_depth(depth: usize, max_depth: usize) -> Option<usize> {
    if depth >= max_depth {
        None
    } else {
        Some(depth + 1)
    }
}

#[inline]
pub(super) fn previous_significant_end_with<T>(
    items: &[T],
    kind_of: impl Fn(&T) -> LexemeKind,
    end_of: impl Fn(&T) -> usize,
) -> usize {
    items
        .iter()
        .rev()
        .find(|item| !matches!(kind_of(item), LexemeKind::Trivia(_)))
        .map(end_of)
        .unwrap_or(0)
}

#[inline]
pub(super) const fn lambda_body_element_is_statement(
    index: usize,
    len: usize,
    is_expression_stmt: bool,
) -> bool {
    !(index == len.saturating_sub(1) && is_expression_stmt)
}

#[inline]
pub(super) const fn control_body_element_is_statement(
    value_required: bool,
    index: usize,
    len: usize,
    is_expression_stmt: bool,
) -> bool {
    !(value_required && index == len.saturating_sub(1) && is_expression_stmt)
}

#[inline]
pub(super) const fn if_missing_else_emits_diagnostic(
    has_else: bool,
    statement_allowed: bool,
) -> bool {
    !has_else && !statement_allowed
}

#[inline]
pub(super) const fn expression_tail_is_block_boundary(
    block_elements: bool,
    is_semicolon: bool,
    can_start_expr: bool,
) -> bool {
    block_elements && (is_semicolon || can_start_expr)
}

#[inline]
pub(super) const fn expression_tail_emits_unexpected_trailing(
    in_stops: bool,
    block_boundary: bool,
) -> bool {
    !in_stops && !block_boundary
}

#[inline]
pub(super) const fn boundary_uses_empty_span(in_stops: bool, at_block_sep: bool) -> bool {
    in_stops || at_block_sep
}

#[inline]
pub(super) const fn type_recovery_boundary_hit(
    in_stops: bool,
    has_block_sep: bool,
    at_block_sep: bool,
    has_class_file: bool,
    at_class_file: bool,
    at_file_angle: bool,
) -> bool {
    in_stops
        || (has_block_sep && at_block_sep)
        || (has_class_file && at_class_file)
        || at_file_angle
}

impl Parser<'_> {
    pub(super) fn significant(&self, ordinal: usize) -> Option<(usize, Lexeme)> {
        self.significant_from(self.index, ordinal)
    }

    pub(super) fn significant_from(&self, raw: usize, ordinal: usize) -> Option<(usize, Lexeme)> {
        let raw = self.strict_trials.significant_raw(raw, ordinal)?;
        let lexeme = self.lexed.lexemes().get(raw).copied()?;
        #[cfg(test)]
        self.significant_raw_visits
            .set(self.significant_raw_visits.get() + 1);
        Some((raw, lexeme))
    }

    pub(super) fn enter_recursion(&mut self) -> Result<(), ParserInternalError> {
        let Some(next_depth) = try_enter_recursion_depth(self.recursion_depth, MAX_RECURSION_DEPTH)
        else {
            return Err(ParserInternalError::NestingLimitExceeded {
                limit: MAX_RECURSION_DEPTH,
            });
        };
        self.recursion_depth = next_depth;
        Ok(())
    }

    pub(super) fn current(&self) -> Result<Lexeme, ParserInternalError> {
        self.significant(0)
            .map(|(_, lexeme)| lexeme)
            .ok_or(ParserInternalError::InvalidLexemeStream)
    }

    pub(super) fn current_raw(&self) -> Result<usize, ParserInternalError> {
        self.significant(0)
            .map(|(raw, _)| raw)
            .ok_or(ParserInternalError::InvalidLexemeStream)
    }

    pub(super) fn peek(&self, ordinal: usize) -> Option<Lexeme> {
        self.significant(ordinal).map(|(_, lexeme)| lexeme)
    }

    pub(super) fn bump(&mut self) -> Result<Lexeme, ParserInternalError> {
        let (index, lexeme) = self
            .significant(0)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        self.index = index + 1;
        Ok(lexeme)
    }

    pub(super) fn current_is_symbol(&self, symbol: Symbol) -> bool {
        matches!(
            self.peek(0).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Symbol(actual))) if actual == symbol
        )
    }

    pub(super) fn peek_is_symbol(&self, ordinal: usize, symbol: Symbol) -> bool {
        matches!(
            self.peek(ordinal).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Symbol(actual))) if actual == symbol
        )
    }

    pub(super) fn current_is_keyword(&self, keyword: Keyword) -> bool {
        matches!(
            self.peek(0).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Keyword(actual))) if actual == keyword
        )
    }

    pub(super) fn peek_is_keyword(&self, ordinal: usize, keyword: Keyword) -> bool {
        matches!(
            self.peek(ordinal).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Keyword(actual))) if actual == keyword
        )
    }

    pub(super) fn current_is_identifier(&self) -> bool {
        matches!(
            self.peek(0).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Identifier))
        )
    }

    pub(super) fn peek_is_identifier(&self, ordinal: usize) -> bool {
        matches!(
            self.peek(ordinal).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Identifier))
        )
    }

    pub(super) fn current_identifier_is(
        &self,
        expected: &str,
    ) -> Result<bool, ParserInternalError> {
        if !self.current_is_identifier() {
            return Ok(false);
        }
        Ok(self.sources.slice(self.current()?.span())? == expected)
    }

    pub(super) fn peek_identifier_is(
        &self,
        ordinal: usize,
        expected: &str,
    ) -> Result<bool, ParserInternalError> {
        let Some(lexeme) = self.peek(ordinal) else {
            return Ok(false);
        };
        if !matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier)) {
            return Ok(false);
        }
        Ok(self.sources.slice(lexeme.span())? == expected)
    }

    pub(super) fn is_poison(&self) -> bool {
        matches!(
            self.peek(0).map(Lexeme::kind),
            Some(LexemeKind::Invalid(_)) | Some(LexemeKind::Token(TokenKind::ReservedWord(_)))
        )
    }

    pub(super) fn span(&self, start: usize, end: usize) -> Result<Span, ParserInternalError> {
        Ok(self.sources.span(self.lexed.source_id(), start, end)?)
    }

    pub(super) fn empty_at(&self, offset: usize) -> Result<Span, ParserInternalError> {
        self.span(offset, offset)
    }

    pub(super) fn boundary_span(
        &self,
        current: Lexeme,
        stops: Stops,
    ) -> Result<Span, ParserInternalError> {
        let in_stops = stops.contains(current);
        let at_block_sep = !in_stops && self.at_block_statement_separator(current, stops)?;
        if boundary_uses_empty_span(in_stops, at_block_sep) {
            self.empty_at(current.span().start())
        } else {
            Ok(current.span())
        }
    }

    pub(super) fn at_block_statement_separator(
        &self,
        current: Lexeme,
        stops: Stops,
    ) -> Result<bool, ParserInternalError> {
        Ok(stops.delimiters & Stops::BLOCK_STATEMENT_SEPARATOR != 0
            && self.at_proven_block_statement_separator(current)?)
    }

    fn at_proven_block_statement_separator(
        &self,
        current: Lexeme,
    ) -> Result<bool, ParserInternalError> {
        if !matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
        ) {
            return Ok(false);
        }
        let raw = self.current_raw()?;
        Ok(self.lexed.lexemes()[raw] == current
            && self
                .lexical_recoveries
                .block_statement_follows(self.lexed, raw))
    }

    pub(super) fn at_type_recovery_boundary(
        &self,
        current: Lexeme,
        stops: TypeStops,
    ) -> Result<bool, ParserInternalError> {
        let in_stops = stops.contains(current);
        let has_block_sep = stops.has_block_statement_separator();
        let at_block_sep =
            !in_stops && has_block_sep && self.at_proven_block_statement_separator(current)?;
        let has_class_file = stops.has_class_file_boundary();
        let at_class_file = !in_stops
            && !at_block_sep
            && has_class_file
            && self.at_class_header_file_boundary(true)?;
        let at_file_angle =
            !in_stops && !at_block_sep && !at_class_file && self.at_file_angle_boundary(current)?;
        Ok(type_recovery_boundary_hit(
            in_stops,
            has_block_sep,
            at_block_sep,
            has_class_file,
            at_class_file,
            at_file_angle,
        ))
    }

    pub(super) fn type_boundary_span(
        &self,
        current: Lexeme,
        stops: TypeStops,
    ) -> Result<Span, ParserInternalError> {
        if self.at_type_recovery_boundary(current, stops)? {
            self.empty_at(current.span().start())
        } else {
            Ok(current.span())
        }
    }

    pub(super) fn previous_significant_end(&self) -> usize {
        previous_significant_end_with(
            &self.lexed.lexemes()[..self.index],
            |lexeme| lexeme.kind(),
            |lexeme| lexeme.span().end(),
        )
    }

    pub(super) fn expression_span(&self, id: ExpressionId) -> Result<Span, ParserInternalError> {
        Ok(self.ast.expressions().get(id)?.span())
    }

    pub(super) fn validate_item_context(&mut self, id: ItemId) -> Result<(), ParserInternalError> {
        self.validate_context_work(vec![ContextWork::Item(id)])
    }

    pub(super) fn expression_statement_boundary(
        &self,
        id: ExpressionId,
    ) -> Result<bool, ParserInternalError> {
        if self.current_is_symbol(Symbol::Semicolon) {
            return Ok(true);
        }
        self.gap_has_line_break(
            self.expression_span(id)?.end(),
            self.current()?.span().start(),
        )
    }

    pub(super) fn initializer_boundary(
        &self,
        id: ExpressionId,
    ) -> Result<bool, ParserInternalError> {
        if self.current_is_symbol(Symbol::Semicolon) {
            return Ok(true);
        }
        self.gap_has_line_break(
            self.expression_span(id)?.end(),
            self.current()?.span().start(),
        )
    }

    pub(super) fn validate_statement_context(
        &mut self,
        id: StatementId,
        expression_is_statement: bool,
    ) -> Result<(), ParserInternalError> {
        self.validate_context_work(vec![ContextWork::Statement {
            id,
            expression_is_statement,
        }])
    }

    pub(super) fn validate_expression_context(
        &mut self,
        id: ExpressionId,
        statement_allowed: bool,
    ) -> Result<(), ParserInternalError> {
        self.validate_context_work(vec![ContextWork::Expression {
            id,
            statement_allowed,
        }])
    }

    pub(super) fn validate_context_work(
        &mut self,
        mut work: Vec<ContextWork>,
    ) -> Result<(), ParserInternalError> {
        while let Some(current) = work.pop() {
            match current {
                ContextWork::Item(id) => {
                    let item = self.ast.items().get(id)?.payload().clone();
                    match item {
                        Item::Error => {}
                        Item::Modified { declaration, .. } => {
                            work.push(ContextWork::Item(declaration));
                        }
                        Item::Variable { initializer, .. } | Item::Constant { initializer, .. } => {
                            work.push(ContextWork::Expression {
                                id: initializer,
                                statement_allowed: false,
                            });
                        }
                        Item::Function { form, .. } => match form {
                            FunctionForm::ImplicitUnitAbsent => {}
                            FunctionForm::ImplicitUnitBlock(body) => {
                                work.push(ContextWork::Statement {
                                    id: body,
                                    expression_is_statement: true,
                                });
                            }
                            FunctionForm::Explicit { body, .. } => match body {
                                FunctionBody::Absent => {}
                                FunctionBody::Expression { expression, .. } => {
                                    work.push(ContextWork::Expression {
                                        id: expression,
                                        statement_allowed: false,
                                    });
                                }
                                FunctionBody::Block(body) => {
                                    work.push(ContextWork::Statement {
                                        id: body,
                                        expression_is_statement: true,
                                    });
                                }
                            },
                        },
                        Item::Classifier(classifier) => {
                            if let Some(body) = classifier.body {
                                for member in body.members.into_iter().rev() {
                                    work.push(ContextWork::Item(member));
                                }
                            }
                        }
                        Item::Companion(companion) => {
                            for member in companion.body.members.into_iter().rev() {
                                work.push(ContextWork::Item(member));
                            }
                        }
                        Item::Deinit { body, .. } => {
                            work.push(ContextWork::Statement {
                                id: body,
                                expression_is_statement: true,
                            });
                        }
                    }
                }
                ContextWork::Statement {
                    id,
                    expression_is_statement,
                } => {
                    let statement = self.ast.statements().get(id)?.payload().clone();
                    match statement {
                        Statement::Error => {}
                        Statement::Block { elements } | Statement::ControlBody { elements } => {
                            for element in elements.into_iter().rev() {
                                work.push(ContextWork::Statement {
                                    id: element,
                                    expression_is_statement: true,
                                });
                            }
                        }
                        Statement::LambdaBody { elements } => {
                            let len = elements.len();
                            let last = len.saturating_sub(1);
                            for (index, element) in elements.into_iter().enumerate().rev() {
                                let is_expression_stmt = index == last
                                    && matches!(
                                        self.ast.statements().get(element)?.payload(),
                                        Statement::Expression { .. }
                                    );
                                work.push(ContextWork::Statement {
                                    id: element,
                                    expression_is_statement: lambda_body_element_is_statement(
                                        index,
                                        len,
                                        is_expression_stmt,
                                    ),
                                });
                            }
                        }
                        Statement::LocalVariable { declaration } => {
                            work.push(ContextWork::Item(declaration));
                        }
                        Statement::LocalDestructuring { initializer, .. } => {
                            work.push(ContextWork::Expression {
                                id: initializer,
                                statement_allowed: false,
                            });
                        }
                        Statement::While {
                            condition, body, ..
                        } => {
                            work.push(ContextWork::Statement {
                                id: body,
                                expression_is_statement: true,
                            });
                            work.push(ContextWork::Expression {
                                id: condition,
                                statement_allowed: false,
                            });
                        }
                        Statement::For { source, body, .. } => {
                            work.push(ContextWork::Statement {
                                id: body,
                                expression_is_statement: true,
                            });
                            work.push(ContextWork::Expression {
                                id: source,
                                statement_allowed: false,
                            });
                        }
                        Statement::Loop { body, .. } => {
                            work.push(ContextWork::Statement {
                                id: body,
                                expression_is_statement: true,
                            });
                        }
                        Statement::Expression { expression } => {
                            work.push(ContextWork::Expression {
                                id: expression,
                                statement_allowed: expression_is_statement,
                            });
                        }
                    }
                }
                ContextWork::ControlBody { id, value_required } => {
                    let statement = self.ast.statements().get(id)?.payload().clone();
                    match statement {
                        Statement::ControlBody { elements } => {
                            let len = elements.len();
                            let last = len.saturating_sub(1);
                            for (index, element) in elements.into_iter().enumerate().rev() {
                                let is_expression_stmt = value_required
                                    && index == last
                                    && matches!(
                                        self.ast.statements().get(element)?.payload(),
                                        Statement::Expression { .. }
                                    );
                                work.push(ContextWork::Statement {
                                    id: element,
                                    expression_is_statement: control_body_element_is_statement(
                                        value_required,
                                        index,
                                        len,
                                        is_expression_stmt,
                                    ),
                                });
                            }
                        }
                        Statement::Expression { expression } => {
                            work.push(ContextWork::Expression {
                                id: expression,
                                statement_allowed: !value_required,
                            });
                        }
                        _ => work.push(ContextWork::Statement {
                            id,
                            expression_is_statement: true,
                        }),
                    }
                }
                ContextWork::Expression {
                    id,
                    statement_allowed,
                } => {
                    let expression = self.ast.expressions().get(id)?.payload().clone();
                    match expression {
                        Expression::Error
                        | Expression::Name
                        | Expression::This
                        | Expression::Literal(_)
                        | Expression::Break { .. }
                        | Expression::Continue { .. }
                        | Expression::SuperMember { .. } => {}
                        Expression::Group { expression }
                        | Expression::Prefix {
                            operand: expression,
                            ..
                        }
                        | Expression::Cast { expression, .. }
                        | Expression::TypeTest { expression, .. }
                        | Expression::Member {
                            receiver: expression,
                            ..
                        }
                        | Expression::NonNullAssert {
                            operand: expression,
                            ..
                        }
                        | Expression::Propagate {
                            value: expression, ..
                        } => work.push(ContextWork::Expression {
                            id: expression,
                            statement_allowed: false,
                        }),
                        Expression::String { parts } => {
                            for part in parts.into_iter().rev() {
                                if let StringPart::Interpolation { expression, .. } = part {
                                    work.push(ContextWork::Expression {
                                        id: expression,
                                        statement_allowed: false,
                                    });
                                }
                            }
                        }
                        Expression::Lambda { body, .. } => {
                            work.push(ContextWork::Statement {
                                id: body,
                                expression_is_statement: true,
                            });
                        }
                        Expression::If {
                            then_branch,
                            else_branch,
                            condition,
                            ..
                        } => {
                            if if_missing_else_emits_diagnostic(
                                else_branch.is_some(),
                                statement_allowed,
                            ) {
                                let span =
                                    self.empty_at(self.statement_span(then_branch)?.end())?;
                                self.emit(
                                    codes::EXPECTED_ELSE_BRANCH,
                                    "expected else branch",
                                    span,
                                )?;
                            }
                            if let Some(branch) = else_branch {
                                work.push(ContextWork::ControlBody {
                                    id: branch,
                                    value_required: !statement_allowed,
                                });
                            }
                            work.push(ContextWork::ControlBody {
                                id: then_branch,
                                value_required: !statement_allowed,
                            });
                            work.push(ContextWork::Expression {
                                id: condition,
                                statement_allowed: false,
                            });
                        }
                        Expression::When {
                            subject, entries, ..
                        } => {
                            for entry in entries.into_iter().rev() {
                                work.push(ContextWork::ControlBody {
                                    id: entry.body,
                                    value_required: !statement_allowed,
                                });
                                for condition in entry.conditions.into_iter().rev() {
                                    match condition {
                                        WhenCondition::Expression(expression)
                                        | WhenCondition::Contains { expression, .. } => {
                                            work.push(ContextWork::Expression {
                                                id: expression,
                                                statement_allowed: false,
                                            });
                                        }
                                        WhenCondition::TypeTest { .. } => {}
                                    }
                                }
                            }
                            if let Some(subject) = subject {
                                work.push(ContextWork::Expression {
                                    id: subject,
                                    statement_allowed: false,
                                });
                            }
                        }
                        Expression::Return { value, .. } => {
                            if let Some(value) = value {
                                work.push(ContextWork::Expression {
                                    id: value,
                                    statement_allowed: false,
                                });
                            }
                        }
                        Expression::Binary { left, right, .. }
                        | Expression::Assignment {
                            target: left,
                            value: right,
                            ..
                        }
                        | Expression::Index {
                            receiver: left,
                            index: right,
                        } => {
                            work.push(ContextWork::Expression {
                                id: right,
                                statement_allowed: false,
                            });
                            work.push(ContextWork::Expression {
                                id: left,
                                statement_allowed: false,
                            });
                        }
                        Expression::Call {
                            callee, arguments, ..
                        } => {
                            for argument in arguments.into_iter().rev() {
                                work.push(ContextWork::Expression {
                                    id: argument.value,
                                    statement_allowed: false,
                                });
                            }
                            work.push(ContextWork::Expression {
                                id: callee,
                                statement_allowed: false,
                            });
                        }
                        Expression::CallableReference { receiver, .. } => {
                            if let Some(receiver) = receiver {
                                work.push(ContextWork::Expression {
                                    id: receiver,
                                    statement_allowed: false,
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn statement_span(&self, id: StatementId) -> Result<Span, ParserInternalError> {
        Ok(self.ast.statements().get(id)?.span())
    }

    pub(super) fn type_span(&self, id: TypeRefId) -> Result<Span, ParserInternalError> {
        Ok(self.ast.type_refs().get(id)?.span())
    }

    pub(super) fn add_expression(
        &mut self,
        span: Span,
        payload: Expression,
    ) -> Result<ExpressionId, ParserInternalError> {
        Ok(self.ast.add_expression(span, payload)?)
    }

    pub(super) fn add_statement(
        &mut self,
        span: Span,
        payload: Statement,
    ) -> Result<StatementId, ParserInternalError> {
        Ok(self.ast.add_statement(span, payload)?)
    }

    pub(super) fn add_item(
        &mut self,
        span: Span,
        item: Item,
    ) -> Result<ItemId, ParserInternalError> {
        Ok(self.ast.add_item(span, item)?)
    }

    pub(super) fn add_type_ref(
        &mut self,
        span: Span,
        payload: TypeRef,
    ) -> Result<TypeRefId, ParserInternalError> {
        Ok(self.ast.add_type_ref(span, payload)?)
    }

    pub(super) fn code(&self, raw: &str) -> Result<DiagnosticCode, ParserInternalError> {
        Ok(codes::catalog()?.resolve(raw)?)
    }

    pub(super) fn emit(
        &mut self,
        raw_code: &str,
        message: &'static str,
        span: Span,
    ) -> Result<(), ParserInternalError> {
        let code = self.code(raw_code)?;
        self.diagnostics.push(Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            message,
            span,
        )?);
        Ok(())
    }

    pub(super) fn emit_closing(
        &mut self,
        primary: Span,
        opener: Span,
    ) -> Result<(), ParserInternalError> {
        let code = self.code(codes::EXPECTED_CLOSING_DELIMITER)?;
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            "expected closing delimiter",
            primary,
        )?;
        diagnostic.add_label(self.sources, opener, "opening delimiter is here")?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn consume_expression_tail(
        &mut self,
        expression: ExpressionId,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let start = self.expression_span(expression)?.start();
        let mut consumed_end = None;

        while self.is_poison() {
            let span = self.bump()?.span();
            self.add_expression(span, Expression::Error)?;
            consumed_end = Some(span.end());
        }

        // A complete block expression may recover at the next expression starter.
        // Dispatch must still reject a missing separator; this never accepts an implicit ';'.
        let is_semicolon = stops.block_elements && self.current_is_symbol(Symbol::Semicolon);
        let can_start_expr =
            stops.block_elements && !is_semicolon && self.can_start_expression(self.current()?);
        let block_boundary =
            expression_tail_is_block_boundary(stops.block_elements, is_semicolon, can_start_expr);
        if expression_tail_emits_unexpected_trailing(
            stops.contains(self.current()?),
            block_boundary,
        ) {
            let first = self.current()?.span();
            self.emit(
                codes::UNEXPECTED_TRAILING_TOKEN,
                "unexpected trailing token",
                first,
            )?;
            let error_start = first.start();
            // Tail 中可以出现完整 nested string / interpolation / delimiter owner；必须在
            // owner 回到 baseline 后识别调用方 stop，不能把内层同形 closer 当成外层边界。
            let error_end = self
                .recover_declaration_region(DeclarationStops::from_expression(stops))?
                .max(first.end());
            self.add_expression(self.span(error_start, error_end)?, Expression::Error)?;
            consumed_end = Some(error_end);
        }

        if let Some(end) = consumed_end {
            return self.add_expression(self.span(start, end)?, Expression::Error);
        }
        Ok(expression)
    }
}
