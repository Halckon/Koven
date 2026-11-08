use super::*;

impl Parser<'_> {
    pub(super) fn parse_supertype_list(
        &mut self,
        kind: ClassifierKind,
        owner: ClassifierHeaderOwner,
    ) -> Result<(Option<Span>, Vec<SupertypeEntry>), ParserInternalError> {
        if !self.current_is_symbol(Symbol::Colon) {
            return Ok((None, Vec::new()));
        }
        let file_header = owner == ClassifierHeaderOwner::File;
        let colon_span = self.bump()?.span();
        let mut entries = Vec::new();
        let mut recovery_stops = DeclarationStops::EMPTY
            .with(DeclarationStops::COMMA)
            .with(DeclarationStops::LEFT_BRACE)
            .with(DeclarationStops::RIGHT_BRACE)
            .union(self.root_declaration_stops());
        let mut type_stops = TypeStops::empty()
            .with(TypeStops::COMMA)
            .with(TypeStops::CLASS_BODY)
            .with(TypeStops::RIGHT_BRACE)
            .with(TypeStops::FILE)
            .for_supertype(owner);
        if file_header {
            recovery_stops = recovery_stops.with(DeclarationStops::CLASS_FILE);
            type_stops = type_stops.with(TypeStops::CLASS_FILE);
        }
        loop {
            let current = self.current()?;
            let file_boundary = file_header && self.at_class_header_file_boundary(true)?;
            if !file_boundary && self.can_start_type_ref(current) {
                let (type_ref, complete) = self.parse_type_ref_with_completion(type_stops)?;
                let type_span = self.type_span(type_ref)?;
                let mut trailing_lexical_error = None;
                let delegation = if complete && self.current_identifier_is("by")? {
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
                        || self.current_is_symbol(Symbol::RightBrace)
                        || matches!(current.kind(), LexemeKind::Eof)
                        || self.is_file_declaration_boundary(current)
                        || (file_header && self.at_class_header_file_boundary(true)?);
                    let mut target = if self.current_is_identifier() && !target_boundary {
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
                        } else if let Some(group) = self.consume_closed_supertype_group()? {
                            NameMarker::Error(group)
                        } else {
                            self.bump()?;
                            NameMarker::Error(primary)
                        }
                    };
                    let current = self.current()?;
                    let at_entry_boundary = self.current_is_symbol(Symbol::Comma)
                        || self.current_is_symbol(Symbol::LeftBrace)
                        || self.current_is_symbol(Symbol::RightBrace)
                        || matches!(current.kind(), LexemeKind::Eof)
                        || self.is_file_declaration_boundary(current)
                        || (file_header && self.at_class_header_file_boundary(false)?);
                    if !at_entry_boundary {
                        let poison = self.is_poison_kind(current.kind());
                        if poison {
                            self.consume_poison_with_closed_type_arguments()?;
                        } else {
                            self.emit(
                                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                                "unsupported class-family form",
                                current.span(),
                            )?;
                        }
                        let _ = self.consume_closed_supertype_group()?;
                        self.recover_declaration_region(recovery_stops)?;
                        if poison {
                            let end = self.previous_significant_end();
                            match target {
                                NameMarker::Error(span) => {
                                    target = NameMarker::Error(self.span(span.start(), end)?);
                                }
                                NameMarker::Present(_) => {
                                    trailing_lexical_error =
                                        Some(self.span(current.span().start(), end)?);
                                }
                                NameMarker::Missing(_) => {}
                            }
                        }
                    }
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
                    Some(delegation)
                } else {
                    None
                };
                if delegation.is_none() && complete {
                    let current = self.current()?;
                    if self.is_declaration_trailing_poison(current) {
                        let start = current.span().start();
                        let mut stops = recovery_stops.with(DeclarationStops::SUPERTYPE_SUFFIX);
                        if owner == ClassifierHeaderOwner::Member {
                            stops = stops.with(DeclarationStops::CLASS_MEMBER);
                        }
                        let end = self.recover_declaration_region(stops)?;
                        trailing_lexical_error = Some(self.span(start, end)?);
                    }
                }
                entries.push(SupertypeEntry {
                    span: self.span(
                        type_span.start(),
                        trailing_lexical_error
                            .or_else(|| delegation.map(|clause| clause.span))
                            .map(Span::end)
                            .unwrap_or(type_span.end()),
                    )?,
                    type_ref,
                    delegation,
                    trailing_lexical_error,
                });
                if self.current_is_symbol(Symbol::LeftParen) {
                    let primary = self.current()?.span();
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        primary,
                    )?;
                    let _ = self.consume_closed_supertype_group()?;
                    self.recover_declaration_region(recovery_stops)?;
                }
            } else {
                let primary = if file_boundary
                    || self.current_is_symbol(Symbol::LeftBrace)
                    || self.current_is_symbol(Symbol::RightBrace)
                    || matches!(current.kind(), LexemeKind::Eof)
                    || self.is_file_declaration_boundary(current)
                {
                    self.empty_at(current.span().start())?
                } else {
                    current.span()
                };
                let poison = self.is_poison_kind(current.kind());
                if !poison {
                    self.emit(codes::EXPECTED_SUPERTYPE, "expected supertype", primary)?;
                }
                if !primary.is_empty() {
                    if poison {
                        self.consume_poison_with_closed_type_arguments()?;
                    }
                    self.recover_declaration_region(recovery_stops)?;
                    if poison {
                        let span = self.span(primary.start(), self.previous_significant_end())?;
                        entries.push(SupertypeEntry {
                            span,
                            type_ref: self.add_type_ref(span, TypeRef::Error)?,
                            delegation: None,
                            trailing_lexical_error: None,
                        });
                    }
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

    fn consume_poison_with_closed_type_arguments(&mut self) -> Result<(), ParserInternalError> {
        while self.is_poison() {
            self.bump()?;
        }
        if self.current_is_symbol(Symbol::Less)
            && let Some(closer) = self.strict_trials.closed_type_list(self.current_raw()?)?
        {
            self.index = closer + 1;
        }
        Ok(())
    }

    fn consume_closed_supertype_group(&mut self) -> Result<Option<Span>, ParserInternalError> {
        let (raw, opener) = self
            .significant(0)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        if !matches!(
            opener.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket))
        ) {
            return Ok(None);
        }
        let Some(closer) = self.lexical_recoveries.group_closer(self.lexed, raw) else {
            return Ok(None);
        };
        self.index = closer + 1;
        Ok(Some(self.span(
            opener.span().start(),
            self.lexed.lexemes()[closer].span().end(),
        )?))
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
