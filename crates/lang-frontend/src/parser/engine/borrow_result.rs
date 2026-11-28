use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExplicitResultAction {
    PlainTypeOnly,
    MissingBorrowFrom,
    ParseBorrowSource,
    ParseCarrierSource,
}

#[inline]
pub(super) const fn explicit_result_action(
    has_borrow: bool,
    has_from: bool,
) -> ExplicitResultAction {
    match (has_borrow, has_from) {
        (false, false) => ExplicitResultAction::PlainTypeOnly,
        (true, false) => ExplicitResultAction::MissingBorrowFrom,
        (true, true) => ExplicitResultAction::ParseBorrowSource,
        (false, true) => ExplicitResultAction::ParseCarrierSource,
    }
}

#[inline]
pub(super) const fn is_borrow_local_binding_keyword_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
    )
}

#[inline]
pub(super) const fn is_borrow_local_start_tail(
    next1: LexemeKind,
    next2: Option<LexemeKind>,
) -> bool {
    is_borrow_local_binding_keyword_kind(next1)
        || (matches!(
            next1,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Question))
        ) && match next2 {
            Some(kind) => is_borrow_local_binding_keyword_kind(kind),
            None => false,
        })
}

#[inline]
pub(super) const fn borrow_local_contract_is_invalid(
    is_borrow: bool,
    immutable: bool,
    conditional: bool,
) -> bool {
    !is_borrow || !immutable || conditional
}

impl Parser<'_> {
    pub(super) fn parse_explicit_result(
        &mut self,
        stops: Stops,
    ) -> Result<(TypeRefId, Option<FunctionResultSource>), ParserInternalError> {
        let borrow = self
            .current_identifier_is("borrow")?
            .then(|| self.bump())
            .transpose()?
            .map(|token| token.span());
        let target = self.parse_type_ref(
            TypeStops::from_expression(stops)
                .with(TypeStops::EQUAL)
                .with(TypeStops::LEFT_BRACE),
        )?;
        let has_from = self.current_identifier_is("from")?;
        let action = explicit_result_action(borrow.is_some(), has_from);
        let from_span = match action {
            ExplicitResultAction::PlainTypeOnly => return Ok((target, None)),
            ExplicitResultAction::MissingBorrowFrom => {
                let span = self.empty_at(self.current()?.span().start())?;
                self.emit(
                    codes::INVALID_BORROW_CONTRACT,
                    "borrow result requires a unique 'from' source",
                    span,
                )?;
                return Ok((
                    target,
                    borrow.map(|borrow_span| {
                        FunctionResultSource::Borrow(BorrowReturnSyntax {
                            borrow_span,
                            from_span: span,
                            source: BorrowReturnSource::Parameter(NameMarker::Missing(span)),
                        })
                    }),
                ));
            }
            ExplicitResultAction::ParseBorrowSource | ExplicitResultAction::ParseCarrierSource => {
                self.bump()?.span()
            }
        };
        let source = if self.current_is_keyword(Keyword::This) {
            BorrowReturnSource::Receiver(self.bump()?.span())
        } else {
            BorrowReturnSource::Parameter(self.parse_name_marker(
                codes::INVALID_BORROW_CONTRACT,
                "expected borrow result source",
                NameContext::Declaration,
            )?)
        };
        let result_source = match borrow {
            Some(borrow_span) => FunctionResultSource::Borrow(BorrowReturnSyntax {
                borrow_span,
                from_span,
                source,
            }),
            None => FunctionResultSource::Carrier(CarrierReturnSyntax { from_span, source }),
        };
        Ok((target, Some(result_source)))
    }

    pub(super) fn borrow_local_start(&self) -> Result<bool, ParserInternalError> {
        if !(self.current_identifier_is("borrow")? || self.current_identifier_is("inout")?) {
            return Ok(false);
        }
        let Some(next1) = self.peek(1).map(Lexeme::kind) else {
            return Ok(false);
        };
        let next2 = if matches!(
            next1,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Question))
        ) {
            self.peek(2).map(Lexeme::kind)
        } else {
            None
        };
        Ok(is_borrow_local_start_tail(next1, next2))
    }

    pub(super) fn parse_borrow_local(
        &mut self,
        stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let is_borrow = self.current_identifier_is("borrow")?;
        let marker = self.bump()?.span();
        let conditional = self.current_is_symbol(Symbol::Question);
        if conditional {
            self.bump()?;
        }
        let immutable = self.current_is_keyword(Keyword::Val);
        self.bump()?;
        if borrow_local_contract_is_invalid(is_borrow, immutable, conditional) {
            self.emit(
                codes::INVALID_BORROW_CONTRACT,
                "only ordinary 'borrow val' local bindings are supported",
                marker,
            )?;
        }
        let declaration =
            self.parse_local_variable_declaration(marker, VariableKind::BorrowVal(marker), stops)?;
        let span = self.ast.items().get(declaration)?.span();
        self.add_statement(span, Statement::LocalVariable { declaration })
    }
}
