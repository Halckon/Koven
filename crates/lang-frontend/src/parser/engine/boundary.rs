use super::*;
use std::cell::{Cell, RefCell};

pub(super) fn unsupported_block_element_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Const
                | Keyword::Fun
                | Keyword::For
                | Keyword::While
                | Keyword::Class
                | Keyword::Interface
                | Keyword::Enum
                | Keyword::Object
                | Keyword::Companion
        ))
    )
}

pub(super) fn control_expression_start_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::If
                | Keyword::When
                | Keyword::Return
                | Keyword::Break
                | Keyword::Continue
                | Keyword::Super
        ))
    )
}

pub(super) fn simple_declaration_start_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Val
                | Keyword::Var
                | Keyword::Const
                | Keyword::Fun
                | Keyword::Class
                | Keyword::Interface
                | Keyword::Enum
                | Keyword::Object
                | Keyword::Public
                | Keyword::Internal
                | Keyword::Private
                | Keyword::Override
        ))
    )
}

pub(super) fn classifier_declaration_start_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Class | Keyword::Interface | Keyword::Enum | Keyword::Object
        ))
    )
}

pub(super) fn class_field_start_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Val
                | Keyword::Var
                | Keyword::Public
                | Keyword::Internal
                | Keyword::Private
                | Keyword::Override
        ))
    )
}

pub(super) fn class_member_start_kind(kind: LexemeKind) -> bool {
    classifier_declaration_start_kind(kind)
        || matches!(
            kind,
            LexemeKind::Token(TokenKind::Keyword(
                Keyword::Fun
                    | Keyword::Const
                    | Keyword::Val
                    | Keyword::Var
                    | Keyword::Companion
                    | Keyword::Public
                    | Keyword::Internal
                    | Keyword::Private
                    | Keyword::Override
            ))
        )
}

#[derive(Clone, Copy)]
pub(super) enum TypeParameterOwner {
    Function,
    Classifier,
    FileClassifier,
}

pub(super) struct FileAngleRecovery {
    pub(super) groups: RefCell<recovery::FileAngleGroups>,
    pub(super) boundary: Cell<Option<(usize, bool)>>,
    pub(super) significant_start: Cell<Option<(usize, usize)>>,
}

