use super::ClassifierHeaderOwner;
use super::supertypes::{
    DelegationLambdaTailEvent, DelegationLambdaTailStep, classify_delegation_lambda_token,
    delegation_suffix_extra_stop, is_direct_delegation_boundary_kind, run_delegation_lambda_tail,
    step_delegation_lambda_tail, supertype_deinit_suffix_decision,
};
use crate::lexer::{
    FloatLiteralSuffix, IntegerLiteralSuffix, InvalidKind, Keyword, LexemeKind, ReservedWord,
    Symbol, TokenKind, TriviaKind,
};
use crate::parser::engine::boundary::DeclarationStops;

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
    any_owner,
    ClassifierHeaderOwner,
    [File, Declaration, Member]
);

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
    any_tail_event,
    DelegationLambdaTailEvent,
    [
        Trivia,
        SameLineClosedCall,
        LineBreakLeftParen,
        UnclosedLeftParen,
        OuterHeaderDelimiter,
        Other,
    ]
);

/// Generates a symbolic sampler for an enum with payload-carrying variants and
/// a wildcard-free, duplicate-forbidding compile-time exhaustiveness witness
/// from the same single variant list.
macro_rules! define_exhaustive_enum_any {
    (
        $fn_name:ident,
        $enum_ty:ty,
        [$(($idx:literal, $pat:pat => $ctor:expr)),+ $(,)?],
        count = $count:literal
    ) => {
        fn $fn_name() -> $enum_ty {
            #[deny(unreachable_patterns)]
            const fn assert_exhaustive(value: $enum_ty) {
                match value {
                    $($pat => {},)+
                }
            }

            let tag: u8 = kani::any();
            kani::assume(tag < $count);
            let picked = match tag {
                $($idx => $ctor,)+
                _ => unreachable!(),
            };
            assert_exhaustive(picked);
            picked
        }
    };
}

define_exhaustive_enum_any!(
    any_token_kind,
    TokenKind,
    [
        (0, TokenKind::Identifier => TokenKind::Identifier),
        (1, TokenKind::IntegerLiteral(_) => TokenKind::IntegerLiteral(any_integer_literal_suffix())),
        (2, TokenKind::FloatLiteral(_) => TokenKind::FloatLiteral(any_float_literal_suffix())),
        (3, TokenKind::CharLiteral => TokenKind::CharLiteral),
        (4, TokenKind::StringStart => TokenKind::StringStart),
        (5, TokenKind::StringText => TokenKind::StringText),
        (6, TokenKind::InterpolationStart => TokenKind::InterpolationStart),
        (7, TokenKind::InterpolationEnd => TokenKind::InterpolationEnd),
        (8, TokenKind::StringEnd => TokenKind::StringEnd),
        (9, TokenKind::Keyword(_) => TokenKind::Keyword(any_keyword())),
        (10, TokenKind::ReservedWord(_) => TokenKind::ReservedWord(any_reserved_word())),
        (11, TokenKind::Symbol(_) => TokenKind::Symbol(any_symbol())),
    ],
    count = 12
);

define_exhaustive_enum_any!(
    any_lexeme_kind,
    LexemeKind,
    [
        (0, LexemeKind::Token(_) => LexemeKind::Token(any_token_kind())),
        (1, LexemeKind::Trivia(_) => LexemeKind::Trivia(any_trivia_kind())),
        (2, LexemeKind::Invalid(_) => LexemeKind::Invalid(any_invalid_kind())),
        (3, LexemeKind::Eof => LexemeKind::Eof),
    ],
    count = 4
);

