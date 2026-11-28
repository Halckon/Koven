use super::PrefixOperator;
use super::boundary::file_construct_start_kind;
use super::expression_kernel::{
    GroupNonPoisonTailAction, WhenConditionDispatch, group_non_poison_tail_action,
    is_group_declaration_tail_kind, is_when_body_line_break_infix_stop_kind,
    prefix_operator_for_kind, return_parses_value, string_missing_closer_emits_diagnostic,
    when_condition_dispatch,
};
use super::operator::can_start_expression_kind;
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
fn verify_prefix_operator_and_when_condition_dispatch_exhaustive() {
    let kind = any_lexeme_kind();
    let can_start: bool = kani::any();

    let expected_prefix = match kind {
        LexemeKind::Token(TokenKind::Symbol(Symbol::Bang)) => Some(PrefixOperator::Not),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)) => Some(PrefixOperator::Plus),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)) => Some(PrefixOperator::Minus),
        _ => None,
    };
    let actual_prefix = prefix_operator_for_kind(kind);
    assert_eq!(actual_prefix, expected_prefix);
    if actual_prefix.is_some() {
        assert!(can_start_expression_kind(kind));
    }

    let expected_when_stop = matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(Keyword::Is | Keyword::In))
            | LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs | Symbol::BangIn))
    );
    let actual_when_stop = is_when_body_line_break_infix_stop_kind(kind);
    assert_eq!(actual_when_stop, expected_when_stop);
    if actual_prefix.is_some() {
        assert!(!actual_when_stop);
    }

    let expected_dispatch = match kind {
        LexemeKind::Token(TokenKind::Keyword(Keyword::Is)) => {
            WhenConditionDispatch::TypeTest { negated: false }
        }
        LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs)) => {
            WhenConditionDispatch::TypeTest { negated: true }
        }
        LexemeKind::Token(TokenKind::Keyword(Keyword::In)) => {
            WhenConditionDispatch::Contains { negated: false }
        }
        LexemeKind::Token(TokenKind::Symbol(Symbol::BangIn)) => {
            WhenConditionDispatch::Contains { negated: true }
        }
        _ if can_start => WhenConditionDispatch::Expression,
        _ => WhenConditionDispatch::ExpectedWhenEntry,
    };
    let actual_dispatch = when_condition_dispatch(kind, can_start);
    assert_eq!(actual_dispatch, expected_dispatch);
    assert_eq!(
        matches!(
            actual_dispatch,
            WhenConditionDispatch::TypeTest { .. } | WhenConditionDispatch::Contains { .. }
        ),
        actual_when_stop
    );
}

#[kani::proof]
fn verify_return_value_and_string_missing_closer_guards() {
    let can_start: bool = kani::any();
    let has_line_break: bool = kani::any();
    let in_stops: bool = kani::any();
    let is_control_start: bool = kani::any();
    assert_eq!(
        return_parses_value(can_start, has_line_break, in_stops, is_control_start),
        can_start && !has_line_break && (!in_stops || is_control_start)
    );

    let lexer_owns_owner_recovery: bool = kani::any();
    let at_source_end: bool = kani::any();
    let terminal_error_at_eof: bool = kani::any();
    assert_eq!(
        string_missing_closer_emits_diagnostic(
            lexer_owns_owner_recovery,
            at_source_end,
            terminal_error_at_eof
        ),
        !lexer_owns_owner_recovery && !(at_source_end && terminal_error_at_eof)
    );
}

#[kani::proof]
fn verify_group_non_poison_tail_action_exhaustive() {
    let kind = any_lexeme_kind();
    let file_mode: bool = kani::any();
    let expected_decl_tail = matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
    ) || (file_mode && file_construct_start_kind(kind));
    assert_eq!(
        is_group_declaration_tail_kind(kind, file_mode),
        expected_decl_tail
    );

    let at_right_paren: bool = kani::any();
    let declaration_tail: bool = kani::any();
    let at_comma: bool = kani::any();
    let in_outer_stops: bool = kani::any();
    let expected_action = if at_right_paren {
        GroupNonPoisonTailAction::AtRightParen
    } else if declaration_tail {
        GroupNonPoisonTailAction::RecoverDeclarationTail
    } else if at_comma || !in_outer_stops {
        GroupNonPoisonTailAction::TryConfirmedCloser
    } else {
        GroupNonPoisonTailAction::NoSkip
    };
    assert_eq!(
        group_non_poison_tail_action(at_right_paren, declaration_tail, at_comma, in_outer_stops),
        expected_action
    );
}
