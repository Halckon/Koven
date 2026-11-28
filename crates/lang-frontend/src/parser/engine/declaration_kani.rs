use super::ParameterModeContext;
use super::declaration::{
    ConstValRecoveryAction, FunctionFormDispatch, ValueParamTypeAction, const_val_recovery_action,
    declaration_tail_emits_unexpected_trailing, function_form_dispatch,
    is_parameter_mode_follow_kind, value_param_default_emits_diagnostic, value_param_type_action,
};
use crate::lexer::{
    FloatLiteralSuffix, IntegerLiteralSuffix, InvalidKind, Keyword, LexemeKind, ReservedWord,
    Symbol, TokenKind, TriviaKind,
};

macro_rules! define_exhaustive_unit_enum_any {
    ($fn_name:ident, $enum_ty:ty, [$($variant:ident),+ $(,)?]) => {
        fn $fn_name() -> $enum_ty {
            #[deny(unreachable_patterns)]
            const fn assert_exhaustive(value: $enum_ty) {
                match value {
                    $(<$enum_ty>::$variant => {},)+
                }
            }

            const ALL: &[$enum_ty] = &[$(<$enum_ty>::$variant),+];
            let idx: usize = kani::any();
            kani::assume(idx < ALL.len());
            let picked = ALL[idx];
            assert_exhaustive(picked);
            picked
        }
    };
}

define_exhaustive_unit_enum_any!(
    any_integer_literal_suffix,
    IntegerLiteralSuffix,
    [None, Long, Unsigned, UnsignedLong]
);

define_exhaustive_unit_enum_any!(any_float_literal_suffix, FloatLiteralSuffix, [None, Float]);

define_exhaustive_unit_enum_any!(
    any_keyword,
    Keyword,
    [
        Class, Companion, Const, Enum, Extern, Fun, Import, Interface, Object, Package, Typealias,
        Val, Var, Vararg, Break, Continue, Else, For, If, In, Is, Return, When, While, Unsafe,
        Internal, Private, Public, As, False, Null, Operator, Override, Super, This, True,
    ]
);

define_exhaustive_unit_enum_any!(
    any_reserved_word,
    ReservedWord,
    [
        Async, Await, Suspend, Actor, Spawn, Sealed, Dyn, Where, Yield, Macro, Reify,
    ]
);

define_exhaustive_unit_enum_any!(
    any_symbol,
    Symbol,
    [
        LeftParen,
        RightParen,
        LeftBracket,
        RightBracket,
        LeftBrace,
        RightBrace,
        Comma,
        Colon,
        Semicolon,
        At,
        Dot,
        QuestionDot,
        Question,
        QuestionColon,
        BangBang,
        Bang,
        ColonColon,
        Arrow,
        Star,
        Slash,
        Percent,
        Plus,
        Minus,
        DotDot,
        DotDotLess,
        Less,
        Greater,
        LessEqual,
        GreaterEqual,
        EqualEqual,
        BangEqual,
        Ampersand,
        AndAnd,
        OrOr,
        PlusEqual,
        MinusEqual,
        StarEqual,
        SlashEqual,
        PercentEqual,
        Equal,
        AsQuestion,
        BangIn,
        BangIs,
    ]
);

define_exhaustive_unit_enum_any!(
    any_trivia_kind,
    TriviaKind,
    [Whitespace, Newline, LineComment, BlockComment]
);

define_exhaustive_unit_enum_any!(
    any_invalid_kind,
    InvalidKind,
    [
        UnexpectedCharacter,
        UnterminatedBlockComment,
        InvalidStringEscape,
        InvalidCharLiteral,
        InvalidNumericLiteral,
    ]
);

define_exhaustive_unit_enum_any!(
    any_parameter_mode_context,
    ParameterModeContext,
    [NamedParameter, FunctionType]
);

macro_rules! define_exhaustive_enum_any {
    (
        $fn_name:ident,
        $enum_ty:ty,
        [$($pat:pat => $ctor:expr),+ $(,)?]
    ) => {
        fn $fn_name() -> $enum_ty {
            #[deny(unreachable_patterns)]
            const fn assert_exhaustive(value: $enum_ty) {
                match value {
                    $($pat => {},)+
                }
            }

            const COUNT: usize = 0usize $(+ {
                let _ = stringify!($pat);
                1usize
            })+;
            let tag: usize = kani::any();
            kani::assume(tag < COUNT);

            let mut cursor = 0usize;
            $(
                if tag == cursor {
                    let picked = $ctor;
                    assert_exhaustive(picked);
                    return picked;
                }
                cursor += 1;
            )+
            unreachable!()
        }
    };
}

