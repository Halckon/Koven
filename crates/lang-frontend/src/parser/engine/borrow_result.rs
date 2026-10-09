use super::*;

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
        if borrow.is_none() && !self.current_identifier_is("from")? {
            return Ok((target, None));
        }
        let from_span = if self.current_identifier_is("from")? {
            self.bump()?.span()
        } else {
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
        Ok(
            (self.current_identifier_is("borrow")? || self.current_identifier_is("inout")?)
                && (self.peek_is_keyword(1, Keyword::Val)
                    || self.peek_is_keyword(1, Keyword::Var)
                    || (self.peek_is_symbol(1, Symbol::Question)
                        && (self.peek_is_keyword(2, Keyword::Val)
                            || self.peek_is_keyword(2, Keyword::Var)))),
        )
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
        if !is_borrow || !immutable || conditional {
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
