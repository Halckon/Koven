use super::postfix::{
    call_argument_boundary_decision, has_recovery_callee_end_in, is_postfix_start_kind,
    is_recovery_callee_ignored_kind, is_recovery_callee_terminal_kind,
    trailing_lambda_attaches_after_call, typed_call_suffix_continues_kind,
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

/// Derives the symbolic sampling bound, branches, and compile-time wildcard-free
/// duplicate-forbidding exhaustiveness witness from one single variant list.
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
fn verify_postfix_start_typed_call_and_trailing_lambda() {
    let kind = any_lexeme_kind();
    let expected_postfix_start = matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(
            Symbol::Dot
                | Symbol::QuestionDot
                | Symbol::LeftParen
                | Symbol::LeftBracket
                | Symbol::BangBang
                | Symbol::Question
                | Symbol::ColonColon
        ))
    );
    assert_eq!(is_postfix_start_kind(kind), expected_postfix_start);

    let has_line_break: bool = kani::any();
    let expected_continues = !matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
    ) || !has_line_break;
    assert_eq!(
        typed_call_suffix_continues_kind(kind, has_line_break),
        expected_continues
    );

    let closed: bool = kani::any();
    let next_when_entry: bool = kani::any();
    let expected_attaches = closed
        && matches!(
            kind,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
        )
        && !next_when_entry;
    assert_eq!(
        trailing_lambda_attaches_after_call(closed, kind, next_when_entry),
        expected_attaches
    );
}

#[kani::proof]
fn verify_call_argument_boundary_and_callee_disjointness() {
    let kind = any_lexeme_kind();
    let outer_contains: bool = kani::any();
    let can_start_expr: bool = kani::any();
    let at_block_separator: bool = kani::any();

    let expected_boundary = matches!(
        kind,
        LexemeKind::Eof | LexemeKind::Token(TokenKind::Symbol(Symbol::Comma | Symbol::RightParen))
    ) || (outer_contains && !can_start_expr)
        || at_block_separator;
    assert_eq!(
        call_argument_boundary_decision(kind, outer_contains, can_start_expr, at_block_separator),
        expected_boundary
    );

    let ignored = is_recovery_callee_ignored_kind(kind);
    let terminal = is_recovery_callee_terminal_kind(kind);
    assert_eq!(ignored, expected_recovery_callee_ignored(kind));
    assert_eq!(terminal, expected_recovery_callee_terminal(kind));
    assert!(!(ignored && terminal));
}

const fn expected_recovery_callee_ignored(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Trivia(_)
            | LexemeKind::Token(TokenKind::Symbol(Symbol::Question | Symbol::BangBang))
    )
}

const fn expected_recovery_callee_terminal(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(
            TokenKind::Identifier
                | TokenKind::IntegerLiteral(_)
                | TokenKind::FloatLiteral(_)
                | TokenKind::CharLiteral
                | TokenKind::StringEnd
                | TokenKind::Keyword(
                    Keyword::True | Keyword::False | Keyword::Null | Keyword::This
                )
                | TokenKind::Symbol(Symbol::RightParen | Symbol::RightBracket | Symbol::RightBrace)
        )
    )
}

#[kani::proof]
#[kani::unwind(8)]
fn verify_recovery_callee_end_reverse_scan_len_6() {
    let kinds = [
        any_lexeme_kind(),
        any_lexeme_kind(),
        any_lexeme_kind(),
        any_lexeme_kind(),
        any_lexeme_kind(),
        any_lexeme_kind(),
    ];
    let len: usize = kani::any();
    kani::assume(len <= 6);

    let actual = has_recovery_callee_end_in(&kinds[..len], |kind| kind);

    let mut expected = false;
    let mut i = len;
    while i > 0 {
        i -= 1;
        if !expected_recovery_callee_ignored(kinds[i]) {
            expected = expected_recovery_callee_terminal(kinds[i]);
            break;
        }
    }
    assert_eq!(actual, expected);
}