impl Parser<'_> {
    pub(super) fn at_file_angle_boundary(
        &self,
        current: Lexeme,
    ) -> Result<bool, ParserInternalError> {
        let Some(recovery) = &self.file_angle else {
            return Ok(false);
        };
        let explicit = matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
        );
        let offset = current.span().start();
        let memo = &recovery.boundary;
        if let Some((previous, boundary)) = memo.get()
            && previous == offset
        {
            return Ok(boundary);
        }
        // 嵌套 TypeRef 可在同一边界逐层退栈，后继 trivia 和声明前缀只读一次。
        let boundary = self.file_declaration_prefix_at(usize::from(explicit))?
            && (explicit || self.file_separator_region_has_line_break()?)
            && recovery.groups.borrow_mut().at_boundary_baseline(
                self.lexed,
                &self.lexical_recoveries.terminal_owner_events,
                self.current_raw()?,
            )?;
        memo.set(Some((offset, boundary)));
        Ok(boundary)
    }

    fn file_declaration_prefix_at(&self, start: usize) -> Result<bool, ParserInternalError> {
        let ordinal = if self.peek(start).is_some_and(|token| {
            matches!(
                token.kind(),
                LexemeKind::Token(TokenKind::Keyword(
                    Keyword::Public | Keyword::Internal | Keyword::Private
                ))
            )
        }) {
            start + 1
        } else {
            start
        };
        Ok(self.peek(ordinal).is_some_and(|token| {
            classifier_declaration_start_kind(token.kind())
                || matches!(
                    token.kind(),
                    LexemeKind::Token(TokenKind::Keyword(
                        Keyword::Val | Keyword::Var | Keyword::Const | Keyword::Fun
                    ))
                )
        }) || (ordinal == start
            && (self.peek_is_keyword(start, Keyword::Package)
                || self.peek_is_keyword(start, Keyword::Import)))
            || self.value_class_start_at(ordinal)?
            || (self.receiver_modifier_at(ordinal)?.is_some()
                && self.peek_is_keyword(ordinal + 1, Keyword::Fun)))
    }

    pub(super) fn value_class_start_at(&self, ordinal: usize) -> Result<bool, ParserInternalError> {
        Ok(self.peek_is_keyword(ordinal + 1, Keyword::Class)
            && self.peek_identifier_is(ordinal, "value")?)
    }

    pub(super) fn at_supertype_recovery_boundary(
        &self,
        raw: usize,
        current: Lexeme,
        owner: super::class::ClassifierHeaderOwner,
        element_start: bool,
    ) -> Result<bool, ParserInternalError> {
        if let Some((previous, mode, start, boundary)) =
            self.lexical_recoveries.supertype_boundary.get()
            && (previous, mode, start) == (raw, owner, element_start)
        {
            return Ok(boundary);
        }
        use super::class::ClassifierHeaderOwner::{Declaration, File, Member};
        let boundary = self.is_file_declaration_boundary(current)
            || match owner {
                File => self.at_class_header_file_boundary(element_start)?,
                Member if !element_start => {
                    self.current_is_symbol(Symbol::Semicolon)
                        || self.class_member_recovery_start()?
                }
                Member => {
                    if self.current_identifier_is("deinit")? {
                        self.peek_is_symbol(1, Symbol::LeftParen)
                    } else if self.current_identifier_is("value")? {
                        match self.peek(1) {
                            Some(next)
                                if matches!(
                                    next.kind(),
                                    LexemeKind::Token(TokenKind::Keyword(Keyword::Class))
                                ) =>
                            {
                                !self
                                    .gap_has_line_break(current.span().end(), next.span().start())?
                            }
                            _ => false,
                        }
                    } else {
                        class_member_start_kind(current.kind())
                            || self.current_is_symbol(Symbol::Semicolon)
                            || (self.receiver_modifier_at(0)?.is_some()
                                && self.peek_is_keyword(1, Keyword::Fun))
                    }
                }
                Declaration => false,
            };
        self.lexical_recoveries
            .supertype_boundary
            .set(Some((raw, owner, element_start, boundary)));
        Ok(boundary)
    }

    pub(super) fn at_class_header_file_boundary(
        &self,
        allow_identifier: bool,
    ) -> Result<bool, ParserInternalError> {
        if self.current_is_symbol(Symbol::Semicolon) {
            return Ok(true);
        }
        let current = self.current()?;
        let ordinal = usize::from(matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Keyword(
                Keyword::Public | Keyword::Internal | Keyword::Private
            ))
        ));
        if self.peek(ordinal).is_some_and(|token| {
            classifier_declaration_start_kind(token.kind())
                || matches!(
                    token.kind(),
                    LexemeKind::Token(TokenKind::Keyword(
                        Keyword::Fun | Keyword::Const | Keyword::Package | Keyword::Import
                    ))
                )
        }) {
            return Ok(true);
        }
        if !self.value_class_start_at(ordinal)? {
            return Ok(false);
        }
        if ordinal != 0 || !allow_identifier {
            return Ok(true);
        }
        let class = self
            .peek(1)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        // 跨行的裸 `value` 仍可作为名称或类型；仅在下一行的完整前缀处归还文件层。
        Ok(
            self.gap_has_line_break(self.previous_significant_end(), current.span().start())?
                && !self.gap_has_line_break(current.span().end(), class.span().start())?,
        )
    }

    pub(super) fn classifier_declaration_start(&self) -> Result<bool, ParserInternalError> {
        if self.current_identifier_is("value")?
            && (self.peek_is_keyword(1, Keyword::Class) || self.peek_is_identifier(1))
        {
            return Ok(true);
        }
        Ok(classifier_declaration_start_kind(self.current()?.kind()))
    }

    pub(super) fn class_member_start(&self) -> Result<bool, ParserInternalError> {
        if self.classifier_declaration_start()? {
            return Ok(true);
        }
        if self.receiver_modifier_at(0)?.is_some() && self.peek_is_keyword(1, Keyword::Fun) {
            return Ok(true);
        }
        if self.current_identifier_is("deinit")? {
            return Ok(true);
        }
        Ok(class_member_start_kind(self.current()?.kind()))
    }

    pub(super) fn receiver_modifier_at(
        &self,
        ordinal: usize,
    ) -> Result<Option<ParameterModeMarker>, ParserInternalError> {
        let Some(lexeme) = self.peek(ordinal) else {
            return Ok(None);
        };
        if !matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier)) {
            return Ok(None);
        }
        let span = lexeme.span();
        Ok(match self.sources.slice(span)? {
            "borrow" => Some(ParameterModeMarker::Borrow(span)),
            "inout" => Some(ParameterModeMarker::Inout(span)),
            "own" => Some(ParameterModeMarker::Own(span)),
            _ => None,
        })
    }

    pub(super) fn class_member_recovery_start(&self) -> Result<bool, ParserInternalError> {
        Ok(self.class_member_start()?
            || (self.receiver_modifier_at(0)?.is_some()
                && (self.receiver_modifier_at(1)?.is_some()
                    || self
                        .peek(1)
                        .is_some_and(|next| class_member_start_kind(next.kind())))))
    }
}

