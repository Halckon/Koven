use super::*;

impl Parser<'_> {
    pub(super) fn parse_explicit_result(
        &mut self,
        stops: Stops,
    ) -> Result<(TypeRefId, Option<BorrowReturnSyntax>), ParserInternalError> {
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
        let Some(borrow_span) = borrow else {
            return Ok((target, None));
        };
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
                Some(BorrowReturnSyntax {
                    borrow_span,
                    from_span: span,
                    source: BorrowReturnSource::Parameter(NameMarker::Missing(span)),
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
        Ok((
            target,
            Some(BorrowReturnSyntax {
                borrow_span,
                from_span,
                source,
            }),
        ))
    }
}
