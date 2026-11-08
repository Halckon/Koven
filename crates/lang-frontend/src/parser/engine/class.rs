mod supertypes;

use super::*;

impl Parser<'_> {
    pub(super) fn parse_classifier_declaration(
        &mut self,
        file_header: bool,
    ) -> Result<ItemId, ParserInternalError> {
        let kind = if self.current_identifier_is("value")? {
            let value_span = self.bump()?.span();
            let class_span = if self.current_is_keyword(Keyword::Class) {
                self.bump()?.span()
            } else {
                let primary = self.current()?.span();
                self.emit(
                    codes::EXPECTED_CLASS_KEYWORD,
                    "expected 'class' keyword",
                    primary,
                )?;
                self.empty_at(primary.start())?
            };
            ClassifierKind::ValueClass {
                value_span,
                class_span,
            }
        } else if self.current_is_keyword(Keyword::Class) {
            ClassifierKind::Class {
                class_span: self.bump()?.span(),
            }
        } else if self.current_is_keyword(Keyword::Interface) {
            ClassifierKind::Interface {
                interface_span: self.bump()?.span(),
            }
        } else if self.current_is_keyword(Keyword::Enum) {
            let enum_span = self.bump()?.span();
            let class_span = if self.current_is_keyword(Keyword::Class) {
                self.bump()?.span()
            } else {
                let primary = self.current()?.span();
                self.emit(
                    codes::EXPECTED_CLASS_KEYWORD,
                    "expected 'class' keyword",
                    primary,
                )?;
                self.empty_at(primary.start())?
            };
            ClassifierKind::EnumClass {
                enum_span,
                class_span,
            }
        } else {
            ClassifierKind::Object {
                object_span: self.bump()?.span(),
            }
        };
        let start = classifier_keyword_start(kind);
        let name = self.parse_classifier_name(file_header)?;
        let (type_parameters, type_parameter_list_span) = self.parse_type_parameters(true)?;
        if let (ClassifierKind::Object { .. }, Some(list_span)) = (kind, type_parameter_list_span) {
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                list_span,
            )?;
        }

        let supports_constructor = matches!(
            kind,
            ClassifierKind::ValueClass { .. } | ClassifierKind::Class { .. }
        );
        let primary_constructor = if self.current_is_symbol(Symbol::LeftParen) {
            let constructor = self.parse_primary_constructor(
                matches!(kind, ClassifierKind::ValueClass { .. }),
                file_header,
            )?;
            if !supports_constructor {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    constructor.left_paren_span,
                )?;
            }
            supports_constructor.then_some(constructor)
        } else {
            if matches!(kind, ClassifierKind::ValueClass { .. }) {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    self.empty_at(self.current()?.span().start())?,
                )?;
            }
            None
        };

        let (supertype_colon_span, supertypes) = self.parse_supertype_list(kind, file_header)?;
        let body = if self.current_is_symbol(Symbol::LeftBrace) {
            Some(self.parse_classifier_body(kind)?)
        } else {
            if matches!(kind, ClassifierKind::EnumClass { .. }) {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    self.empty_at(self.current()?.span().start())?,
                )?;
            }
            None
        };
        let end = self.previous_significant_end();
        self.add_item(
            self.span(start, end)?,
            Item::Classifier(Box::new(ClassifierDeclaration {
                kind,
                name,
                type_parameters,
                type_parameter_list_span,
                primary_constructor,
                supertype_colon_span,
                supertypes,
                body,
            })),
        )
    }

    fn parse_classifier_name(
        &mut self,
        file_header: bool,
    ) -> Result<NameMarker, ParserInternalError> {
        let file_boundary = file_header && self.at_class_header_file_boundary(true)?;
        if self.current_is_identifier() && !file_boundary {
            return Ok(NameMarker::Present(self.bump()?.span()));
        }
        let current = self.current()?;
        if matches!(current.kind(), LexemeKind::Token(TokenKind::StringStart)) {
            let start = current.span().start();
            let owner_end = self
                .lexical_recoveries
                .string_recovery_end(start)
                .or_else(|| {
                    self.lexical_recoveries
                        .lexical_poison_string_recovery_end(start)
                })
                .or_else(|| self.lexical_recoveries.string_owner_end(start));
            if let Some(owner_end) = owner_end {
                self.emit(
                    codes::EXPECTED_CLASSIFIER_NAME,
                    "expected classifier name",
                    current.span(),
                )?;
                let mut end = start;
                while end < owner_end {
                    end = self.bump()?.span().end();
                }
                return Ok(NameMarker::Error(self.span(start, end)?));
            }
        }
        let is_boundary = file_boundary
            || matches!(current.kind(), LexemeKind::Eof)
            || matches!(
                current.kind(),
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::Less
                        | Symbol::LeftParen
                        | Symbol::Colon
                        | Symbol::LeftBrace
                        | Symbol::RightBrace
                ))
            )
            || self.is_file_declaration_boundary(current);
        let primary = if is_boundary {
            self.empty_at(current.span().start())?
        } else {
            current.span()
        };
        if !self.is_poison_kind(current.kind()) {
            self.emit(
                codes::EXPECTED_CLASSIFIER_NAME,
                "expected classifier name",
                primary,
            )?;
        }
        if is_boundary {
            Ok(NameMarker::Missing(primary))
        } else {
            self.bump()?;
            Ok(NameMarker::Error(primary))
        }
    }

    pub(super) fn parse_primary_constructor(
        &mut self,
        require_nonempty: bool,
        file_header: bool,
    ) -> Result<PrimaryConstructor, ParserInternalError> {
        let file_recovery = file_header
            && self
                .lexical_recoveries
                .group_closer(self.lexed, self.current_raw()?)
                .is_none();
        let left_paren_span = self.bump()?.span();
        let mut fields = Vec::new();
        let mut boundary = TypeStops::empty()
            .with(TypeStops::RIGHT_PAREN)
            .with(TypeStops::RIGHT_BRACE)
            .with(TypeStops::CLASS_BODY);
        let mut recovery_stops = DeclarationStops::EMPTY
            .with(DeclarationStops::COMMA)
            .with(DeclarationStops::RIGHT_PAREN)
            .with(DeclarationStops::RIGHT_BRACE)
            .with(DeclarationStops::CLASS_BODY);
        if file_recovery {
            boundary = boundary.with(TypeStops::CLASS_FILE);
            recovery_stops = recovery_stops.with(DeclarationStops::CLASS_FILE);
        }
        if self.current_is_symbol(Symbol::RightParen) && require_nonempty {
            self.emit(
                codes::EXPECTED_CONSTRUCTOR_FIELD,
                "expected constructor field",
                self.current()?.span(),
            )?;
        }
        while !self.at_constructor_field_boundary(boundary)? {
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                self.emit(
                    codes::EXPECTED_CONSTRUCTOR_FIELD,
                    "expected constructor field",
                    comma,
                )?;
                continue;
            }
            if let Some(field) = self.parse_class_field(boundary, recovery_stops)? {
                fields.push(field);
            }
            if !self.at_constructor_field_boundary(boundary)?
                && !self.current_is_symbol(Symbol::Comma)
            {
                if !self.is_poison() {
                    self.emit(
                        codes::EXPECTED_CONSTRUCTOR_SEPARATOR,
                        "expected constructor separator",
                        self.current()?.span(),
                    )?;
                }
                if class_field_start_kind(self.current()?.kind()) {
                    continue;
                }
                self.recover_declaration_region(recovery_stops)?;
            }
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                if self.current_is_symbol(Symbol::RightParen) {
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        comma,
                    )?;
                }
            }
        }
        let right_paren_span = if self.current_is_symbol(Symbol::RightParen) {
            Some(self.bump()?.span())
        } else {
            self.emit_closing(
                self.empty_at(self.current()?.span().start())?,
                left_paren_span,
            )?;
            None
        };
        Ok(PrimaryConstructor {
            left_paren_span,
            fields,
            right_paren_span,
        })
    }

    fn at_constructor_field_boundary(&self, stops: TypeStops) -> Result<bool, ParserInternalError> {
        Ok(stops.contains(self.current()?)
            || (stops.has_class_file_boundary() && self.at_class_header_file_boundary(false)?))
    }

    fn parse_class_field(
        &mut self,
        boundary: TypeStops,
        recovery_stops: DeclarationStops,
    ) -> Result<Option<ClassField>, ParserInternalError> {
        let modifiers =
            self.parse_declaration_modifiers(false, ReceiverModifierPolicy::NotRecognized)?;
        let visibility = modifiers.visibility;
        let current_start = self.current()?.span().start();
        let start = declaration_modifier_start(modifiers).unwrap_or(current_start);
        let is_invalid_mode_modifier = (self.current_identifier_is("borrow")?
            || self.current_identifier_is("inout")?
            || self.current_identifier_is("own")?)
            && (self.peek_is_keyword(1, Keyword::Val) || self.peek_is_keyword(1, Keyword::Var));
        if is_invalid_mode_modifier
            || matches!(
                self.current()?.kind(),
                LexemeKind::Token(TokenKind::Keyword(Keyword::Vararg))
            )
        {
            let primary = self.bump()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
        }
        let (kind, keyword_span) = if self.current_is_keyword(Keyword::Val) {
            (VariableKind::Val, self.bump()?.span())
        } else if self.current_is_keyword(Keyword::Var) {
            (VariableKind::Var, self.bump()?.span())
        } else {
            let primary = self.current()?.span();
            if !self.is_poison() {
                self.emit(
                    codes::EXPECTED_CONSTRUCTOR_FIELD,
                    "expected constructor field",
                    primary,
                )?;
            }
            self.recover_declaration_region(recovery_stops)?;
            return Ok(None);
        };
        let name = self.parse_name_marker_with_stops(
            codes::EXPECTED_PARAMETER_NAME,
            "expected parameter name",
            NameContext::ValueParameter,
            recovery_stops,
        )?;
        let colon_span = if self.current_is_symbol(Symbol::Colon) {
            self.bump()?.span()
        } else {
            let primary = self.empty_at(self.current()?.span().start())?;
            self.emit(
                codes::EXPECTED_PARAMETER_COLON,
                "expected parameter colon",
                primary,
            )?;
            primary
        };
        let type_ref =
            self.parse_type_ref(boundary.with(TypeStops::COMMA).with(TypeStops::EQUAL))?;
        if self.current_is_symbol(Symbol::Equal) {
            let primary = self.bump()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            let mut default_stops = DeclarationStops::EMPTY
                .with(DeclarationStops::COMMA)
                .with(DeclarationStops::RIGHT_PAREN)
                .with(DeclarationStops::RIGHT_BRACE);
            if recovery_stops.has_class_file_boundary() {
                default_stops = default_stops.with(DeclarationStops::CLASS_FILE);
            }
            self.recover_declaration_region(default_stops)?;
        }
        let end = self.previous_significant_end();
        Ok(Some(ClassField {
            span: self.span(start, end.max(start))?,
            visibility,
            kind,
            keyword_span,
            name,
            colon_span,
            type_ref,
        }))
    }

    pub(super) fn parse_classifier_body(
        &mut self,
        kind: ClassifierKind,
    ) -> Result<ClassifierBody, ParserInternalError> {
        if matches!(kind, ClassifierKind::EnumClass { .. }) {
            self.parse_enum_body()
        } else {
            let context = match kind {
                ClassifierKind::ValueClass { .. } => ClassMemberContext::ValueClass,
                ClassifierKind::Class { .. } => ClassMemberContext::Class,
                ClassifierKind::Interface { .. } => ClassMemberContext::Interface,
                ClassifierKind::Object { .. } => ClassMemberContext::Object,
                ClassifierKind::EnumClass { .. } => ClassMemberContext::Enum,
            };
            self.parse_ordinary_classifier_body(context)
        }
    }

    pub(super) fn parse_ordinary_classifier_body(
        &mut self,
        context: ClassMemberContext,
    ) -> Result<ClassifierBody, ParserInternalError> {
        let left_brace_span = self.bump()?.span();
        let members = self.parse_classifier_members(context)?;
        let right_brace_span = self.finish_classifier_body(left_brace_span)?;
        Ok(ClassifierBody {
            left_brace_span,
            variants: Vec::new(),
            enum_member_delimiter_span: None,
            members,
            right_brace_span,
        })
    }

    pub(super) fn parse_classifier_members(
        &mut self,
        context: ClassMemberContext,
    ) -> Result<Vec<ItemId>, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_classifier_members_inner(context);
        self.recursion_depth -= 1;
        result
    }

    fn parse_classifier_members_inner(
        &mut self,
        context: ClassMemberContext,
    ) -> Result<Vec<ItemId>, ParserInternalError> {
        let mut members = Vec::new();
        let mut companion_seen = false;
        while !self.current_is_symbol(Symbol::RightBrace)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            if self.current_is_symbol(Symbol::Semicolon) {
                let primary = self.bump()?.span();
                self.emit(codes::EXPECTED_MEMBER, "expected member", primary)?;
                continue;
            }
            let before = self.index;
            let member = self.parse_classifier_member(context, &mut companion_seen)?;
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            let member_end = self.ast.items().get(member)?.span().end();
            members.push(member);
            if self.current_is_symbol(Symbol::RightBrace)
                || matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                break;
            }
            if self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
                continue;
            }
            if self.gap_has_line_break(member_end, self.current()?.span().start())? {
                continue;
            }
            self.emit(
                codes::EXPECTED_MEMBER_SEPARATOR,
                "expected member separator",
                self.current()?.span(),
            )?;
            if self.class_member_start()? {
                continue;
            }
            self.recover_declaration_region(
                DeclarationStops::EMPTY
                    .with(DeclarationStops::RIGHT_BRACE)
                    .with(DeclarationStops::SEMICOLON)
                    .with(DeclarationStops::CLASS_MEMBER),
            )?;
            if self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
            }
        }
        Ok(members)
    }

    pub(super) fn parse_classifier_member(
        &mut self,
        context: ClassMemberContext,
        companion_seen: &mut bool,
    ) -> Result<ItemId, ParserInternalError> {
        let member_stops = self.root_expression_stops().with(Stops::RIGHT_BRACE);
        let receiver_policy = if matches!(context, ClassMemberContext::Companion) {
            ReceiverModifierPolicy::Rejected
        } else {
            ReceiverModifierPolicy::Allowed
        };
        let mut modifiers =
            self.parse_declaration_modifiers(context.allows_override(), receiver_policy)?;
        let primary = self.current()?.span();
        if !self.current_is_keyword(Keyword::Fun)
            && let Some(receiver) = modifiers.receiver_mode.take()
        {
            self.emit(
                codes::INVALID_DECLARATION_MODIFIER,
                "invalid declaration modifier",
                parameter_mode_span(receiver),
            )?;
        }
        if matches!(context, ClassMemberContext::Interface)
            && self.current_is_keyword(Keyword::Fun)
            && let Some(
                visibility @ (VisibilityModifier::Internal(_) | VisibilityModifier::Private(_)),
            ) = modifiers.visibility
        {
            self.emit(
                codes::INVALID_DECLARATION_MODIFIER,
                "invalid declaration modifier",
                visibility_span(visibility),
            )?;
        }
        let declaration = if self.current_is_keyword(Keyword::Companion) {
            if !context.allows_companion() || *companion_seen {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    primary,
                )?;
            }
            *companion_seen = true;
            self.parse_companion_object()?
        } else if self.current_is_keyword(Keyword::Fun) {
            self.parse_function_declaration(member_stops, false)?
        } else if self.current_identifier_is("deinit")? {
            if !context.allows_deinit() {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    primary,
                )?;
            }
            if modifiers.visibility.is_some()
                || modifiers.override_span.is_some()
                || modifiers.receiver_mode.is_some()
            {
                let mod_span = declaration_modifier_start(modifiers)
                    .and_then(|start| self.span(start, primary.start()).ok())
                    .unwrap_or(primary);
                self.emit(
                    codes::INVALID_DECLARATION_MODIFIER,
                    "invalid declaration modifier",
                    mod_span,
                )?;
            }
            self.parse_deinit_declaration(member_stops)?
        } else if self.current_is_keyword(Keyword::Const) {
            if !context.allows_constant() {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    primary,
                )?;
            }
            self.parse_constant_declaration(member_stops.with(Stops::SEMICOLON))?
        } else if self.current_is_keyword(Keyword::Val) || self.current_is_keyword(Keyword::Var) {
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            let kind = if self.current_is_keyword(Keyword::Var) {
                VariableKind::Var
            } else {
                VariableKind::Val
            };
            let keyword = self.bump()?.span();
            self.parse_variable_declaration(keyword, kind, member_stops)?
        } else if self.classifier_declaration_start()? {
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            self.parse_classifier_declaration(false)?
        } else {
            let boundary = self.current_is_symbol(Symbol::RightBrace)
                || matches!(self.current()?.kind(), LexemeKind::Eof);
            let primary = if boundary {
                self.empty_at(primary.start())?
            } else {
                primary
            };
            let start = primary.start();
            if !self.is_poison() {
                if self.current_identifier_is("constructor")?
                    || self.current_identifier_is("init")?
                    || self.current_identifier_is("by")?
                {
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        primary,
                    )?;
                } else {
                    self.emit(codes::EXPECTED_MEMBER, "expected member", primary)?;
                }
            }
            let end = self.recover_declaration_region(
                DeclarationStops::EMPTY
                    .with(DeclarationStops::RIGHT_BRACE)
                    .with(DeclarationStops::SEMICOLON)
                    .with(DeclarationStops::CLASS_MEMBER),
            )?;
            self.add_item(self.span(start, end.max(primary.end()))?, Item::Error)?
        };
        self.wrap_modified_item(modifiers, declaration)
    }

    pub(super) fn parse_companion_object(&mut self) -> Result<ItemId, ParserInternalError> {
        let companion_span = self.bump()?.span();
        let object_span = if self.current_is_keyword(Keyword::Object) {
            let object_span = self.bump()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "write `companion { ... }`; `companion object` is not Koven syntax",
                object_span,
            )?;
            object_span
        } else {
            self.empty_at(self.current()?.span().start())?
        };
        let body = if self.current_is_symbol(Symbol::LeftBrace) {
            self.parse_ordinary_classifier_body(ClassMemberContext::Companion)?
        } else {
            let primary = self.current()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            ClassifierBody {
                left_brace_span: self.empty_at(primary.start())?,
                variants: Vec::new(),
                enum_member_delimiter_span: None,
                members: Vec::new(),
                right_brace_span: None,
            }
        };
        let end = body
            .right_brace_span
            .map(Span::end)
            .unwrap_or_else(|| object_span.end().max(companion_span.end()));
        self.add_item(
            self.span(companion_span.start(), end)?,
            Item::Companion(Box::new(CompanionObject {
                companion_span,
                object_span,
                body,
            })),
        )
    }

    pub(super) fn parse_deinit_declaration(
        &mut self,
        stops: Stops,
    ) -> Result<ItemId, ParserInternalError> {
        let deinit_span = self.bump()?.span();
        let left_paren_span = if self.current_is_symbol(Symbol::LeftParen) {
            self.bump()?.span()
        } else {
            let primary = self.current()?.span();
            self.emit(
                codes::EXPECTED_CLOSING_DELIMITER,
                "expected '(' after deinit",
                primary,
            )?;
            self.empty_at(primary.start())?
        };
        let right_paren_span = if self.current_is_symbol(Symbol::RightParen) {
            Some(self.bump()?.span())
        } else {
            let primary = self.current()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "deinit cannot declare parameters",
                primary,
            )?;
            while !self.current_is_symbol(Symbol::RightParen)
                && !self.current_is_symbol(Symbol::LeftBrace)
                && !matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                self.bump()?;
            }
            if self.current_is_symbol(Symbol::RightParen) {
                Some(self.bump()?.span())
            } else {
                None
            }
        };
        let body = if self.current_is_symbol(Symbol::LeftBrace) {
            self.parse_block_statement(stops)?
        } else {
            let primary = self.current()?.span();
            self.emit(
                codes::EXPECTED_BLOCK,
                "expected block body for deinit",
                primary,
            )?;
            self.add_statement(primary, Statement::Error)?
        };
        let end = self
            .ast
            .statements()
            .get(body)
            .map(|st| st.span().end())
            .unwrap_or(deinit_span.end());
        self.add_item(
            self.span(deinit_span.start(), end)?,
            Item::Deinit {
                deinit_span,
                left_paren_span,
                right_paren_span,
                body,
            },
        )
    }

    pub(super) fn parse_enum_body(&mut self) -> Result<ClassifierBody, ParserInternalError> {
        let left_brace_span = self.bump()?.span();
        let mut variants = Vec::new();
        let mut delimiter = None;
        if self.current_is_symbol(Symbol::RightBrace) || self.current_is_symbol(Symbol::Semicolon) {
            self.emit(
                codes::EXPECTED_ENUM_VARIANT,
                "expected enum variant",
                self.current()?.span(),
            )?;
        }
        while !self.current_is_symbol(Symbol::RightBrace)
            && !self.current_is_symbol(Symbol::Semicolon)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            if !self.current_is_identifier() {
                if self.class_member_start()? {
                    self.emit(
                        codes::EXPECTED_ENUM_MEMBER_DELIMITER,
                        "expected enum member delimiter",
                        self.current()?.span(),
                    )?;
                    break;
                }
                let primary = self.current()?.span();
                if !self.is_poison() {
                    self.emit(
                        codes::EXPECTED_ENUM_VARIANT,
                        "expected enum variant",
                        primary,
                    )?;
                }
                self.recover_declaration_region(
                    DeclarationStops::EMPTY
                        .with(DeclarationStops::COMMA)
                        .with(DeclarationStops::RIGHT_BRACE)
                        .with(DeclarationStops::SEMICOLON)
                        .with(DeclarationStops::ENUM_VARIANT),
                )?;
            } else {
                variants.push(self.parse_enum_variant()?);
            }
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                if self.current_is_symbol(Symbol::RightBrace)
                    || self.current_is_symbol(Symbol::Semicolon)
                {
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        comma,
                    )?;
                }
                continue;
            }
            if self.current_is_symbol(Symbol::RightBrace)
                || self.current_is_symbol(Symbol::Semicolon)
                || matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                break;
            }
            if self.class_member_start()? {
                self.emit(
                    codes::EXPECTED_ENUM_MEMBER_DELIMITER,
                    "expected enum member delimiter",
                    self.current()?.span(),
                )?;
                break;
            }
            self.emit(
                codes::EXPECTED_ENUM_VARIANT_SEPARATOR,
                "expected enum variant separator",
                self.current()?.span(),
            )?;
            if self.current_is_identifier() {
                continue;
            }
        }
        let members = if self.current_is_symbol(Symbol::Semicolon) {
            let separator = self.bump()?.span();
            delimiter = Some(separator);
            let members = self.parse_classifier_members(ClassMemberContext::Enum)?;
            if members.is_empty() {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    separator,
                )?;
            }
            members
        } else if self.class_member_start()? {
            self.parse_classifier_members(ClassMemberContext::Enum)?
        } else {
            Vec::new()
        };
        let right_brace_span = self.finish_classifier_body(left_brace_span)?;
        Ok(ClassifierBody {
            left_brace_span,
            variants,
            enum_member_delimiter_span: delimiter,
            members,
            right_brace_span,
        })
    }

    pub(super) fn parse_enum_variant(&mut self) -> Result<EnumVariant, ParserInternalError> {
        let name = NameMarker::Present(self.bump()?.span());
        let start = marker_span(name).start();
        let mut parameters = Vec::new();
        let mut left_paren_span = None;
        let mut right_paren_span = None;
        if self.current_is_symbol(Symbol::LeftParen) {
            let opener = self.bump()?.span();
            left_paren_span = Some(opener);
            if self.current_is_symbol(Symbol::RightParen) {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    self.current()?.span(),
                )?;
            }
            let payload_stops = Stops::ROOT
                .with(Stops::RIGHT_PAREN)
                .with(Stops::RIGHT_BRACE)
                .with(Stops::SEMICOLON);
            while !payload_stops.contains(self.current()?) {
                if self.current_is_symbol(Symbol::Comma) {
                    let primary = self.bump()?.span();
                    self.emit(
                        codes::EXPECTED_CONSTRUCTOR_FIELD,
                        "expected constructor field",
                        primary,
                    )?;
                    continue;
                }
                parameters.push(self.parse_enum_variant_parameter(payload_stops)?);
                if !payload_stops.contains(self.current()?)
                    && !self.current_is_symbol(Symbol::Comma)
                {
                    if !self.is_poison() {
                        self.emit(
                            codes::EXPECTED_CONSTRUCTOR_SEPARATOR,
                            "expected constructor separator",
                            self.current()?.span(),
                        )?;
                    }
                    if self.current_is_identifier() {
                        continue;
                    }
                    self.recover_declaration_region(
                        DeclarationStops::EMPTY
                            .with(DeclarationStops::COMMA)
                            .union(DeclarationStops::from_expression(payload_stops)),
                    )?;
                }
                if self.current_is_symbol(Symbol::Comma) {
                    let comma = self.bump()?.span();
                    if self.current_is_symbol(Symbol::RightParen) {
                        self.emit(
                            codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                            "unsupported class-family form",
                            comma,
                        )?;
                    }
                }
            }
            if self.current_is_symbol(Symbol::RightParen) {
                right_paren_span = Some(self.bump()?.span());
            } else {
                self.emit_closing(self.empty_at(self.current()?.span().start())?, opener)?;
            }
        }
        let end = self.previous_significant_end();
        Ok(EnumVariant {
            span: self.span(start, end)?,
            name,
            left_paren_span,
            parameters,
            right_paren_span,
        })
    }

    pub(super) fn parse_enum_variant_parameter(
        &mut self,
        payload_stops: Stops,
    ) -> Result<EnumVariantParameter, ParserInternalError> {
        let start = self.current()?.span().start();
        let is_invalid_mode_modifier = (self.current_identifier_is("borrow")?
            || self.current_identifier_is("inout")?
            || self.current_identifier_is("own")?)
            && self.peek_is_identifier(1);
        if is_invalid_mode_modifier
            || matches!(
                self.current()?.kind(),
                LexemeKind::Token(TokenKind::Keyword(
                    Keyword::Val | Keyword::Var | Keyword::Vararg
                ))
            )
        {
            let primary = self.bump()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
        }
        let name = self.parse_name_marker_with_stops(
            codes::EXPECTED_PARAMETER_NAME,
            "expected parameter name",
            NameContext::ValueParameter,
            DeclarationStops::from_expression(payload_stops),
        )?;
        let colon_span = if self.current_is_symbol(Symbol::Colon) {
            self.bump()?.span()
        } else {
            let primary = self.empty_at(self.current()?.span().start())?;
            self.emit(
                codes::EXPECTED_PARAMETER_COLON,
                "expected parameter colon",
                primary,
            )?;
            primary
        };
        let type_ref =
            self.parse_type_ref(TypeStops::from_expression(payload_stops).with(TypeStops::COMMA))?;
        let end = self.previous_significant_end().max(start);
        Ok(EnumVariantParameter {
            span: self.span(start, end)?,
            name,
            colon_span,
            type_ref,
        })
    }

    pub(super) fn finish_classifier_body(
        &mut self,
        opener: Span,
    ) -> Result<Option<Span>, ParserInternalError> {
        if self.current_is_symbol(Symbol::RightBrace) {
            Ok(Some(self.bump()?.span()))
        } else {
            let current = self.current()?;
            if !self.is_poison_kind(current.kind()) {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
            Ok(None)
        }
    }
}