pub(super) fn visibility_span(visibility: VisibilityModifier) -> Span {
    match visibility {
        VisibilityModifier::Public(span)
        | VisibilityModifier::Internal(span)
        | VisibilityModifier::Private(span) => span,
    }
}

pub(super) fn declaration_modifier_start(modifiers: DeclarationModifiers) -> Option<usize> {
    modifiers
        .visibility
        .map(visibility_span)
        .map(Span::start)
        .into_iter()
        .chain(modifiers.override_span.map(Span::start))
        .chain(
            modifiers
                .receiver_mode
                .map(parameter_mode_span)
                .map(Span::start),
        )
        .min()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReceiverModifierPolicy {
    /// 当前语法位置另有定向诊断，不由通用 modifier scanner 消费 receiver token。
    NotRecognized,
    /// 消费 receiver token 并以 L0076 拒绝，但不写入 AST modifier。
    Rejected,
    /// instance-function slot 可保存 receiver token；最终仍须由 member parser 确认后接 `fun`。
    Allowed,
}

pub(super) fn classifier_keyword_start(kind: ClassifierKind) -> usize {
    match kind {
        ClassifierKind::ValueClass { value_span, .. } => value_span.start(),
        ClassifierKind::Class { class_span } => class_span.start(),
        ClassifierKind::Interface { interface_span } => interface_span.start(),
        ClassifierKind::EnumClass { enum_span, .. } => enum_span.start(),
        ClassifierKind::Object { object_span } => object_span.start(),
    }
}

#[derive(Clone, Copy)]
pub(super) enum ClassMemberContext {
    ValueClass,
    Class,
    Interface,
    Enum,
    Object,
    Companion,
}

impl ClassMemberContext {
    pub(super) const fn allows_override(self) -> bool {
        matches!(
            self,
            Self::ValueClass | Self::Class | Self::Enum | Self::Object
        )
    }

    pub(super) const fn allows_companion(self) -> bool {
        !matches!(self, Self::Object | Self::Companion)
    }

    pub(super) const fn allows_constant(self) -> bool {
        matches!(self, Self::Object | Self::Companion)
    }

    pub(super) const fn allows_deinit(self) -> bool {
        matches!(self, Self::Class)
    }
}

pub(super) fn file_construct_start_kind(kind: LexemeKind) -> bool {
    simple_declaration_start_kind(kind)
        || matches!(
            kind,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Package | Keyword::Import))
        )
}

#[derive(Clone, Copy)]
pub(super) struct Stops {
    pub(super) delimiters: u16,
    pub(super) block_elements: bool,
    pub(super) when_entry_body: bool,
}

impl Stops {
    pub(super) const ROOT: Self = Self {
        delimiters: 0,
        block_elements: false,
        when_entry_body: false,
    };
    pub(super) const FILE: Self = Self {
        delimiters: Self::FILE_DECLARATION,
        block_elements: false,
        when_entry_body: false,
    };
    pub(super) const RIGHT_PAREN: u16 = 1 << 0;
    pub(super) const RIGHT_BRACKET: u16 = 1 << 1;
    pub(super) const COMMA: u16 = 1 << 2;
    pub(super) const INTERPOLATION_END: u16 = 1 << 3;
    pub(super) const RIGHT_BRACE: u16 = 1 << 4;
    pub(super) const ARROW: u16 = 1 << 5;
    pub(super) const LAMBDA_COMMA: u16 = 1 << 6;
    pub(super) const FILE_DECLARATION: u16 = 1 << 7;
    pub(super) const ELSE: u16 = 1 << 8;
    pub(super) const SEMICOLON: u16 = 1 << 9;
    pub(super) const HARD_DELIMITERS: u16 =
        Self::RIGHT_PAREN | Self::RIGHT_BRACKET | Self::INTERPOLATION_END;

