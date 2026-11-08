use super::*;

impl Parser<'_> {
    pub(super) fn parse_supertype_list(
        &mut self,
        kind: ClassifierKind,
    ) -> Result<(Option<Span>, Vec<SupertypeEntry>), ParserInternalError> {
        if !self.current_is_symbol(Symbol::Colon) {
            return Ok((None, Vec::new()));
        }
        let colon_span = self.bump()?.span();
        let mut entries = Vec::new();
        loop {
            let current = self.current()?;
            if self.can_start_type_ref(current) {
                let type_ref = self.parse_type_ref(
                    TypeStops::empty()
                        .with(TypeStops::COMMA)
                        .with(TypeStops::LEFT_BRACE)
                        .with(TypeStops::FILE),
                )?;
                let type_span = self.type_span(type_ref)?;
                let delegation = if self.current_identifier_is("by")? {
                    let by_span = self.bump()?.span();
                    if !matches!(kind, ClassifierKind::Class { .. }) {
                        self.emit(
                            codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                            "unsupported class-family form",
                            by_span,
                        )?;
                    }
                    let current = self.current()?;
                    let target_boundary = self.current_is_symbol(Symbol::Comma)
                        || self.current_is_symbol(Symbol::LeftBrace)
                        || matches!(current.kind(), LexemeKind::Eof)
                        || self.is_file_declaration_boundary(current);
                    let target = if self.current_is_identifier() {
                        NameMarker::Present(self.bump()?.span())
                    } else if matches!(current.kind(), LexemeKind::Token(TokenKind::StringStart)) {
                        self.recover_string_delegation_target(current.span())?
                    } else {
                        let primary = if target_boundary {
                            self.empty_at(current.span().start())?
                        } else {
                            current.span()
                        };
                        if !self.is_poison_kind(current.kind()) {
                            self.emit(
                                codes::EXPECTED_DELEGATION_TARGET,
                                "expected delegation target",
                                primary,
                            )?;
                        }
                        if target_boundary {
                            NameMarker::Missing(primary)
                        } else {
                            self.bump()?;
                            NameMarker::Error(primary)
                        }
                    };
                    let target_span = marker_span(target);
                    let delegation = DelegationClause {
                        span: self.span(
                            by_span.start(),
                            if target_span.is_empty() {
                                by_span.end()
                            } else {
                                target_span.end().max(by_span.end())
                            },
                        )?,
                        by_span,
                        target,
                    };
                    let current = self.current()?;
                    let at_entry_boundary = self.current_is_symbol(Symbol::Comma)
                        || self.current_is_symbol(Symbol::LeftBrace)
                        || matches!(current.kind(), LexemeKind::Eof)
                        || self.is_file_declaration_boundary(current);
                    if !at_entry_boundary {
                        if !self.is_poison_kind(current.kind()) {
                            self.emit(
                                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                                "unsupported class-family form",
                                current.span(),
                            )?;
                        }
                        self.recover_declaration_region(
                            DeclarationStops::EMPTY
                                .with(DeclarationStops::COMMA)
                                .with(DeclarationStops::LEFT_BRACE)
                                .union(self.root_declaration_stops()),
                        )?;
                    }
                    Some(delegation)
                } else {
                    None
                };
                entries.push(SupertypeEntry {
                    span: self.span(
                        type_span.start(),
                        delegation
                            .map(|clause| clause.span.end())
                            .unwrap_or(type_span.end()),
                    )?,
                    type_ref,
                    delegation,
                });
                if self.current_is_symbol(Symbol::LeftParen) {
                    let primary = self.current()?.span();
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        primary,
                    )?;
                    self.recover_declaration_region(
                        DeclarationStops::EMPTY
                            .with(DeclarationStops::COMMA)
                            .with(DeclarationStops::LEFT_BRACE)
                            .union(self.root_declaration_stops()),
                    )?;
                }
            } else {
                let primary = if self.current_is_symbol(Symbol::LeftBrace)
                    || matches!(current.kind(), LexemeKind::Eof)
                    || self.is_file_declaration_boundary(current)
                {
                    self.empty_at(current.span().start())?
                } else {
                    current.span()
                };
                if !self.is_poison_kind(current.kind()) {
                    self.emit(codes::EXPECTED_SUPERTYPE, "expected supertype", primary)?;
                }
                if !primary.is_empty() {
                    self.recover_declaration_region(
                        DeclarationStops::EMPTY
                            .with(DeclarationStops::COMMA)
                            .with(DeclarationStops::LEFT_BRACE)
                            .union(self.root_declaration_stops()),
                    )?;
                }
            }
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                if self.current_is_symbol(Symbol::LeftBrace)
                    || matches!(self.current()?.kind(), LexemeKind::Eof)
                {
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        comma,
                    )?;
                    break;
                }
                continue;
            }
            break;
        }
        Ok((Some(colon_span), entries))
    }

    fn recover_string_delegation_target(
        &mut self,
        opener: Span,
    ) -> Result<NameMarker, ParserInternalError> {
        let (end, string_poison) = self.recover_declaration_string()?;
        if !string_poison {
            self.emit(
                codes::EXPECTED_DELEGATION_TARGET,
                "expected delegation target",
                opener,
            )?;
        }
        Ok(NameMarker::Error(self.span(opener.start(), end)?))
    }
}
