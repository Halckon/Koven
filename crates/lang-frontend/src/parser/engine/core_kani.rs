use super::core::{
    boundary_uses_empty_span, control_body_element_is_statement,
    expression_tail_emits_unexpected_trailing, expression_tail_is_block_boundary,
    if_missing_else_emits_diagnostic, lambda_body_element_is_statement,
    previous_significant_end_with, try_enter_recursion_depth, type_recovery_boundary_hit,
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

#[kani::proof]
fn verify_recursion_depth_and_body_context_propagation() {
    let depth: usize = kani::any();
    let max_depth: usize = kani::any();
    kani::assume(max_depth <= 1024);

    let entered = try_enter_recursion_depth(depth, max_depth);
    if depth >= max_depth {
        assert_eq!(entered, None);
    } else {
        assert_eq!(entered, Some(depth + 1));
    }

    let len: usize = kani::any();
    let index: usize = kani::any();
    kani::assume(len <= 64 && index <= 64);
    let is_expr: bool = kani::any();
    let value_required: bool = kani::any();

    let expected_lambda_stmt = !(index == len.saturating_sub(1) && is_expr);
    let actual_lambda_stmt = lambda_body_element_is_statement(index, len, is_expr);
    assert_eq!(actual_lambda_stmt, expected_lambda_stmt);

    let expected_control_stmt = !(value_required && index == len.saturating_sub(1) && is_expr);
    let actual_control_stmt =
        control_body_element_is_statement(value_required, index, len, is_expr);
    assert_eq!(actual_control_stmt, expected_control_stmt);
    if value_required {
        assert_eq!(actual_control_stmt, actual_lambda_stmt);
    } else {
        assert!(actual_control_stmt);
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn verify_previous_significant_end_len_6() {
    let items: [(LexemeKind, usize); 6] = [
        (any_lexeme_kind(), kani::any()),
        (any_lexeme_kind(), kani::any()),
        (any_lexeme_kind(), kani::any()),
        (any_lexeme_kind(), kani::any()),
        (any_lexeme_kind(), kani::any()),
        (any_lexeme_kind(), kani::any()),
    ];
    let len: usize = kani::any();
    kani::assume(len <= 6);
    let slice = &items[..len];

    let actual = previous_significant_end_with(slice, |item| item.0, |item| item.1);

    // Independent forward reference computation (does not use `.rev()` or `matches!(Trivia)`).
    let mut expected = 0usize;
    let mut i = 0usize;
    while i < len {
        let is_non_trivia = match slice[i].0 {
            LexemeKind::Token(_) | LexemeKind::Invalid(_) | LexemeKind::Eof => true,
            LexemeKind::Trivia(_) => false,
        };
        if is_non_trivia {
            expected = slice[i].1;
        }
        i += 1;
    }
    assert_eq!(actual, expected);
}

#[kani::proof]
fn verify_if_else_tail_and_recovery_boundary_guards_exhaustive() {
    let has_else: bool = kani::any();
    let statement_allowed: bool = kani::any();
    assert_eq!(
        if_missing_else_emits_diagnostic(has_else, statement_allowed),
        !has_else && !statement_allowed
    );

    let in_stops: bool = kani::any();
    let block_elements: bool = kani::any();
    let is_semicolon: bool = kani::any();
    let can_start_expr: bool = kani::any();
    let block_boundary =
        expression_tail_is_block_boundary(block_elements, is_semicolon, can_start_expr);
    assert_eq!(
        block_boundary,
        block_elements && (is_semicolon || can_start_expr)
    );
    assert_eq!(
        expression_tail_emits_unexpected_trailing(in_stops, block_boundary),
        !in_stops && !block_boundary
    );

    let has_block_sep: bool = kani::any();
    let at_block_sep: bool = kani::any();
    let has_class_file: bool = kani::any();
    let at_class_file: bool = kani::any();
    let at_file_angle: bool = kani::any();
    assert_eq!(
        boundary_uses_empty_span(in_stops, at_block_sep),
        in_stops || at_block_sep
    );
    assert_eq!(
        type_recovery_boundary_hit(
            in_stops,
            has_block_sep,
            at_block_sep,
            has_class_file,
            at_class_file,
            at_file_angle
        ),
        in_stops
            || (has_block_sep && at_block_sep)
            || (has_class_file && at_class_file)
            || at_file_angle
    );
}