    pub(super) const fn block_expression(outer_stops: Self) -> Self {
        Self {
            delimiters: (outer_stops.delimiters & Self::HARD_DELIMITERS) | Self::RIGHT_BRACE,
            block_elements: true,
            when_entry_body: false,
        }
    }

    pub(super) const fn lambda_expression(outer_stops: Self) -> Self {
        Self {
            delimiters: (outer_stops.delimiters & Self::HARD_DELIMITERS)
                | Self::RIGHT_BRACE
                | Self::LAMBDA_COMMA
                | Self::ARROW,
            block_elements: true,
            when_entry_body: false,
        }
    }

    pub(super) const fn with(self, flag: u16) -> Self {
        Self {
            delimiters: self.delimiters | flag,
            block_elements: self.block_elements,
            when_entry_body: self.when_entry_body,
        }
    }

    pub(super) const fn as_when_entry_body(self) -> Self {
        Self {
            when_entry_body: true,
            ..self
        }
    }

    pub(super) const fn without_lambda_body_soft_stops(self) -> Self {
        Self {
            delimiters: self.delimiters
                & !(Self::LAMBDA_COMMA | Self::ARROW | Self::FILE_DECLARATION | Self::SEMICOLON),
            block_elements: false,
            when_entry_body: self.when_entry_body,
        }
    }

    pub(super) const fn without_file_and_semicolon_stops(self) -> Self {
        Self {
            delimiters: self.delimiters & !(Self::FILE_DECLARATION | Self::SEMICOLON),
            block_elements: self.block_elements,
            when_entry_body: self.when_entry_body,
        }
    }

    pub(super) fn contains_hard(self, lexeme: Lexeme) -> bool {
        match lexeme.kind() {
            LexemeKind::Eof => true,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                self.delimiters & Self::RIGHT_PAREN != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => {
                self.delimiters & Self::RIGHT_BRACKET != 0
            }
            LexemeKind::Token(TokenKind::InterpolationEnd) => {
                self.delimiters & Self::INTERPOLATION_END != 0
            }
            _ => false,
        }
    }

