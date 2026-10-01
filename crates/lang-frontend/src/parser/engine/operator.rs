use super::*;

impl Parser<'_> {
    pub(super) fn unsupported_operator(&self) -> Result<Option<Span>, ParserInternalError> {
        let Some(first) = self.peek(0) else {
            return Ok(None);
        };
        let Some(second) = self.peek(1) else {
            return Ok(None);
        };
        if first.span().end() != second.span().start() {
            return Ok(None);
        }
        let first_symbol = match first.kind() {
            LexemeKind::Token(TokenKind::Symbol(symbol)) => symbol,
            _ => return Ok(None),
        };
        let second_symbol = match second.kind() {
            LexemeKind::Token(TokenKind::Symbol(symbol)) => symbol,
            _ => return Ok(None),
        };
        let unsupported = matches!(
            (first_symbol, second_symbol),
            (Symbol::Plus, Symbol::Plus)
                | (Symbol::Minus, Symbol::Minus)
                | (Symbol::Less, Symbol::Less)
                | (Symbol::Greater, Symbol::Greater)
                | (Symbol::DotDot, Symbol::Dot)
        );
        if !unsupported {
            return Ok(None);
        }
        Ok(Some(self.span(first.span().start(), second.span().end())?))
    }

    pub(super) fn can_start_expression(&self, lexeme: Lexeme) -> bool {
        matches!(
            lexeme.kind(),
            LexemeKind::Token(
                TokenKind::Identifier
                    | TokenKind::IntegerLiteral(_)
                    | TokenKind::FloatLiteral(_)
                    | TokenKind::CharLiteral
                    | TokenKind::StringStart
            ) | LexemeKind::Token(TokenKind::Keyword(
                Keyword::True
                    | Keyword::False
                    | Keyword::Null
                    | Keyword::This
                    | Keyword::If
                    | Keyword::When
                    | Keyword::Return
                    | Keyword::Break
                    | Keyword::Continue
                    | Keyword::Super
            )) | LexemeKind::Token(TokenKind::Symbol(
                Symbol::LeftParen
                    | Symbol::LeftBrace
                    | Symbol::ColonColon
                    | Symbol::Bang
                    | Symbol::Plus
                    | Symbol::Minus
            )) | LexemeKind::Invalid(_)
                | LexemeKind::Token(TokenKind::ReservedWord(_))
        )
    }

    pub(super) fn can_start_type_ref(&self, lexeme: Lexeme) -> bool {
        matches!(
            lexeme.kind(),
            LexemeKind::Token(TokenKind::Identifier)
                | LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen))
        )
    }

    pub(super) fn is_poison_kind(&self, kind: LexemeKind) -> bool {
        matches!(
            kind,
            LexemeKind::Invalid(_) | LexemeKind::Token(TokenKind::ReservedWord(_))
        )
    }

    /// 声明尾随恢复必须让 Lexer 已拥有的错误区域保持唯一根因。
    pub(super) fn is_declaration_trailing_poison(&self, lexeme: Lexeme) -> bool {
        self.is_poison_kind(lexeme.kind())
            || (matches!(lexeme.kind(), LexemeKind::Token(TokenKind::StringStart))
                && self
                    .lexical_recoveries
                    .string_recovery_end(lexeme.span().start())
                    .is_some())
            || (matches!(lexeme.kind(), LexemeKind::Token(TokenKind::StringStart))
                && self
                    .lexical_recoveries
                    .lexical_poison_string_recovery_end(lexeme.span().start())
                    .is_some())
    }

    pub(super) fn infix_rule(
        &self,
        lexeme: Lexeme,
    ) -> Result<Option<InfixRule>, ParserInternalError> {
        let rule = match lexeme.kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::Star)) => InfixRule::left(
                PREC_MULTIPLICATIVE,
                InfixKind::Binary(BinaryOperator::Multiply),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Slash)) => InfixRule::left(
                PREC_MULTIPLICATIVE,
                InfixKind::Binary(BinaryOperator::Divide),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Percent)) => InfixRule::left(
                PREC_MULTIPLICATIVE,
                InfixKind::Binary(BinaryOperator::Remainder),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)) => {
                InfixRule::left(PREC_ADDITIVE, InfixKind::Binary(BinaryOperator::Add))
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)) => {
                InfixRule::left(PREC_ADDITIVE, InfixKind::Binary(BinaryOperator::Subtract))
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::DotDot)) => InfixRule::non_associative(
                PREC_RANGE,
                InfixKind::Binary(BinaryOperator::InclusiveRange),
                NonAssociativeGroup::Range,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::DotDotLess)) => InfixRule::non_associative(
                PREC_RANGE,
                InfixKind::Binary(BinaryOperator::ExclusiveRange),
                NonAssociativeGroup::Range,
            ),
            LexemeKind::Token(TokenKind::Identifier) => {
                let name = self.sources.slice(lexeme.span())?;
                match name {
                    "to" => InfixRule::left(PREC_TO, InfixKind::Binary(BinaryOperator::To)),
                    "shl" => InfixRule::left(PREC_SHIFT, InfixKind::Binary(BinaryOperator::Shl)),
                    "shr" => InfixRule::left(PREC_SHIFT, InfixKind::Binary(BinaryOperator::Shr)),
                    "ushr" => InfixRule::left(PREC_SHIFT, InfixKind::Binary(BinaryOperator::Ushr)),
                    "and" => InfixRule::left(
                        PREC_BITWISE_AND,
                        InfixKind::Binary(BinaryOperator::BitwiseAnd),
                    ),
                    "xor" => InfixRule::left(
                        PREC_BITWISE_XOR,
                        InfixKind::Binary(BinaryOperator::BitwiseXor),
                    ),
                    "or" => InfixRule::left(
                        PREC_BITWISE_OR,
                        InfixKind::Binary(BinaryOperator::BitwiseOr),
                    ),
                    _ => return Ok(None),
                }
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::QuestionColon)) => {
                InfixRule::right(PREC_ELVIS, InfixKind::Binary(BinaryOperator::Elvis))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::In)) => InfixRule::non_associative(
                PREC_MEMBERSHIP,
                InfixKind::Binary(BinaryOperator::In),
                NonAssociativeGroup::Membership,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangIn)) => InfixRule::non_associative(
                PREC_MEMBERSHIP,
                InfixKind::Binary(BinaryOperator::NotIn),
                NonAssociativeGroup::Membership,
            ),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Is)) => InfixRule::non_associative(
                PREC_MEMBERSHIP,
                InfixKind::TypeTest { negated: false },
                NonAssociativeGroup::Membership,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs)) => InfixRule::non_associative(
                PREC_MEMBERSHIP,
                InfixKind::TypeTest { negated: true },
                NonAssociativeGroup::Membership,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Less)) => InfixRule::non_associative(
                PREC_COMPARISON,
                InfixKind::Binary(BinaryOperator::Less),
                NonAssociativeGroup::Comparison,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)) => InfixRule::non_associative(
                PREC_COMPARISON,
                InfixKind::Binary(BinaryOperator::Greater),
                NonAssociativeGroup::Comparison,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::LessEqual)) => InfixRule::non_associative(
                PREC_COMPARISON,
                InfixKind::Binary(BinaryOperator::LessEqual),
                NonAssociativeGroup::Comparison,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::GreaterEqual)) => {
                InfixRule::non_associative(
                    PREC_COMPARISON,
                    InfixKind::Binary(BinaryOperator::GreaterEqual),
                    NonAssociativeGroup::Comparison,
                )
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::EqualEqual)) => InfixRule::non_associative(
                PREC_EQUALITY,
                InfixKind::Binary(BinaryOperator::Equal),
                NonAssociativeGroup::Equality,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangEqual)) => InfixRule::non_associative(
                PREC_EQUALITY,
                InfixKind::Binary(BinaryOperator::NotEqual),
                NonAssociativeGroup::Equality,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::AndAnd)) => {
                InfixRule::left(PREC_AND, InfixKind::Binary(BinaryOperator::LogicalAnd))
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::OrOr)) => {
                InfixRule::left(PREC_OR, InfixKind::Binary(BinaryOperator::LogicalOr))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::As)) => {
                InfixRule::left(PREC_CAST, InfixKind::Cast(CastOperator::As))
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::AsQuestion)) => {
                InfixRule::left(PREC_CAST, InfixKind::Cast(CastOperator::SafeAs))
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::Assign),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::PlusEqual)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::AddAssign),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::MinusEqual)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::SubtractAssign),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::StarEqual)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::MultiplyAssign),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::SlashEqual)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::DivideAssign),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::PercentEqual)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::RemainderAssign),
            ),
            _ => return Ok(None),
        };
        Ok(Some(rule))
    }

    pub(super) fn build_infix(
        &mut self,
        left: ExpressionId,
        operator_span: Span,
        rule: InfixRule,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let left_span = self.expression_span(left)?;
        match rule.kind {
            InfixKind::Cast(operator) => {
                let type_ref = self.parse_type_ref(TypeStops::from_expression(stops))?;
                let type_span = self.type_span(type_ref)?;
                self.add_expression(
                    self.span(left_span.start(), type_span.end())?,
                    Expression::Cast {
                        expression: left,
                        operator,
                        operator_span,
                        type_ref,
                    },
                )
            }
            InfixKind::TypeTest { negated } => {
                let type_ref = self.parse_type_ref(TypeStops::from_expression(stops))?;
                let type_span = self.type_span(type_ref)?;
                self.add_expression(
                    self.span(left_span.start(), type_span.end())?,
                    Expression::TypeTest {
                        expression: left,
                        negated,
                        operator_span,
                        type_ref,
                    },
                )
            }
            InfixKind::Binary(operator) => {
                let right = self.parse_expression_bp(rule.right_precedence, stops)?;
                let right_span = self.expression_span(right)?;
                self.add_expression(
                    self.span(left_span.start(), right_span.end())?,
                    Expression::Binary {
                        left,
                        operator,
                        operator_span,
                        right,
                    },
                )
            }
            InfixKind::Assignment(operator) => {
                let right = self.parse_expression_bp(rule.right_precedence, stops)?;
                let right_span = self.expression_span(right)?;
                self.add_expression(
                    self.span(left_span.start(), right_span.end())?,
                    Expression::Assignment {
                        target: left,
                        operator,
                        operator_span,
                        value: right,
                    },
                )
            }
        }
    }

    pub(super) fn consume_rule_right(
        &mut self,
        rule: InfixRule,
        stops: Stops,
    ) -> Result<ParsedRight, ParserInternalError> {
        let parsed = match rule.kind {
            InfixKind::Cast(_) | InfixKind::TypeTest { .. } => {
                ParsedRight::Type(self.parse_type_ref(TypeStops::from_expression(stops))?)
            }
            InfixKind::Binary(_) | InfixKind::Assignment(_) => {
                ParsedRight::Expression(self.parse_expression_bp(rule.right_precedence, stops)?)
            }
        };
        Ok(parsed)
    }
}