define_exhaustive_enum_any!(
    any_token_kind,
    TokenKind,
    [
        TokenKind::Identifier => TokenKind::Identifier,
        TokenKind::IntegerLiteral(_) => TokenKind::IntegerLiteral(any_integer_literal_suffix()),
        TokenKind::FloatLiteral(_) => TokenKind::FloatLiteral(any_float_literal_suffix()),
        TokenKind::CharLiteral => TokenKind::CharLiteral,
        TokenKind::StringStart => TokenKind::StringStart,
        TokenKind::StringText => TokenKind::StringText,
        TokenKind::InterpolationStart => TokenKind::InterpolationStart,
        TokenKind::InterpolationEnd => TokenKind::InterpolationEnd,
        TokenKind::StringEnd => TokenKind::StringEnd,
        TokenKind::Keyword(_) => TokenKind::Keyword(any_keyword()),
        TokenKind::ReservedWord(_) => TokenKind::ReservedWord(any_reserved_word()),
        TokenKind::Symbol(_) => TokenKind::Symbol(any_symbol()),
    ]
);

define_exhaustive_enum_any!(
    any_lexeme_kind,
    LexemeKind,
    [
        LexemeKind::Token(_) => LexemeKind::Token(any_token_kind()),
        LexemeKind::Trivia(_) => LexemeKind::Trivia(any_trivia_kind()),
        LexemeKind::Invalid(_) => LexemeKind::Invalid(any_invalid_kind()),
        LexemeKind::Eof => LexemeKind::Eof,
    ]
);

#[kani::proof]
fn verify_const_val_recovery_and_function_form_dispatch() {
    let kind = any_lexeme_kind();
    let boundary: bool = kani::any();
    let expected_const = if boundary {
        ConstValRecoveryAction::MissingAtBoundary
    } else if matches!(kind, LexemeKind::Token(TokenKind::Keyword(Keyword::Var))) {
        ConstValRecoveryAction::ConsumeErrorVar
    } else if matches!(
        kind,
        LexemeKind::Token(
            TokenKind::Identifier
                | TokenKind::StringStart
                | TokenKind::Symbol(Symbol::Colon | Symbol::Equal)
        )
    ) {
        ConstValRecoveryAction::MissingKeepCurrent
    } else {
        ConstValRecoveryAction::ConsumeErrorOther
    };
    assert_eq!(const_val_recovery_action(kind, boundary), expected_const);

    let can_start_type: bool = kani::any();
    let expected_form = if matches!(kind, LexemeKind::Token(TokenKind::Symbol(Symbol::Colon))) {
        FunctionFormDispatch::ExplicitColon
    } else if matches!(kind, LexemeKind::Token(TokenKind::Symbol(Symbol::Equal))) {
        FunctionFormDispatch::MissingReturnTypeBeforeEqual
    } else if matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
    ) {
        FunctionFormDispatch::ImplicitUnitBlock
    } else if can_start_type {
        FunctionFormDispatch::MissingColonBeforeTypeRef
    } else {
        FunctionFormDispatch::ImplicitUnitAbsent
    };
    assert_eq!(function_form_dispatch(kind, can_start_type), expected_form);
}

#[kani::proof]
fn verify_parameter_mode_follow_and_value_param_guards() {
    let context = any_parameter_mode_context();
    let next1 = any_lexeme_kind();
    let expected_follow = match context {
        ParameterModeContext::NamedParameter => {
            matches!(next1, LexemeKind::Token(TokenKind::Identifier))
        }
        ParameterModeContext::FunctionType => matches!(
            next1,
            LexemeKind::Token(TokenKind::Identifier | TokenKind::Symbol(Symbol::LeftParen))
        ),
    };
    assert_eq!(
        is_parameter_mode_follow_kind(context, next1),
        expected_follow
    );

    let kind = any_lexeme_kind();
    let can_start_type: bool = kani::any();
    let expected_type_action = if matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(
            Symbol::Equal | Symbol::Comma | Symbol::RightParen
        )) | LexemeKind::Eof
    ) {
        ValueParamTypeAction::EmptyErrorAtBoundary
    } else if can_start_type {
        ValueParamTypeAction::ParseTypeRef
    } else {
        ValueParamTypeAction::RecoverErrorRegion
    };
    assert_eq!(
        value_param_type_action(kind, can_start_type),
        expected_type_action
    );

    let saw_equal_before: bool = kani::any();
    let colon_empty: bool = kani::any();
    let equal_after: bool = kani::any();
    assert_eq!(
        value_param_default_emits_diagnostic(saw_equal_before, colon_empty, equal_after),
        equal_after && !(saw_equal_before && colon_empty)
    );
}

#[kani::proof]
fn verify_declaration_tail_guard_exhaustive() {
    let current = any_lexeme_kind();
    let implicit_function: bool = kani::any();
    let is_trailing_poison: bool = kani::any();
    let expected =
        !matches!(current, LexemeKind::Eof) && (!implicit_function || !is_trailing_poison);
    assert_eq!(
        declaration_tail_emits_unexpected_trailing(current, implicit_function, is_trailing_poison),
        expected
    );
}