    pub(super) fn contains(self, lexeme: Lexeme) -> bool {
        match lexeme.kind() {
            LexemeKind::Eof => true,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                self.delimiters & Self::RIGHT_PAREN != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => {
                self.delimiters & Self::RIGHT_BRACKET != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)) => {
                self.delimiters & Self::RIGHT_BRACE != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)) => {
                self.delimiters & (Self::COMMA | Self::LAMBDA_COMMA) != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)) => {
                self.delimiters & (Self::FILE_DECLARATION | Self::SEMICOLON) != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow)) => {
                self.delimiters & Self::ARROW != 0
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::Else)) => {
                self.delimiters & Self::ELSE != 0
            }
            LexemeKind::Token(TokenKind::InterpolationEnd) => {
                self.delimiters & Self::INTERPOLATION_END != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
            | LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
                if self.block_elements =>
            {
                true
            }
            kind if self.block_elements && unsupported_block_element_kind(kind) => true,
            kind if self.block_elements && control_expression_start_kind(kind) => true,
            kind if self.delimiters & Self::FILE_DECLARATION != 0
                && file_construct_start_kind(kind) =>
            {
                true
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct TypeStops {
    delimiters: u16,
    supertype_owner: Option<super::class::ClassifierHeaderOwner>,
    proven_type_closer: Option<usize>,
}

impl TypeStops {
    pub(super) const EQUAL: u16 = 1 << 5;
    pub(super) const LEFT_BRACE: u16 = 1 << 6;
    pub(super) const RIGHT_BRACE: u16 = 1 << 7;
    pub(super) const BLOCK_ELEMENT: u16 = 1 << 8;
    pub(super) const fn empty() -> Self {
        Self {
            delimiters: 0,
            supertype_owner: None,
            proven_type_closer: None,
        }
    }

    pub(super) const COMMA: u16 = 1 << 0;
    pub(super) const GREATER: u16 = 1 << 1;
    pub(super) const RIGHT_PAREN: u16 = 1 << 2;
    pub(super) const RIGHT_BRACKET: u16 = 1 << 3;
    pub(super) const INTERPOLATION_END: u16 = 1 << 4;
    pub(super) const FILE: u16 = 1 << 9;
    pub(super) const ARROW: u16 = 1 << 10;
    pub(super) const EXTENSION_NAME: u16 = 1 << 11;
    const SEMICOLON: u16 = 1 << 12;
    pub(super) const CLASS_BODY: u16 = 1 << 13;
    pub(super) const CLASS_FILE: u16 = 1 << 14;

    pub(super) const fn proven_type_closer(self) -> Option<usize> {
        self.proven_type_closer
    }

    pub(super) const fn for_proven_function_parameters(self, closer: usize) -> Self {
        let outer = Self::GREATER
            | Self::RIGHT_BRACKET
            | Self::ARROW
            | Self::LEFT_BRACE
            | Self::RIGHT_BRACE
            | Self::CLASS_BODY;
        Self {
            delimiters: self.delimiters & !outer,
            proven_type_closer: Some(closer),
            ..self
        }
    }

    pub(super) const fn for_supertype(self, owner: super::class::ClassifierHeaderOwner) -> Self {
        Self {
            delimiters: self.delimiters | Self::SEMICOLON,
            supertype_owner: Some(owner),
            ..self
        }
    }

    pub(super) const fn supertype_owner(self) -> Option<super::class::ClassifierHeaderOwner> {
        self.supertype_owner
    }

    pub(super) fn supertype_element_recovery(
        self,
        owner: super::class::ClassifierHeaderOwner,
    ) -> DeclarationStops {
        let mut stops = DeclarationStops::EMPTY
            .with(DeclarationStops::SUPERTYPE_SUFFIX)
            .with(DeclarationStops::TYPE_ELEMENT)
            .with(DeclarationStops::SEMICOLON);
        for (type_stop, declaration_stop) in [
            (Self::COMMA, DeclarationStops::COMMA),
            (Self::GREATER, DeclarationStops::GREATER),
            (Self::RIGHT_PAREN, DeclarationStops::RIGHT_PAREN),
            (Self::RIGHT_BRACKET, DeclarationStops::RIGHT_BRACKET),
            (Self::INTERPOLATION_END, DeclarationStops::INTERPOLATION_END),
            (Self::ARROW, DeclarationStops::ARROW),
            (
                Self::LEFT_BRACE | Self::CLASS_BODY,
                DeclarationStops::LEFT_BRACE,
            ),
            (Self::RIGHT_BRACE, DeclarationStops::RIGHT_BRACE),
        ] {
            if self.delimiters & type_stop != 0 {
                stops = stops.with(declaration_stop);
            }
        }
        match owner {
            super::class::ClassifierHeaderOwner::File => stops.with(DeclarationStops::CLASS_FILE),
            super::class::ClassifierHeaderOwner::Member => {
                stops.with(DeclarationStops::CLASS_MEMBER)
            }
            super::class::ClassifierHeaderOwner::Declaration => stops,
        }
    }

    pub(super) const fn has_class_file_boundary(self) -> bool {
        self.delimiters & Self::CLASS_FILE != 0
    }

    pub(super) const fn has_class_body_boundary(self) -> bool {
        self.delimiters & Self::CLASS_BODY != 0
    }

    pub(super) const fn ends_at_extension_name(self) -> bool {
        self.delimiters & Self::EXTENSION_NAME != 0
    }

    pub(super) const fn without_extension_name(self) -> Self {
        Self {
            delimiters: self.delimiters & !Self::EXTENSION_NAME,
            ..self
        }
    }

    pub(super) const fn from_expression(stops: Stops) -> Self {
        let mut bits = 0;
        if stops.delimiters & (Stops::COMMA | Stops::LAMBDA_COMMA) != 0 {
            bits |= Self::COMMA;
        }
        if stops.delimiters & Stops::RIGHT_PAREN != 0 {
            bits |= Self::RIGHT_PAREN;
        }
        if stops.delimiters & Stops::RIGHT_BRACKET != 0 {
            bits |= Self::RIGHT_BRACKET;
        }
        if stops.delimiters & Stops::INTERPOLATION_END != 0 {
            bits |= Self::INTERPOLATION_END;
        }
        if stops.delimiters & Stops::ARROW != 0 {
            bits |= Self::ARROW;
        }
        if stops.delimiters & Stops::RIGHT_BRACE != 0 {
            bits |= Self::RIGHT_BRACE;
        }
        if stops.block_elements {
            bits |= Self::BLOCK_ELEMENT | Self::LEFT_BRACE;
        }
        if stops.delimiters & Stops::FILE_DECLARATION != 0 {
            bits |= Self::FILE;
        }
        if stops.delimiters & Stops::SEMICOLON != 0 {
            bits |= Self::SEMICOLON;
        }
        Self::empty().with(bits)
    }

    pub(super) const fn with(self, flag: u16) -> Self {
        Self {
            delimiters: self.delimiters | flag,
            ..self
        }
    }

    pub(super) const fn without_block_elements(self) -> Self {
        // Explicit semicolons belong to the enclosing declaration even inside TypeRefs.
        Self {
            delimiters: self.delimiters
                & !(Self::BLOCK_ELEMENT | Self::LEFT_BRACE | Self::FILE | Self::CLASS_FILE),
            ..self
        }
    }

    pub(super) fn contains(self, lexeme: Lexeme) -> bool {
        match lexeme.kind() {
            LexemeKind::Eof => true,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)) => {
                self.delimiters & Self::COMMA != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)) => {
                self.delimiters & Self::GREATER != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                self.delimiters & Self::RIGHT_PAREN != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => {
                self.delimiters & Self::RIGHT_BRACKET != 0
            }
            LexemeKind::Token(TokenKind::InterpolationEnd) => {
                self.delimiters & Self::INTERPOLATION_END != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)) => {
                self.delimiters & Self::EQUAL != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow)) => {
                self.delimiters & Self::ARROW != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) => {
                self.delimiters & (Self::LEFT_BRACE | Self::CLASS_BODY) != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)) => {
                self.delimiters & Self::RIGHT_BRACE != 0
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
                if self.delimiters & Self::BLOCK_ELEMENT != 0 =>
            {
                true
            }
            kind if self.delimiters & Self::BLOCK_ELEMENT != 0
                && unsupported_block_element_kind(kind) =>
            {
                true
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)) => {
                self.delimiters & (Self::FILE | Self::SEMICOLON) != 0
            }
            kind if self.delimiters & Self::FILE != 0 && file_construct_start_kind(kind) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum NameContext {
    Declaration,
    LocalDeclaration,
    TypeParameter,
    ValueParameter,
}

