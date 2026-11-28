use super::{class::ClassifierHeaderOwner, *};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FileHeaderSeparatorDecision {
    Satisfied,
    ConsumeSemicolon,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FileRootSeparatorDiagnostic {
    None,
    ExpectedFileHeaderSeparator,
    ExpectedDeclarationSeparator,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ModifierToken {
    Visibility,
    Override,
    Receiver,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ModifierStepOutcome {
    Accept,
    Invalid,
    Stop,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ModifierScanState {
    pub(super) has_visibility: bool,
    pub(super) saw_override: bool,
    pub(super) saw_receiver: bool,
}

pub(super) const fn file_header_separator_decision(
    kind: LexemeKind,
    has_line_break: bool,
) -> FileHeaderSeparatorDecision {
    if matches!(kind, LexemeKind::Eof) || has_line_break {
        FileHeaderSeparatorDecision::Satisfied
    } else if matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
    ) {
        FileHeaderSeparatorDecision::ConsumeSemicolon
    } else {
        FileHeaderSeparatorDecision::Missing
    }
}

pub(super) const fn file_root_separator_diagnostic(
    started_as_declaration: bool,
    at_file_boundary: bool,
    has_line_break: bool,
    kind: LexemeKind,
) -> FileRootSeparatorDiagnostic {
    if matches!(
        kind,
        LexemeKind::Eof | LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
    ) || !started_as_declaration
        || !at_file_boundary
        || has_line_break
    {
        FileRootSeparatorDiagnostic::None
    } else if matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(Keyword::Package | Keyword::Import))
    ) {
        FileRootSeparatorDiagnostic::ExpectedFileHeaderSeparator
    } else {
        FileRootSeparatorDiagnostic::ExpectedDeclarationSeparator
    }
}

pub(super) const fn step_declaration_modifier(
    allow_override: bool,
    policy: ReceiverModifierPolicy,
    state: ModifierScanState,
    token: ModifierToken,
) -> (ModifierScanState, ModifierStepOutcome) {
    match token {
        ModifierToken::Visibility => {
            if state.has_visibility || state.saw_override || state.saw_receiver {
                (state, ModifierStepOutcome::Invalid)
            } else {
                (
                    ModifierScanState {
                        has_visibility: true,
                        ..state
                    },
                    ModifierStepOutcome::Accept,
                )
            }
        }
        ModifierToken::Override => {
            let next = ModifierScanState {
                saw_override: true,
                ..state
            };
            if !allow_override || state.saw_override || state.saw_receiver {
                (next, ModifierStepOutcome::Invalid)
            } else {
                (next, ModifierStepOutcome::Accept)
            }
        }
        ModifierToken::Receiver => match policy {
            ReceiverModifierPolicy::NotRecognized => (state, ModifierStepOutcome::Stop),
            ReceiverModifierPolicy::Rejected => (
                ModifierScanState {
                    saw_receiver: true,
                    ..state
                },
                ModifierStepOutcome::Invalid,
            ),
            ReceiverModifierPolicy::Allowed => {
                let next = ModifierScanState {
                    saw_receiver: true,
                    ..state
                };
                if state.saw_receiver {
                    (next, ModifierStepOutcome::Invalid)
                } else {
                    (next, ModifierStepOutcome::Accept)
                }
            }
        },
    }
}

#[cfg(any(test, kani))]
pub(super) fn run_declaration_modifiers(
    allow_override: bool,
    policy: ReceiverModifierPolicy,
    initial: ModifierScanState,
    tokens: &[ModifierToken],
) -> (ModifierScanState, usize) {
    let mut state = initial;
    let mut errors = 0usize;
    let mut index = 0usize;
    while index < tokens.len() {
        let (next, outcome) =
            step_declaration_modifier(allow_override, policy, state, tokens[index]);
        state = next;
        match outcome {
            ModifierStepOutcome::Stop => break,
            ModifierStepOutcome::Accept => {}
            ModifierStepOutcome::Invalid => errors += 1,
        }
        index += 1;
    }
    (state, errors)
}

impl Parser<'_> {
    pub(super) fn parse_declaration_root(&mut self) -> Result<ItemId, ParserInternalError> {
        let root = self.parse_declaration_item()?;
        self.consume_declaration_tail(root)
    }

    pub(super) fn parse_file_header(
        &mut self,
    ) -> Result<(Option<PackageDirective>, Vec<ImportDirective>, bool), ParserInternalError> {
        let (package, initial_package_header_complete) =
            if self.current_is_keyword(Keyword::Package) {
                let (directive, name_complete) = self.parse_package_directive()?;
                let separator_complete = self.consume_file_header_separator()?;
                (Some(directive), name_complete && separator_complete)
            } else {
                (None, true)
            };

        let mut imports = Vec::new();
        while self.current_is_keyword(Keyword::Import) {
            imports.push(self.parse_import_directive()?);
            self.consume_file_header_separator()?;
        }

        Ok((package, imports, initial_package_header_complete))
    }

    pub(super) fn consume_file_header_separator(&mut self) -> Result<bool, ParserInternalError> {
        let current = self.current()?;
        if matches!(
            file_header_separator_decision(current.kind(), false),
            FileHeaderSeparatorDecision::Satisfied
        ) {
            return Ok(true);
        }
        let has_line_break = self.file_separator_region_has_line_break()?;
        match file_header_separator_decision(current.kind(), has_line_break) {
            FileHeaderSeparatorDecision::Satisfied => Ok(true),
            FileHeaderSeparatorDecision::ConsumeSemicolon => {
                self.bump()?;
                Ok(true)
            }
            FileHeaderSeparatorDecision::Missing => {
                self.emit(
                    codes::EXPECTED_FILE_HEADER_SEPARATOR,
                    "expected file header separator",
                    current.span(),
                )?;
                Ok(false)
            }
        }
    }

    pub(super) fn parse_package_directive(
        &mut self,
    ) -> Result<(PackageDirective, bool), ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let (segments, _, name_complete) = self.parse_qualified_name(
            codes::EXPECTED_PACKAGE_NAME,
            "expected package name",
            false,
        )?;
        let end = self.previous_significant_end().max(keyword_span.end());
        Ok((
            PackageDirective {
                span: self.span(keyword_span.start(), end)?,
                keyword_span,
                segments,
            },
            name_complete,
        ))
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
        let (segments, wildcard_prefix, _) = self.parse_qualified_name(
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
    ) -> Result<(Vec<QualifiedNameSegment>, bool, bool), ParserInternalError> {
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
            return Ok((segments, false, false));
        }

        segments.push(QualifiedNameSegment {
            span: self.bump()?.span(),
        });
        let mut name_complete = true;
        while self.current_is_symbol(Symbol::Dot) {
            self.bump()?;
            if allow_wildcard && self.current_is_symbol(Symbol::Star) {
                return Ok((segments, true, true));
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
            name_complete = false;
            break;
        }
        Ok((segments, false, name_complete))
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
                    let (directive, _) = self.parse_package_directive()?;
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

            let started_as_declaration = file_construct_start_kind(self.current()?.kind())
                || self.value_class_start_at(0)?;
            let before = self.index;
            let root = self.parse_declaration_item()?;
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            roots.push(root);

            let current = self.current()?;
            if matches!(current.kind(), LexemeKind::Eof) {
                break;
            }
            let separated_by_line_break = self.file_separator_region_has_line_break()?;
            if self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
                continue;
            }
            let at_file_boundary = started_as_declaration
                && !separated_by_line_break
                && (self.is_file_declaration_boundary(current) || self.value_class_start_at(0)?);
            match file_root_separator_diagnostic(
                started_as_declaration,
                at_file_boundary,
                separated_by_line_break,
                current.kind(),
            ) {
                FileRootSeparatorDiagnostic::None => {}
                FileRootSeparatorDiagnostic::ExpectedFileHeaderSeparator => {
                    self.emit(
                        codes::EXPECTED_FILE_HEADER_SEPARATOR,
                        "expected file header separator",
                        current.span(),
                    )?;
                }
                FileRootSeparatorDiagnostic::ExpectedDeclarationSeparator => {
                    self.emit(
                        codes::EXPECTED_DECLARATION_SEPARATOR,
                        "expected declaration separator",
                        current.span(),
                    )?;
                }
            }
        }
        Ok(roots)
    }

    pub(super) fn parse_declaration_item(&mut self) -> Result<ItemId, ParserInternalError> {
        let mut modifiers =
            self.parse_declaration_modifiers(false, ReceiverModifierPolicy::Allowed)?;
        let declaration = self.parse_unmodified_declaration_item()?;
        if let Some(mode) = modifiers.receiver_mode
            && (matches!(mode, ParameterModeMarker::Inout(_))
                || !matches!(
                    self.ast.items().get(declaration)?.payload(),
                    Item::Function {
                        extension_receiver: Some(_),
                        ..
                    }
                ))
        {
            let span = match mode {
                ParameterModeMarker::Own(span)
                | ParameterModeMarker::Borrow(span)
                | ParameterModeMarker::Inout(span) => span,
            };
            self.emit(
                codes::INVALID_DECLARATION_MODIFIER,
                "invalid declaration modifier",
                span,
            )?;
            modifiers.receiver_mode = None;
        }
        self.wrap_modified_item(modifiers, declaration)
    }

    pub(super) fn parse_unmodified_declaration_item(
        &mut self,
    ) -> Result<ItemId, ParserInternalError> {
        let declaration_stops = self.root_expression_stops();
        let classifier_owner = if self.file_mode {
            ClassifierHeaderOwner::File
        } else {
            ClassifierHeaderOwner::Declaration
        };
        let is_nocopy_classifier = self.current_identifier_is("nocopy")?
            && (self.peek_identifier_is(1, "value")? && self.peek_is_keyword(2, Keyword::Class)
                || self
                    .peek(1)
                    .is_some_and(|next| classifier_declaration_start_kind(next.kind())));
        if is_nocopy_classifier {
            let primary = self.bump()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            return self.parse_classifier_declaration(classifier_owner);
        }
        Ok(
            if self.local_destructuring_start(Keyword::Val)
                || self.local_destructuring_start(Keyword::Var)
                || self.const_local_destructuring_start()
            {
                self.parse_unsupported_destructuring_context()?
            } else if self.current_is_keyword(Keyword::Val) {
                let keyword = self.bump()?.span();
                self.parse_variable_declaration(keyword, VariableKind::Val, declaration_stops)?
            } else if self.current_is_keyword(Keyword::Var) {
                let keyword = self.bump()?.span();
                self.parse_variable_declaration(keyword, VariableKind::Var, declaration_stops)?
            } else if self.current_is_keyword(Keyword::Const) {
                self.parse_constant_declaration(declaration_stops)?
            } else if self.current_is_keyword(Keyword::Fun) {
                self.parse_function_declaration(declaration_stops, true)?
            } else if self.classifier_declaration_start()? {
                self.parse_classifier_declaration(classifier_owner)?
            } else {
                let current = self.current()?;
                let span = if self.file_mode {
                    let start = current.span().start();
                    let end = self.recover_declaration_region(DeclarationStops::FILE)?;
                    self.span(start, end.max(current.span().end()))?
                } else if matches!(current.kind(), LexemeKind::Eof) {
                    self.empty_at(current.span().start())?
                } else if matches!(current.kind(), LexemeKind::Token(TokenKind::StringStart)) {
                    // String 是多 lexeme 的单一词法 owner；从 opener 整体恢复，避免 tail
                    // recovery 在 owner 中途遇到 StringEnd 并误判为损坏的 lexeme stream。
                    let end = self.recover_declaration_region(DeclarationStops::EMPTY)?;
                    self.span(current.span().start(), end.max(current.span().end()))?
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
        receiver_policy: ReceiverModifierPolicy,
    ) -> Result<DeclarationModifiers, ParserInternalError> {
        let mut modifiers = DeclarationModifiers::default();
        let mut scan_state = ModifierScanState::default();
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
                let (next_state, outcome) = step_declaration_modifier(
                    allow_override,
                    receiver_policy,
                    scan_state,
                    ModifierToken::Visibility,
                );
                scan_state = next_state;
                match outcome {
                    ModifierStepOutcome::Accept => modifiers.visibility = Some(visibility),
                    ModifierStepOutcome::Invalid => {
                        self.emit(
                            codes::INVALID_DECLARATION_MODIFIER,
                            "invalid declaration modifier",
                            span,
                        )?;
                    }
                    ModifierStepOutcome::Stop => break,
                }
                continue;
            }
            if self.current_is_keyword(Keyword::Override) {
                let span = self.bump()?.span();
                let (next_state, outcome) = step_declaration_modifier(
                    allow_override,
                    receiver_policy,
                    scan_state,
                    ModifierToken::Override,
                );
                scan_state = next_state;
                match outcome {
                    ModifierStepOutcome::Accept => modifiers.override_span = Some(span),
                    ModifierStepOutcome::Invalid => {
                        self.emit(
                            codes::INVALID_DECLARATION_MODIFIER,
                            "invalid declaration modifier",
                            span,
                        )?;
                    }
                    ModifierStepOutcome::Stop => break,
                }
                continue;
            }
            let Some(receiver) = self.receiver_modifier_at(0)? else {
                break;
            };
            let (next_state, outcome) = step_declaration_modifier(
                allow_override,
                receiver_policy,
                scan_state,
                ModifierToken::Receiver,
            );
            if outcome == ModifierStepOutcome::Stop {
                break;
            }
            let span = self.bump()?.span();
            scan_state = next_state;
            match outcome {
                ModifierStepOutcome::Accept => modifiers.receiver_mode = Some(receiver),
                ModifierStepOutcome::Invalid => {
                    self.emit(
                        codes::INVALID_DECLARATION_MODIFIER,
                        "invalid declaration modifier",
                        span,
                    )?;
                }
                ModifierStepOutcome::Stop => break,
            }
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
