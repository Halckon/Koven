use super::type_ref::{
    bounded_proven_parameter_closer, is_extension_receiver_dot_stop,
    is_function_type_right_paren_closer, is_function_type_start_kinds,
    run_segmented_string_type_poison_end, step_segmented_string_type_poison_end,
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

fn any_opt_usize() -> Option<usize> {
    if kani::any() { Some(kani::any()) } else { None }
}

#[kani::proof]
fn verify_type_ref_start_extension_and_closer_guards() {
    let current = any_lexeme_kind();
    let is_move_ident: bool = kani::any();
    let next = any_opt_lexeme_kind();
    let next2 = any_opt_lexeme_kind();

    let expected_fn_start = (matches!(current, LexemeKind::Token(TokenKind::Identifier))
        && is_move_ident
        && matches!(
            next,
            Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)))
        ))
        || matches!(
            current,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen))
        );
    assert_eq!(
        is_function_type_start_kinds(current, is_move_ident, next),
        expected_fn_start
    );

    let ends_at_ext: bool = kani::any();
    let expected_ext_stop = ends_at_ext
        && matches!(next, Some(LexemeKind::Token(TokenKind::Identifier)))
        && matches!(
            next2,
            Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)))
        );
    assert_eq!(
        is_extension_receiver_dot_stop(ends_at_ext, next, next2),
        expected_ext_stop
    );

    let current_raw: usize = kani::any();
    let proven_closer = any_opt_usize();
    let expected_rparen = matches!(
        current,
        LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen))
    ) && proven_closer.is_none_or(|limit| current_raw != limit);
    assert_eq!(
        is_function_type_right_paren_closer(current, current_raw, proven_closer),
        expected_rparen
    );
}

#[kani::proof]
fn verify_bounded_proven_parameter_closer_invariants() {
    let closer = any_opt_usize();
    let proven_limit = any_opt_usize();
    let result = bounded_proven_parameter_closer(closer, proven_limit);
    match (closer, proven_limit) {
        (Some(c), Some(limit)) if c < limit => assert_eq!(result, Some(c)),
        (Some(c), None) => assert_eq!(result, Some(c)),
        _ => assert_eq!(result, None),
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn verify_string_type_poison_scan_len_6() {
    let initial_end: usize = kani::any();
    let recovery_end: usize = kani::any();
    let lexemes = [
        (any_lexeme_kind(), kani::any::<usize>()),
        (any_lexeme_kind(), kani::any::<usize>()),
        (any_lexeme_kind(), kani::any::<usize>()),
        (any_lexeme_kind(), kani::any::<usize>()),
        (any_lexeme_kind(), kani::any::<usize>()),
        (any_lexeme_kind(), kani::any::<usize>()),
    ];
    let len: usize = kani::any();
    kani::assume(len <= 6);

    let actual = run_segmented_string_type_poison_end(initial_end, recovery_end, &lexemes[..len]);

    let mut expected = initial_end;
    let mut i = 0usize;
    while i < len {
        let (kind, end) = lexemes[i];
        let (next_end, stop) =
            step_segmented_string_type_poison_end(expected, recovery_end, kind, end);
        if expected >= recovery_end || matches!(kind, LexemeKind::Eof) {
            assert!(stop);
            assert_eq!(next_end, expected);
            break;
        }
        assert!(!stop);
        assert_eq!(next_end, end);
        expected = next_end;
        i += 1;
    }
    assert_eq!(actual, expected);
}
