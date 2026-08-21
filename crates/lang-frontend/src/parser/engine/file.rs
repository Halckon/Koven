use super::*;

impl Parser<'_> {
    pub(super) fn parse_declaration_root(&mut self) -> Result<ItemId, ParserInternalError> {
        let root = self.parse_declaration_item()?;
        self.consume_declaration_tail(root)
    }

    pub(super) fn parse_file_header(
        &mut self,
    ) -> Result<(Option<PackageDirective>, Vec<ImportDirective>), ParserInternalError> {
        let package = if self.current_is_keyword(Keyword::Package) {
            let directive = self.parse_package_directive()?;
            self.consume_file_header_separator()?;
            Some(directive)
        } else {
            None
        };

        let mut imports = Vec::new();
        while self.current_is_keyword(Keyword::Import) {
            imports.push(self.parse_import_directive()?);
            self.consume_file_header_separator()?;
        }

        Ok((package, imports))
    }

    pub(super) fn consume_file_header_separator(&mut self) -> Result<(), ParserInternalError> {
        if matches!(self.current()?.kind(), LexemeKind::Eof) {
            return Ok(());
        }
        if self.file_separator_region_has_line_break()? {
            return Ok(());
        }
        if self.current_is_symbol(Symbol::Semicolon) {
            self.bump()?;
            return Ok(());
        }
        self.emit(
            codes::EXPECTED_FILE_HEADER_SEPARATOR,
            "expected file header separator",
            self.current()?.span(),
        )
    }

    pub(super) fn parse_package_directive(
        &mut self,
    ) -> Result<PackageDirective, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let (segments, _) = self.parse_qualified_name(
            codes::EXPECTED_PACKAGE_NAME,
            "expected package name",
            false,
        )?;
        let end = self.previous_significant_end().max(keyword_span.end());
        Ok(PackageDirective {
            span: self.span(keyword_span.start(), end)?,
            keyword_span,
            segments,
        })
    }

    pub(super) fn parse_import_directive(
        &mut self,
    ) -> Result<ImportDirective, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let (segments, wildcard_span) = self.parse_import_target()?;
        let mut alias = None;
        let mut end = self.previous_significant_end().max(keyword_span.end());

        if self.current_is_keyword(Keyword::As) {
            let as_span = self.bump()?.span();
            if wildcard_span.is_some() {
                self.emit(
                    codes::WILDCARD_IMPORT_ALIAS,
                    "wildcard import cannot have an alias",
                    as_span,
                )?;
            }
            let name_span = if self.current_is_identifier() {
                self.bump()?.span()
            } else {
                let current = self.current()?;
                let boundary = matches!(current.kind(), LexemeKind::Eof)
                    || self.is_file_declaration_boundary(current);
                let span = if boundary {
                    self.empty_at(current.span().start())?
                } else {
                    current.span()
                };
                if !self.is_poison_kind(current.kind()) {
                    self.emit(codes::EXPECTED_IMPORT_ALIAS, "expected import alias", span)?;
                }
                if !boundary {
                    self.recover_declaration_region(DeclarationStops::FILE)?;
                    end = self.previous_significant_end().max(end);
                }
                span
            };
            end = end.max(name_span.end()).max(as_span.end());
            if wildcard_span.is_none() {
                alias = Some(ImportAlias { as_span, name_span });
            }
        }

        Ok(ImportDirective {
            span: self.span(keyword_span.start(), end)?,
            keyword_span,
            segments,
            wildcard_span,
            alias,
        })
    }

    pub(super) fn parse_import_target(
        &mut self,
    ) -> Result<(Vec<QualifiedNameSegment>, Option<Span>), ParserInternalError> {
        let (segments, wildcard_prefix) = self.parse_qualified_name(
            codes::EXPECTED_IMPORT_TARGET,
            "expected import target",
            true,
        )?;
        let wildcard_span = if wildcard_prefix && self.current_is_symbol(Symbol::Star) {
            Some(self.bump()?.span())
        } else {
            None
        };
        Ok((segments, wildcard_span))
    }

    pub(super) fn parse_qualified_name(
        &mut self,
        code: &str,
        message: &'static str,
        allow_wildcard: bool,
    ) -> Result<(Vec<QualifiedNameSegment>, bool), ParserInternalError> {
        let mut segments = Vec::new();
        if !self.current_is_identifier() {
            let current = self.current()?;
            let boundary = matches!(current.kind(), LexemeKind::Eof)
                || self.is_file_declaration_boundary(current);
            let span = if boundary {
                self.empty_at(current.span().start())?
            } else {
                current.span()
            };
            if !self.is_poison_kind(current.kind()) {
                self.emit(code, message, span)?;
            }
            if !boundary {
                self.recover_declaration_region(DeclarationStops::FILE)?;
            }
            return Ok((segments, false));
        }

        segments.push(QualifiedNameSegment {
            span: self.bump()?.span(),
        });
        while self.current_is_symbol(Symbol::Dot) {
            self.bump()?;
            if allow_wildcard && self.current_is_symbol(Symbol::Star) {
                return Ok((segments, true));
            }
            if self.current_is_identifier() {
                segments.push(QualifiedNameSegment {
                    span: self.bump()?.span(),
                });
                continue;
            }
            let current = self.current()?;
            let boundary = matches!(current.kind(), LexemeKind::Eof)
                || self.is_file_declaration_boundary(current);
            let span = if boundary {
                self.empty_at(current.span().start())?
            } else {
                current.span()
            };
            if !self.is_poison_kind(current.kind()) {
                self.emit(code, message, span)?;
            }
            if !boundary {
                self.recover_declaration_region(DeclarationStops::FILE)?;
            }
            break;
        }
        Ok((segments, false))
    }

    pub(super) fn parse_file_roots(&mut self) -> Result<Vec<ItemId>, ParserInternalError> {
        let mut roots = Vec::new();
        while !matches!(self.current()?.kind(), LexemeKind::Eof) {
            if self.current_is_symbol(Symbol::Semicolon) {
                let span = self.bump()?.span();
                self.emit(codes::EXPECTED_DECLARATION, "expected declaration", span)?;
                roots.push(self.add_item(span, Item::Error)?);
                continue;
            }

            if self.current_is_keyword(Keyword::Package) || self.current_is_keyword(Keyword::Import)
            {
                let is_package = self.current_is_keyword(Keyword::Package);
                let start = self.current()?.span().start();
                let (directive_end, primary) = if is_package {
                    let directive = self.parse_package_directive()?;
                    (directive.span.end(), directive.keyword_span)
                } else {
                    let directive = self.parse_import_directive()?;
                    (directive.span.end(), directive.keyword_span)
                };
                let span = self.span(start, directive_end)?;
                self.emit(
                    if is_package {
                        codes::MISPLACED_PACKAGE_DIRECTIVE
                    } else {
                        codes::MISPLACED_IMPORT_DIRECTIVE
                    },
                    if is_package {
                        "misplaced package directive"
                    } else {
                        "misplaced import directive"
                    },
                    primary,
                )?;
                roots.push(self.add_item(span, Item::Error)?);
                self.consume_file_header_separator()?;
                continue;
            }

            let started_as_declaration = simple_declaration_start_kind(self.current()?.kind());
            let before = self.index;
            let root = self.parse_declaration_item()?;
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            roots.push(root);

            if matches!(self.current()?.kind(), LexemeKind::Eof) {
                break;
            }
            let separated_by_line_break = self.file_separator_region_has_line_break()?;
            if self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
                continue;
            }
            if started_as_declaration
                && self.is_file_declaration_boundary(self.current()?)
                && !separated_by_line_break
            {
                self.emit(
                    if self.current_is_keyword(Keyword::Package)
                        || self.current_is_keyword(Keyword::Import)
                    {
                        codes::EXPECTED_FILE_HEADER_SEPARATOR
                    } else {
                        codes::EXPECTED_DECLARATION_SEPARATOR
                    },
                    if self.current_is_keyword(Keyword::Package)
                        || self.current_is_keyword(Keyword::Import)
                    {
                        "expected file header separator"
                    } else {
                        "expected declaration separator"
                    },
                    self.current()?.span(),
                )?;
            }
        }
        Ok(roots)
    }

    pub(super) fn parse_declaration_item(&mut self) -> Result<ItemId, ParserInternalError> {
        let modifiers = self.parse_declaration_modifiers(false)?;
        let declaration = self.parse_unmodified_declaration_item()?;
        self.wrap_modified_item(modifiers, declaration)
    }

    pub(super) fn parse_unmodified_declaration_item(
        &mut self,
    ) -> Result<ItemId, ParserInternalError> {
        if self.current_identifier_is("nocopy")?
            && self
                .peek(1)
                .is_some_and(|next| classifier_declaration_start_kind(next.kind()))
        {
            let primary = self.bump()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            return self.parse_classifier_declaration();
        }
        Ok(
            if self.local_destructuring_start(Keyword::Val)
                || self.local_destructuring_start(Keyword::Var)
                || self.const_local_destructuring_start()
            {
                self.parse_unsupported_destructuring_context()?
            } else if self.current_is_keyword(Keyword::Val) {
                let keyword = self.bump()?.span();
                self.parse_variable_declaration(keyword, VariableKind::Val)?
            } else if self.current_is_keyword(Keyword::Var) {
                let keyword = self.bump()?.span();
                self.parse_variable_declaration(keyword, VariableKind::Var)?
            } else if self.current_is_keyword(Keyword::Const) {
                self.parse_constant_declaration()?
            } else if self.current_is_keyword(Keyword::Fun) {
                self.parse_function_declaration()?
            } else if classifier_declaration_start_kind(self.current()?.kind()) {
                self.parse_classifier_declaration()?
            } else {
                let current = self.current()?;
                let span = if self.file_mode {
                    let start = current.span().start();
                    let end = self.recover_declaration_region(DeclarationStops::FILE)?;
                    self.span(start, end.max(current.span().end()))?
                } else if matches!(current.kind(), LexemeKind::Eof) {
                    self.empty_at(current.span().start())?
                } else {
                    self.bump()?.span()
                };
                if !self.is_poison_kind(current.kind()) {
                    self.emit(codes::EXPECTED_DECLARATION, "expected declaration", span)?;
                }
                self.add_item(span, Item::Error)?
            },
        )
    }

    pub(super) fn parse_declaration_modifiers(
        &mut self,
        allow_override: bool,
    ) -> Result<DeclarationModifiers, ParserInternalError> {
        let mut modifiers = DeclarationModifiers::default();
        let mut saw_override = false;
        loop {
            let visibility = if self.current_is_keyword(Keyword::Public) {
                Some(VisibilityModifier::Public(self.current()?.span()))
            } else if self.current_is_keyword(Keyword::Internal) {
                Some(VisibilityModifier::Internal(self.current()?.span()))
            } else if self.current_is_keyword(Keyword::Private) {
                Some(VisibilityModifier::Private(self.current()?.span()))
            } else {
                None
            };
            if let Some(visibility) = visibility {
                let span = self.bump()?.span();
                if modifiers.visibility.is_some() || saw_override {
                    self.emit(
                        codes::INVALID_DECLARATION_MODIFIER,
                        "invalid declaration modifier",
                        span,
                    )?;
                } else {
                    modifiers.visibility = Some(visibility);
                }
                continue;
            }
            if self.current_is_keyword(Keyword::Override) {
                let span = self.bump()?.span();
                if !allow_override || saw_override {
                    self.emit(
                        codes::INVALID_DECLARATION_MODIFIER,
                        "invalid declaration modifier",
                        span,
                    )?;
                } else {
                    modifiers.override_span = Some(span);
                }
                saw_override = true;
                continue;
            }
            break;
        }
        Ok(modifiers)
    }

    pub(super) fn wrap_modified_item(
        &mut self,
        modifiers: DeclarationModifiers,
        declaration: ItemId,
    ) -> Result<ItemId, ParserInternalError> {
        let Some(start) = declaration_modifier_start(modifiers) else {
            return Ok(declaration);
        };
        let child_span = self.ast.items().get(declaration)?.span();
        self.add_item(
            self.span(start, child_span.end().max(start))?,
            Item::Modified {
                modifiers,
                declaration,
            },
        )
    }
}
