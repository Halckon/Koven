use super::*;

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

impl Parser<'_> {
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
        if (self.current_identifier_is("borrow")?
            || self.current_identifier_is("inout")?
            || self.current_identifier_is("own")?)
            && self.peek_is_keyword(1, Keyword::Fun)
        {
            return Ok(true);
        }
        if self.current_identifier_is("deinit")? {
            return Ok(true);
        }
        Ok(class_member_start_kind(self.current()?.kind()))
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
                & !(Self::LAMBDA_COMMA | Self::ARROW | Self::FILE_DECLARATION),
            block_elements: false,
            when_entry_body: self.when_entry_body,
        }
    }

    pub(super) const fn without_file_declaration_stop(self) -> Self {
        Self {
            delimiters: self.delimiters & !Self::FILE_DECLARATION,
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
                self.delimiters & Self::FILE_DECLARATION != 0
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
pub(super) struct TypeStops(u16);

impl TypeStops {
    pub(super) const EQUAL: u16 = 1 << 5;
    pub(super) const LEFT_BRACE: u16 = 1 << 6;
    pub(super) const RIGHT_BRACE: u16 = 1 << 7;
    pub(super) const BLOCK_ELEMENT: u16 = 1 << 8;
    pub(super) const fn empty() -> Self {
        Self(0)
    }

    pub(super) const COMMA: u16 = 1 << 0;
    pub(super) const GREATER: u16 = 1 << 1;
    pub(super) const RIGHT_PAREN: u16 = 1 << 2;
    pub(super) const RIGHT_BRACKET: u16 = 1 << 3;
    pub(super) const INTERPOLATION_END: u16 = 1 << 4;
    pub(super) const FILE: u16 = 1 << 9;
    pub(super) const ARROW: u16 = 1 << 10;

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
        Self(bits)
    }

    pub(super) const fn with(self, flag: u16) -> Self {
        Self(self.0 | flag)
    }

    pub(super) const fn without_block_elements(self) -> Self {
        Self(self.0 & !(Self::BLOCK_ELEMENT | Self::LEFT_BRACE | Self::FILE))
    }

    pub(super) fn contains(self, lexeme: Lexeme) -> bool {
        match lexeme.kind() {
            LexemeKind::Eof => true,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)) => self.0 & Self::COMMA != 0,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)) => self.0 & Self::GREATER != 0,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                self.0 & Self::RIGHT_PAREN != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => {
                self.0 & Self::RIGHT_BRACKET != 0
            }
            LexemeKind::Token(TokenKind::InterpolationEnd) => self.0 & Self::INTERPOLATION_END != 0,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)) => self.0 & Self::EQUAL != 0,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow)) => self.0 & Self::ARROW != 0,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) => {
                self.0 & Self::LEFT_BRACE != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)) => {
                self.0 & Self::RIGHT_BRACE != 0
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
                if self.0 & Self::BLOCK_ELEMENT != 0 =>
            {
                true
            }
            kind if self.0 & Self::BLOCK_ELEMENT != 0 && unsupported_block_element_kind(kind) => {
                true
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)) if self.0 & Self::FILE != 0 => {
                true
            }
            kind if self.0 & Self::FILE != 0 && file_construct_start_kind(kind) => true,
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
    const ARROW: u32 = 1 << 15;
    const ELSE: u32 = 1 << 16;
    const CONTROL_EXPRESSION: u32 = 1 << 17;

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
            || (self.0 & Self::CLASS_MEMBER != 0 && class_member_start_kind(lexeme.kind()))
            || (self.0 & Self::ENUM_VARIANT != 0
                && matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier)))
            || (self.0 & Self::CONTROL_EXPRESSION != 0
                && control_expression_start_kind(lexeme.kind()))
            || (self.0 & Self::FILE.0 != 0 && file_construct_start_kind(lexeme.kind()))
    }
}
