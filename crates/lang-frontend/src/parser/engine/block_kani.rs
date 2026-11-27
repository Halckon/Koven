use super::block::{
    block_element_separator_required, block_expression_line_break_boundary,
    is_explicit_block_start, run_missing_block_root_end, step_missing_block_root_end,
    unsupported_block_element_kind,
};
use crate::lexer::{
    FloatLiteralSuffix, IntegerLiteralSuffix, InvalidKind, Keyword, LexemeKind, ReservedWord,
    Symbol, TokenKind, TriviaKind,
};

/// Generates both the symbolic sampler array and a wildcard-free,
/// duplicate-forbidding compile-time exhaustiveness witness from one single
/// variant list so any omitted or duplicated variant fails compilation.
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

#[kani::proof]
fn verify_block_unsupported_line_break_and_explicit_start() {
    let kind = any_lexeme_kind();
    let expected_unsupported = matches!(
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
    );
    assert_eq!(unsupported_block_element_kind(kind), expected_unsupported);

    let block_elements: bool = kani::any();
    let has_line_break: bool = kani::any();
    let expected_line_break = block_elements
        && has_line_break
        && matches!(
            kind,
            LexemeKind::Token(TokenKind::Symbol(
                Symbol::LeftParen | Symbol::Plus | Symbol::Minus
            ))
        );
    assert_eq!(
        block_expression_line_break_boundary(block_elements, kind, has_line_break),
        expected_line_break
    );

    let is_block_ident: bool = kani::any();
    let has_next: bool = kani::any();
    let next_kind = if has_next {
        Some(any_lexeme_kind())
    } else {
        None
    };
    let expected_explicit = matches!(kind, LexemeKind::Token(TokenKind::Identifier))
        && is_block_ident
        && matches!(
            next_kind,
            Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)))
        )
        && !has_line_break;
    assert_eq!(
        is_explicit_block_start(kind, is_block_ident, next_kind, has_line_break),
        expected_explicit
    );
}

#[kani::proof]
fn verify_block_element_separator_required_matrix() {
    let kind = any_lexeme_kind();
    let outer_hard_stop: bool = kani::any();
    let is_poison: bool = kani::any();
    let is_unsupported: bool = kani::any();
    let can_start_expr: bool = kani::any();
    let prev_is_error: bool = kani::any();
    let has_line_break: bool = kani::any();

    let actual = block_element_separator_required(
        kind,
        outer_hard_stop,
        is_poison,
        is_unsupported,
        can_start_expr,
        prev_is_error,
        has_line_break,
    );

    let starts_loop = matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(Keyword::While | Keyword::For))
    );
    let is_delim_or_eof = matches!(
        kind,
        LexemeKind::Eof
            | LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon | Symbol::RightBrace))
    );
    let starts_next = can_start_expr
        || matches!(
            kind,
            LexemeKind::Token(TokenKind::Keyword(
                Keyword::Val | Keyword::Var | Keyword::While | Keyword::For
            ))
        );
    let expected = !is_delim_or_eof
        && !outer_hard_stop
        && !is_poison
        && !(is_unsupported && !starts_loop)
        && starts_next
        && !prev_is_error
        && !has_line_break;
    assert_eq!(actual, expected);

    if !outer_hard_stop && !is_poison && is_unsupported && !prev_is_error && !has_line_break {
        assert_eq!(actual, starts_loop);
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn verify_missing_block_root_end_run_len_6() {
    let initial_end: usize = kani::any();
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

    let actual = run_missing_block_root_end(initial_end, &lexemes[..len]);

    let mut expected = initial_end;
    let mut i = 0usize;
    while i < len {
        let (kind, end) = lexemes[i];
        let (next_end, stop) = step_missing_block_root_end(expected, kind, end);
        if matches!(kind, LexemeKind::Eof) {
            assert!(stop);
            assert_eq!(next_end, expected);
            break;
        }
        assert!(!stop);
        if matches!(kind, LexemeKind::Trivia(_)) {
            assert_eq!(next_end, expected);
        } else {
            assert_eq!(next_end, end);
        }
        expected = next_end;
        i += 1;
    }
    assert_eq!(actual, expected);
}
