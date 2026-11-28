use super::*;

#[inline]
pub(super) const fn is_unsupported_adjacent_operator_pair(
    first: LexemeKind,
    second: LexemeKind,
    adjacent: bool,
) -> bool {
    if !adjacent {
        return false;
    }
    let first_symbol = match first {
        LexemeKind::Token(TokenKind::Symbol(symbol)) => symbol,
        _ => return false,
    };
    let second_symbol = match second {
        LexemeKind::Token(TokenKind::Symbol(symbol)) => symbol,
        _ => return false,
    };
    matches!(
        (first_symbol, second_symbol),
        (Symbol::Plus, Symbol::Plus)
            | (Symbol::Minus, Symbol::Minus)
            | (Symbol::Less, Symbol::Less)
            | (Symbol::Greater, Symbol::Greater)
            | (Symbol::DotDot, Symbol::Dot)
    )
}

#[inline]
pub(super) const fn can_start_expression_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
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

#[inline]
pub(super) const fn can_start_type_ref_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Identifier)
            | LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen))
    )
}

#[inline]
pub(super) const fn is_poison_lexeme_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Invalid(_) | LexemeKind::Token(TokenKind::ReservedWord(_))
    )
}

#[inline]
pub(super) const fn is_declaration_trailing_poison_decision(
    kind: LexemeKind,
    has_string_recovery: bool,
    has_lexical_poison_string_recovery: bool,
) -> bool {
    is_poison_lexeme_kind(kind)
        || (matches!(kind, LexemeKind::Token(TokenKind::StringStart))
            && (has_string_recovery || has_lexical_poison_string_recovery))
}

impl Parser<'_> {
    pub(super) fn unsupported_operator(&self) -> Result<Option<Span>, ParserInternalError> {
        let Some(first) = self.peek(0) else {
            return Ok(None);
        };
        let Some(second) = self.peek(1) else {
            return Ok(None);
        };
        if !is_unsupported_adjacent_operator_pair(
            first.kind(),
            second.kind(),
            first.span().end() == second.span().start(),
        ) {
            return Ok(None);
        }
        Ok(Some(self.span(first.span().start(), second.span().end())?))
    }

    pub(super) fn can_start_expression(&self, lexeme: Lexeme) -> bool {
        can_start_expression_kind(lexeme.kind())
    }

    pub(super) fn can_start_type_ref(&self, lexeme: Lexeme) -> bool {
        can_start_type_ref_kind(lexeme.kind())
    }

    pub(super) fn is_poison_kind(&self, kind: LexemeKind) -> bool {
        is_poison_lexeme_kind(kind)
    }

    /// 声明尾随恢复必须让 Lexer 已拥有的错误区域保持唯一根因。
    pub(super) fn is_declaration_trailing_poison(&self, lexeme: Lexeme) -> bool {
        let kind = lexeme.kind();
        let (has_string_recovery, has_lexical_poison_string_recovery) =
            if !is_poison_lexeme_kind(kind)
                && matches!(kind, LexemeKind::Token(TokenKind::StringStart))
            {
                let has_string = self
                    .lexical_recoveries
                    .string_recovery_end(lexeme.span().start())
                    .is_some();
                let has_lexical_poison = !has_string
                    && self
                        .lexical_recoveries
                        .lexical_poison_string_recovery_end(lexeme.span().start())
                        .is_some();
                (has_string, has_lexical_poison)
            } else {
                (false, false)
            };
        is_declaration_trailing_poison_decision(
            kind,
            has_string_recovery,
            has_lexical_poison_string_recovery,
        )
    }

    pub(super) fn infix_rule(
        &self,
        lexeme: Lexeme,
    ) -> Result<Option<InfixRule>, ParserInternalError> {
        let soft = if matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier)) {
            SoftInfixKeyword::parse(self.sources.slice(lexeme.span())?)
        } else {
            None
        };
        Ok(infix_rule_from_parts(lexeme.kind(), soft))
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
                let type_ref = self.parse_operator_type_operand(stops)?;
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
                let type_ref = self.parse_operator_type_operand(stops)?;
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
                ParsedRight::Type(self.parse_operator_type_operand(stops)?)
            }
            InfixKind::Binary(_) | InfixKind::Assignment(_) => {
                ParsedRight::Expression(self.parse_expression_bp(rule.right_precedence, stops)?)
            }
        };
        Ok(parsed)
    }

    fn parse_operator_type_operand(
        &mut self,
        stops: Stops,
    ) -> Result<TypeRefId, ParserInternalError> {
        if self.at_committed_when_arm_boundary(stops)?
            || (self.unclosed_when_operand_boundary(stops)?
                && !self.complete_function_type_starts_here()?)
        {
            let at = self.empty_at(self.current()?.span().start())?;
            self.emit(
                codes::EXPECTED_TYPE_REFERENCE,
                "expected type reference",
                at,
            )?;
            let end = self.empty_at(self.previous_significant_end())?;
            return self.add_type_ref(end, TypeRef::Error);
        }
        let mut type_stops = TypeStops::from_expression(stops);
        if stops.delimiters & Stops::WHEN_CALL_OPERAND_RECOVERY != 0 {
            type_stops.when_call_body = stops.when_entry_body;
        }
        self.parse_type_ref(type_stops)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SoftInfixKeyword {
    To,
    Shl,
    Shr,
    Ushr,
    And,
    Xor,
    Or,
}

impl SoftInfixKeyword {
    pub(super) fn parse(name: &str) -> Option<Self> {
        match name {
            "to" => Some(Self::To),
            "shl" => Some(Self::Shl),
            "shr" => Some(Self::Shr),
            "ushr" => Some(Self::Ushr),
            "and" => Some(Self::And),
            "xor" => Some(Self::Xor),
            "or" => Some(Self::Or),
            _ => None,
        }
    }
}

pub(super) fn infix_rule_from_parts(
    kind: LexemeKind,
    soft: Option<SoftInfixKeyword>,
) -> Option<InfixRule> {
    let rule = match kind {
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
        LexemeKind::Token(TokenKind::Identifier) => match soft? {
            SoftInfixKeyword::To => InfixRule::left(PREC_TO, InfixKind::Binary(BinaryOperator::To)),
            SoftInfixKeyword::Shl => {
                InfixRule::left(PREC_SHIFT, InfixKind::Binary(BinaryOperator::Shl))
            }
            SoftInfixKeyword::Shr => {
                InfixRule::left(PREC_SHIFT, InfixKind::Binary(BinaryOperator::Shr))
            }
            SoftInfixKeyword::Ushr => {
                InfixRule::left(PREC_SHIFT, InfixKind::Binary(BinaryOperator::Ushr))
            }
            SoftInfixKeyword::And => InfixRule::left(
                PREC_BITWISE_AND,
                InfixKind::Binary(BinaryOperator::BitwiseAnd),
            ),
            SoftInfixKeyword::Xor => InfixRule::left(
                PREC_BITWISE_XOR,
                InfixKind::Binary(BinaryOperator::BitwiseXor),
            ),
            SoftInfixKeyword::Or => InfixRule::left(
                PREC_BITWISE_OR,
                InfixKind::Binary(BinaryOperator::BitwiseOr),
            ),
        },
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
        LexemeKind::Token(TokenKind::Symbol(Symbol::GreaterEqual)) => InfixRule::non_associative(
            PREC_COMPARISON,
            InfixKind::Binary(BinaryOperator::GreaterEqual),
            NonAssociativeGroup::Comparison,
        ),
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
        _ => return None,
    };
    Some(rule)
}
