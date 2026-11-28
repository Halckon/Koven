use super::lambda::{
    LambdaBodyLoopAction, is_lambda_body_unsupported_symbol_kind, lambda_body_loop_action,
    lambda_body_missing_closer_emits_diagnostic, skip_lambda_body_semicolons_in,
    unsupported_lambda_body_consumes_val_kind,
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

const fn expected_is_skippable_semicolon(kind: LexemeKind, outer_hard_stop: bool) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
    ) && !outer_hard_stop
}

#[kani::proof]
fn verify_lambda_body_loop_action_and_unsupported_symbol() {
    let kind = any_lexeme_kind();
    let outer_hard_stop: bool = kani::any();

    let expected_action = if matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace))
    ) || outer_hard_stop
    {
        LambdaBodyLoopAction::Stop
    } else if matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
    ) {
        LambdaBodyLoopAction::SkipSemicolon
    } else {
        LambdaBodyLoopAction::ParseElement
    };
    assert_eq!(
        lambda_body_loop_action(kind, outer_hard_stop),
        expected_action
    );

    let expected_unsupported = matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::Comma | Symbol::Arrow))
    );
    assert_eq!(
        is_lambda_body_unsupported_symbol_kind(kind),
        expected_unsupported
    );
}

#[kani::proof]
fn verify_lambda_body_missing_closer_and_const_val_guards() {
    let current = any_lexeme_kind();
    let terminal_error_at_eof: bool = kani::any();
    let is_poison: bool = kani::any();
    let expected_missing = !matches!(
        current,
        LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace))
    ) && !terminal_error_at_eof
        && !is_poison;
    assert_eq!(
        lambda_body_missing_closer_emits_diagnostic(current, terminal_error_at_eof, is_poison),
        expected_missing
    );

    let first = any_lexeme_kind();
    let second = any_opt_lexeme_kind();
    let expected_consumes = matches!(first, LexemeKind::Token(TokenKind::Keyword(Keyword::Const)))
        && matches!(
            second,
            Some(LexemeKind::Token(TokenKind::Keyword(Keyword::Val)))
        );
    assert_eq!(
        unsupported_lambda_body_consumes_val_kind(first, second),
        expected_consumes
    );
}

#[kani::proof]
#[kani::unwind(8)]
fn verify_lambda_body_semicolon_skip_len_6() {
    let entries = [
        (any_lexeme_kind(), kani::any::<bool>()),
        (any_lexeme_kind(), kani::any::<bool>()),
        (any_lexeme_kind(), kani::any::<bool>()),
        (any_lexeme_kind(), kani::any::<bool>()),
        (any_lexeme_kind(), kani::any::<bool>()),
        (any_lexeme_kind(), kani::any::<bool>()),
    ];
    let len: usize = kani::any();
    kani::assume(len <= 6);

    let stopped = skip_lambda_body_semicolons_in(&entries[..len], |e| e.0, |e| e.1);
    assert!(stopped <= len);

    let mut i = 0usize;
    while i < stopped {
        assert!(expected_is_skippable_semicolon(entries[i].0, entries[i].1));
        i += 1;
    }
    if stopped < len {
        assert!(!expected_is_skippable_semicolon(
            entries[stopped].0,
            entries[stopped].1
        ));
    }
}