#[kani::proof]
fn verify_supertype_direct_boundary_and_lambda_token_classification() {
    let kind = any_lexeme_kind();
    let has_line_break: bool = kani::any();
    let has_call_closer: bool = kani::any();

    let expected_direct = matches!(
        kind,
        LexemeKind::Eof
            | LexemeKind::Token(TokenKind::Symbol(
                Symbol::Comma | Symbol::LeftBrace | Symbol::RightBrace
            ))
    );
    assert_eq!(is_direct_delegation_boundary_kind(kind), expected_direct);

    let event = classify_delegation_lambda_token(kind, has_line_break, has_call_closer);
    match kind {
        LexemeKind::Trivia(_) => assert_eq!(event, DelegationLambdaTailEvent::Trivia),
        LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => {
            if has_line_break {
                assert_eq!(event, DelegationLambdaTailEvent::LineBreakLeftParen);
            } else if has_call_closer {
                assert_eq!(event, DelegationLambdaTailEvent::SameLineClosedCall);
            } else {
                assert_eq!(event, DelegationLambdaTailEvent::UnclosedLeftParen);
            }
        }
        LexemeKind::Token(TokenKind::Symbol(Symbol::Comma | Symbol::LeftBrace)) => {
            assert_eq!(event, DelegationLambdaTailEvent::OuterHeaderDelimiter);
        }
        _ => assert_eq!(event, DelegationLambdaTailEvent::Other),
    }
}

#[kani::proof]
fn verify_supertype_deinit_and_suffix_stop_decisions() {
    let owner = any_owner();
    let in_delegation_recovery: bool = kani::any();
    let in_unsupported_suffix: bool = kani::any();
    let next_line: bool = kani::any();
    let call_body_follows: bool = kani::any();

    let decision = supertype_deinit_suffix_decision(
        owner,
        in_delegation_recovery,
        in_unsupported_suffix,
        next_line,
        call_body_follows,
    );
    if owner != ClassifierHeaderOwner::Member || !in_delegation_recovery {
        assert_eq!(decision, None);
    } else if in_unsupported_suffix && !next_line {
        assert_eq!(decision, Some(false));
    } else if !in_unsupported_suffix && next_line {
        assert_eq!(decision, None);
    } else {
        assert_eq!(decision, Some(call_body_follows));
    }

    let invalid_target: bool = kani::any();
    let poison: bool = kani::any();
    let extra = delegation_suffix_extra_stop(invalid_target, poison);
    if invalid_target {
        assert_eq!(extra, DeclarationStops::DELEGATION_ERROR_TARGET);
    } else if poison {
        assert_eq!(extra, DeclarationStops::LEADING_UNPROVED_ANGLE_COMMA);
    } else {
        assert_eq!(extra, DeclarationStops::DELEGATION_UNSUPPORTED_SUFFIX);
    }

    let error_target = DeclarationStops::DELEGATION_ERROR_TARGET;
    let angle_comma = DeclarationStops::LEADING_UNPROVED_ANGLE_COMMA;
    let unsupported_suffix = DeclarationStops::DELEGATION_UNSUPPORTED_SUFFIX;
    assert_ne!(error_target, 0);
    assert_ne!(angle_comma, 0);
    assert_ne!(unsupported_suffix, 0);
    assert_eq!(error_target & angle_comma, 0);
    assert_eq!(error_target & unsupported_suffix, 0);
    assert_eq!(angle_comma & unsupported_suffix, 0);
    assert_eq!(
        DeclarationStops::EMPTY
            .with(extra)
            .in_unsupported_delegation_suffix(),
        !invalid_target && !poison
    );
}

#[kani::proof]
#[kani::unwind(8)]
fn verify_delegation_lambda_tail_run_len_6() {
    let events = [
        any_tail_event(),
        any_tail_event(),
        any_tail_event(),
        any_tail_event(),
        any_tail_event(),
        any_tail_event(),
    ];
    let len: usize = kani::any();
    kani::assume(len <= 6);

    let accepted = run_delegation_lambda_tail(&events[..len]);

    let mut expected = false;
    let mut i = 0usize;
    while i < len {
        match step_delegation_lambda_tail(events[i]) {
            DelegationLambdaTailStep::Continue => {
                assert!(matches!(
                    events[i],
                    DelegationLambdaTailEvent::Trivia
                        | DelegationLambdaTailEvent::SameLineClosedCall
                ));
            }
            DelegationLambdaTailStep::Accept => {
                assert_eq!(events[i], DelegationLambdaTailEvent::OuterHeaderDelimiter);
                expected = true;
                break;
            }
            DelegationLambdaTailStep::Reject => {
                assert!(matches!(
                    events[i],
                    DelegationLambdaTailEvent::LineBreakLeftParen
                        | DelegationLambdaTailEvent::UnclosedLeftParen
                        | DelegationLambdaTailEvent::Other
                ));
                break;
            }
        }
        i += 1;
    }
    assert_eq!(accepted, expected);
}
