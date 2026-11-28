use super::borrow_result::{
    ExplicitResultAction, borrow_local_contract_is_invalid, explicit_result_action,
    is_borrow_local_binding_keyword_kind, is_borrow_local_start_tail,
};
use super::declaration_head::{is_extension_receiver_follow_kind, is_extension_receiver_head_kind};
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

fn any_opt_lexeme_kind() -> Option<LexemeKind> {
    if kani::any() {
        Some(any_lexeme_kind())
    } else {
        None
    }
}

#[kani::proof]
fn verify_explicit_result_action_and_borrow_local_contract() {
    let has_borrow: bool = kani::any();
    let has_from: bool = kani::any();
    let action = explicit_result_action(has_borrow, has_from);
    let expected_action = if !has_borrow && !has_from {
        ExplicitResultAction::PlainTypeOnly
    } else if has_borrow && !has_from {
        ExplicitResultAction::MissingBorrowFrom
    } else if has_borrow && has_from {
        ExplicitResultAction::ParseBorrowSource
    } else {
        ExplicitResultAction::ParseCarrierSource
    };
    assert_eq!(action, expected_action);
    assert_eq!(
        action == ExplicitResultAction::MissingBorrowFrom,
        has_borrow && !has_from
    );

    let is_borrow: bool = kani::any();
    let immutable: bool = kani::any();
    let conditional: bool = kani::any();
    let invalid = borrow_local_contract_is_invalid(is_borrow, immutable, conditional);
    assert_eq!(!invalid, is_borrow && immutable && !conditional);
}

#[kani::proof]
fn verify_borrow_local_start_tail_exhaustive() {
    let next1 = any_lexeme_kind();
    let next2 = any_opt_lexeme_kind();

    let expected_next1_binding = matches!(
        next1,
        LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
    );
    assert_eq!(
        is_borrow_local_binding_keyword_kind(next1),
        expected_next1_binding
    );

    let expected_next2_binding = matches!(
        next2,
        Some(LexemeKind::Token(TokenKind::Keyword(
            Keyword::Val | Keyword::Var
        )))
    );
    let expected_tail = expected_next1_binding
        || (matches!(
            next1,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Question))
        ) && expected_next2_binding);
    assert_eq!(is_borrow_local_start_tail(next1, next2), expected_tail);
}

#[kani::proof]
fn verify_extension_receiver_head_exhaustive() {
    let current_is_identifier: bool = kani::any();
    let next1 = any_lexeme_kind();

    let expected_follow = matches!(
        next1,
        LexemeKind::Token(TokenKind::Symbol(
            Symbol::Dot | Symbol::Less | Symbol::Question
        ))
    );
    assert_eq!(is_extension_receiver_follow_kind(next1), expected_follow);
    assert_eq!(
        is_extension_receiver_head_kind(current_is_identifier, next1),
        current_is_identifier && expected_follow
    );

    if matches!(
        next1,
        LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
    ) {
        assert!(!is_extension_receiver_follow_kind(next1));
    }
}