impl NameContext {
    pub(super) fn is_stop(self, lexeme: Lexeme) -> bool {
        if matches!(self, Self::LocalDeclaration) && unsupported_block_element_kind(lexeme.kind()) {
            return true;
        }
        matches!(
            (self, lexeme.kind()),
            (
                Self::Declaration | Self::LocalDeclaration,
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::Colon | Symbol::Equal | Symbol::LeftParen,
                )),
            ) | (
                Self::LocalDeclaration,
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace | Symbol::RightBrace,)),
            ) | (
                Self::LocalDeclaration,
                LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var)),
            ) | (
                Self::TypeParameter,
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::Colon | Symbol::Comma | Symbol::Greater,
                )),
            ) | (
                Self::ValueParameter,
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::Colon | Symbol::Comma | Symbol::RightParen,
                )),
            )
        )
    }

    pub(super) const fn recovery_stops(self) -> DeclarationStops {
        match self {
            Self::Declaration => DeclarationStops::EMPTY
                .with(DeclarationStops::COLON)
                .with(DeclarationStops::EQUAL)
                .with(DeclarationStops::LEFT_PAREN),
            Self::LocalDeclaration => DeclarationStops::EMPTY
                .with(DeclarationStops::COLON)
                .with(DeclarationStops::EQUAL)
                .with(DeclarationStops::LEFT_PAREN)
                .with(DeclarationStops::LEFT_BRACE)
                .with(DeclarationStops::RIGHT_BRACE)
                .with(DeclarationStops::BLOCK_ELEMENT),
            Self::TypeParameter => DeclarationStops::EMPTY
                .with(DeclarationStops::COLON)
                .with(DeclarationStops::COMMA)
                .with(DeclarationStops::GREATER),
            Self::ValueParameter => DeclarationStops::EMPTY
                .with(DeclarationStops::COLON)
                .with(DeclarationStops::COMMA)
                .with(DeclarationStops::RIGHT_PAREN),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct DeclarationStops(u32);

impl DeclarationStops {
    pub(super) const EMPTY: Self = Self(0);
    pub(super) const COMMA: u32 = 1 << 0;
    pub(super) const RIGHT_PAREN: u32 = 1 << 1;
    pub(super) const GREATER: u32 = 1 << 2;
    pub(super) const LEFT_BRACE: u32 = 1 << 3;
    pub(super) const COLON: u32 = 1 << 4;
    pub(super) const EQUAL: u32 = 1 << 5;
    pub(super) const LEFT_PAREN: u32 = 1 << 6;
    pub(super) const RIGHT_BRACE: u32 = 1 << 7;
    pub(super) const BLOCK_ELEMENT: u32 = 1 << 8;
    pub(super) const RIGHT_BRACKET: u32 = 1 << 9;
    pub(super) const INTERPOLATION_END: u32 = 1 << 10;
    pub(super) const FILE: Self = Self(1 << 11);
    pub(super) const SEMICOLON: u32 = 1 << 12;
    pub(super) const CLASS_MEMBER: u32 = 1 << 13;
    pub(super) const ENUM_VARIANT: u32 = 1 << 14;
    pub(super) const ARROW: u32 = 1 << 15;
    const ELSE: u32 = 1 << 16;
    const CONTROL_EXPRESSION: u32 = 1 << 17;
    pub(super) const CLASS_BODY: u32 = 1 << 18;
    pub(super) const CLASS_FILE: u32 = 1 << 19;
    pub(super) const SUPERTYPE_SUFFIX: u32 = 1 << 20;
    const TYPE_ELEMENT: u32 = 1 << 21;
    pub(super) const LEADING_UNPROVED_ANGLE_COMMA: u32 = 1 << 22;
    pub(super) const DELEGATION_ERROR_TARGET: u32 = 1 << 23;
    pub(super) const DELEGATION_UNSUPPORTED_SUFFIX: u32 = 1 << 24;
    pub(super) const PROVEN_GROUPS: u32 = 1 << 25;

    pub(super) const fn requires_proven_groups(self) -> bool {
        self.0 & Self::PROVEN_GROUPS != 0
    }

    pub(super) const fn in_delegation_recovery(self) -> bool {
        self.0 & (Self::DELEGATION_ERROR_TARGET | Self::DELEGATION_UNSUPPORTED_SUFFIX) != 0
    }

    pub(super) const fn in_unsupported_delegation_suffix(self) -> bool {
        self.0 & Self::DELEGATION_UNSUPPORTED_SUFFIX != 0
    }

    pub(super) const fn keeps_unproved_angle_comma(self, leading: bool) -> bool {
        self.in_delegation_recovery()
            || (leading && self.0 & Self::LEADING_UNPROVED_ANGLE_COMMA != 0)
    }

    pub(super) const fn recovers_type_element(self) -> bool {
        self.0 & Self::TYPE_ELEMENT != 0
    }

    pub(super) const fn has_semicolon(self) -> bool {
        self.0 & Self::SEMICOLON != 0
    }

    pub(super) const fn has_supertype_suffix(self) -> bool {
        self.0 & Self::SUPERTYPE_SUFFIX != 0
    }

    pub(super) const fn has_greater(self) -> bool {
        self.0 & Self::GREATER != 0
    }

    pub(super) const fn has_class_file_boundary(self) -> bool {
        self.0 & Self::CLASS_FILE != 0
    }

    pub(super) const fn from_expression_hard(stops: Stops) -> Self {
        let mut bits = 0;
        if stops.delimiters & Stops::RIGHT_PAREN != 0 {
            bits |= Self::RIGHT_PAREN;
        }
        if stops.delimiters & Stops::RIGHT_BRACKET != 0 {
            bits |= Self::RIGHT_BRACKET;
        }
        if stops.delimiters & Stops::RIGHT_BRACE != 0 {
            bits |= Self::RIGHT_BRACE;
        }
        if stops.delimiters & Stops::INTERPOLATION_END != 0 {
            bits |= Self::INTERPOLATION_END;
        }
        Self(bits)
    }

    pub(super) const fn from_expression(stops: Stops) -> Self {
        let mut result = Self::from_expression_hard(stops);
        if stops.delimiters & (Stops::COMMA | Stops::LAMBDA_COMMA) != 0 {
            result.0 |= Self::COMMA;
        }
        if stops.delimiters & Stops::ARROW != 0 {
            result.0 |= Self::ARROW;
        }
        if stops.delimiters & Stops::ELSE != 0 {
            result.0 |= Self::ELSE;
        }
        if stops.delimiters & Stops::FILE_DECLARATION != 0 {
            result.0 |= Self::FILE.0;
        }
        if stops.delimiters & Stops::SEMICOLON != 0 {
            result.0 |= Self::SEMICOLON;
        }
        if stops.block_elements {
            result.0 |= Self::LEFT_BRACE | Self::BLOCK_ELEMENT | Self::CONTROL_EXPRESSION;
        }
        result
    }

    pub(super) const fn with(self, flag: u32) -> Self {
        Self(self.0 | flag)
    }

    pub(super) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub(super) fn contains_hard(self, lexeme: Lexeme, symbol: Option<Symbol>) -> bool {
        matches!(
            symbol,
            Some(Symbol::RightParen) if self.0 & Self::RIGHT_PAREN != 0
        ) || matches!(
            symbol,
            Some(Symbol::Greater) if self.0 & Self::GREATER != 0
        ) || matches!(
            symbol,
            Some(Symbol::LeftBrace) if self.0 & Self::LEFT_BRACE != 0
        ) || matches!(
            symbol,
            Some(Symbol::RightBrace) if self.0 & Self::RIGHT_BRACE != 0
        ) || matches!(
            symbol,
            Some(Symbol::RightBracket) if self.0 & Self::RIGHT_BRACKET != 0
        ) || (self.0 & Self::BLOCK_ELEMENT != 0
            && matches!(
                lexeme.kind(),
                LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
            ))
            || (self.0 & Self::BLOCK_ELEMENT != 0 && unsupported_block_element_kind(lexeme.kind()))
            || (self.0 & Self::INTERPOLATION_END != 0
                && matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::InterpolationEnd)
                ))
    }

    pub(super) fn contains_soft(self, lexeme: Lexeme, symbol: Option<Symbol>) -> bool {
        matches!(symbol, Some(Symbol::Comma) if self.0 & Self::COMMA != 0)
            || matches!(symbol, Some(Symbol::LeftBrace) if self.0 & Self::CLASS_BODY != 0)
            || matches!(symbol, Some(Symbol::Colon) if self.0 & Self::COLON != 0)
            || matches!(symbol, Some(Symbol::Equal) if self.0 & Self::EQUAL != 0)
            || matches!(
                symbol,
                Some(Symbol::LeftParen) if self.0 & Self::LEFT_PAREN != 0
            )
            || matches!(symbol, Some(Symbol::Semicolon) if self.0 & Self::FILE.0 != 0)
            || matches!(symbol, Some(Symbol::Semicolon) if self.0 & Self::SEMICOLON != 0)
            || matches!(symbol, Some(Symbol::Arrow) if self.0 & Self::ARROW != 0)
            || (self.0 & Self::ELSE != 0
                && matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Keyword(Keyword::Else))
                ))
            || (self.0 & Self::ENUM_VARIANT != 0
                && matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier)))
            || (self.0 & Self::CONTROL_EXPRESSION != 0
                && control_expression_start_kind(lexeme.kind()))
            || (self.0 & Self::FILE.0 != 0 && file_construct_start_kind(lexeme.kind()))
    }

    pub(super) const fn stops_at_class_member(self) -> bool {
        self.0 & Self::CLASS_MEMBER != 0
    }
}
