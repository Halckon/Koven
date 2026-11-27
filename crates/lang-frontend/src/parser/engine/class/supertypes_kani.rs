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

fn any_owner() -> ClassifierHeaderOwner {
    let tag: u8 = kani::any();
    kani::assume(tag < 3);
    match tag {
        0 => ClassifierHeaderOwner::File,
        1 => ClassifierHeaderOwner::Declaration,
        _ => ClassifierHeaderOwner::Member,
    }
}

fn any_integer_literal_suffix() -> IntegerLiteralSuffix {
    const ALL: [IntegerLiteralSuffix; 4] = [
        IntegerLiteralSuffix::None,
        IntegerLiteralSuffix::Long,
        IntegerLiteralSuffix::Unsigned,
        IntegerLiteralSuffix::UnsignedLong,
    ];
    let idx: usize = kani::any();
    kani::assume(idx < ALL.len());
    ALL[idx]
}

fn any_float_literal_suffix() -> FloatLiteralSuffix {
    const ALL: [FloatLiteralSuffix; 2] = [FloatLiteralSuffix::None, FloatLiteralSuffix::Float];
    let idx: usize = kani::any();
    kani::assume(idx < ALL.len());
    ALL[idx]
}

fn any_keyword() -> Keyword {
    const ALL: [Keyword; 36] = [
        Keyword::Class,
        Keyword::Companion,
        Keyword::Const,
        Keyword::Enum,
        Keyword::Extern,
        Keyword::Fun,
        Keyword::Import,
        Keyword::Interface,
        Keyword::Object,
        Keyword::Package,
        Keyword::Typealias,
        Keyword::Val,
        Keyword::Var,
        Keyword::Vararg,
        Keyword::Break,
        Keyword::Continue,
        Keyword::Else,
        Keyword::For,
        Keyword::If,
        Keyword::In,
        Keyword::Is,
        Keyword::Return,
        Keyword::When,
        Keyword::While,
        Keyword::Unsafe,
        Keyword::Internal,
        Keyword::Private,
        Keyword::Public,
        Keyword::As,
        Keyword::False,
        Keyword::Null,
        Keyword::Operator,
        Keyword::Override,
        Keyword::Super,
        Keyword::This,
        Keyword::True,
    ];
    let idx: usize = kani::any();
    kani::assume(idx < ALL.len());
    ALL[idx]
}

fn any_reserved_word() -> ReservedWord {
    const ALL: [ReservedWord; 11] = [
        ReservedWord::Async,
        ReservedWord::Await,
        ReservedWord::Suspend,
        ReservedWord::Actor,
        ReservedWord::Spawn,
        ReservedWord::Sealed,
        ReservedWord::Dyn,
        ReservedWord::Where,
        ReservedWord::Yield,
        ReservedWord::Macro,
        ReservedWord::Reify,
    ];
    let idx: usize = kani::any();
    kani::assume(idx < ALL.len());
    ALL[idx]
}

fn any_symbol() -> Symbol {
    const ALL: [Symbol; 43] = [
        Symbol::LeftParen,
        Symbol::RightParen,
        Symbol::LeftBracket,
        Symbol::RightBracket,
        Symbol::LeftBrace,
        Symbol::RightBrace,
        Symbol::Comma,
        Symbol::Colon,
        Symbol::Semicolon,
        Symbol::At,
        Symbol::Dot,
        Symbol::QuestionDot,
        Symbol::Question,
        Symbol::QuestionColon,
        Symbol::BangBang,
        Symbol::Bang,
        Symbol::ColonColon,
        Symbol::Arrow,
        Symbol::Star,
        Symbol::Slash,
        Symbol::Percent,
        Symbol::Plus,
        Symbol::Minus,
        Symbol::DotDot,
        Symbol::DotDotLess,
        Symbol::Less,
        Symbol::Greater,
        Symbol::LessEqual,
        Symbol::GreaterEqual,
        Symbol::EqualEqual,
        Symbol::BangEqual,
        Symbol::Ampersand,
        Symbol::AndAnd,
        Symbol::OrOr,
        Symbol::PlusEqual,
        Symbol::MinusEqual,
        Symbol::StarEqual,
        Symbol::SlashEqual,
        Symbol::PercentEqual,
        Symbol::Equal,
        Symbol::AsQuestion,
        Symbol::BangIn,
        Symbol::BangIs,
    ];
    let idx: usize = kani::any();
    kani::assume(idx < ALL.len());
    ALL[idx]
}

fn any_trivia_kind() -> TriviaKind {
    const ALL: [TriviaKind; 4] = [
        TriviaKind::Whitespace,
        TriviaKind::Newline,
        TriviaKind::LineComment,
        TriviaKind::BlockComment,
    ];
    let idx: usize = kani::any();
    kani::assume(idx < ALL.len());
    ALL[idx]
}

fn any_invalid_kind() -> InvalidKind {
    const ALL: [InvalidKind; 5] = [
        InvalidKind::UnexpectedCharacter,
        InvalidKind::UnterminatedBlockComment,
        InvalidKind::InvalidStringEscape,
        InvalidKind::InvalidCharLiteral,
        InvalidKind::InvalidNumericLiteral,
    ];
    let idx: usize = kani::any();
    kani::assume(idx < ALL.len());
    ALL[idx]
}

fn any_token_kind() -> TokenKind {
    let tag: u8 = kani::any();
    kani::assume(tag < 12);
    match tag {
        0 => TokenKind::Identifier,
        1 => TokenKind::IntegerLiteral(any_integer_literal_suffix()),
        2 => TokenKind::FloatLiteral(any_float_literal_suffix()),
        3 => TokenKind::CharLiteral,
        4 => TokenKind::StringStart,
        5 => TokenKind::StringText,
        6 => TokenKind::InterpolationStart,
        7 => TokenKind::InterpolationEnd,
        8 => TokenKind::StringEnd,
        9 => TokenKind::Keyword(any_keyword()),
        10 => TokenKind::ReservedWord(any_reserved_word()),
        _ => TokenKind::Symbol(any_symbol()),
    }
}

/// Compile-time exhaustiveness witness ensuring `any_lexeme_kind()` stays in sync
/// with every production `LexemeKind`, `TokenKind`, `TriviaKind`, `InvalidKind`,
/// `Keyword`, `ReservedWord`, `Symbol`, and literal suffix.
const fn assert_lexeme_kind_exhaustive(kind: LexemeKind) {
    match kind {
        LexemeKind::Eof => {}
        LexemeKind::Trivia(
            TriviaKind::Whitespace
            | TriviaKind::Newline
            | TriviaKind::LineComment
            | TriviaKind::BlockComment,
        ) => {}
        LexemeKind::Invalid(
            InvalidKind::UnexpectedCharacter
            | InvalidKind::UnterminatedBlockComment
            | InvalidKind::InvalidStringEscape
            | InvalidKind::InvalidCharLiteral
            | InvalidKind::InvalidNumericLiteral,
        ) => {}
        LexemeKind::Token(token) => match token {
            TokenKind::Identifier
            | TokenKind::CharLiteral
            | TokenKind::StringStart
            | TokenKind::StringText
            | TokenKind::InterpolationStart
            | TokenKind::InterpolationEnd
            | TokenKind::StringEnd => {}
            TokenKind::IntegerLiteral(
                IntegerLiteralSuffix::None
                | IntegerLiteralSuffix::Long
                | IntegerLiteralSuffix::Unsigned
                | IntegerLiteralSuffix::UnsignedLong,
            ) => {}
            TokenKind::FloatLiteral(FloatLiteralSuffix::None | FloatLiteralSuffix::Float) => {}
            TokenKind::Keyword(
                Keyword::Class
                | Keyword::Companion
                | Keyword::Const
                | Keyword::Enum
                | Keyword::Extern
                | Keyword::Fun
                | Keyword::Import
                | Keyword::Interface
                | Keyword::Object
                | Keyword::Package
                | Keyword::Typealias
                | Keyword::Val
                | Keyword::Var
                | Keyword::Vararg
                | Keyword::Break
                | Keyword::Continue
                | Keyword::Else
                | Keyword::For
                | Keyword::If
                | Keyword::In
                | Keyword::Is
                | Keyword::Return
                | Keyword::When
                | Keyword::While
                | Keyword::Unsafe
                | Keyword::Internal
                | Keyword::Private
                | Keyword::Public
                | Keyword::As
                | Keyword::False
                | Keyword::Null
                | Keyword::Operator
                | Keyword::Override
                | Keyword::Super
                | Keyword::This
                | Keyword::True,
            ) => {}
            TokenKind::ReservedWord(
                ReservedWord::Async
                | ReservedWord::Await
                | ReservedWord::Suspend
                | ReservedWord::Actor
                | ReservedWord::Spawn
                | ReservedWord::Sealed
                | ReservedWord::Dyn
                | ReservedWord::Where
                | ReservedWord::Yield
                | ReservedWord::Macro
                | ReservedWord::Reify,
            ) => {}
            TokenKind::Symbol(
                Symbol::LeftParen
                | Symbol::RightParen
                | Symbol::LeftBracket
                | Symbol::RightBracket
                | Symbol::LeftBrace
                | Symbol::RightBrace
                | Symbol::Comma
                | Symbol::Colon
                | Symbol::Semicolon
                | Symbol::At
                | Symbol::Dot
                | Symbol::QuestionDot
                | Symbol::Question
                | Symbol::QuestionColon
                | Symbol::BangBang
                | Symbol::Bang
                | Symbol::ColonColon
                | Symbol::Arrow
                | Symbol::Star
                | Symbol::Slash
                | Symbol::Percent
                | Symbol::Plus
                | Symbol::Minus
                | Symbol::DotDot
                | Symbol::DotDotLess
                | Symbol::Less
                | Symbol::Greater
                | Symbol::LessEqual
                | Symbol::GreaterEqual
                | Symbol::EqualEqual
                | Symbol::BangEqual
                | Symbol::Ampersand
                | Symbol::AndAnd
                | Symbol::OrOr
                | Symbol::PlusEqual
                | Symbol::MinusEqual
                | Symbol::StarEqual
                | Symbol::SlashEqual
                | Symbol::PercentEqual
                | Symbol::Equal
                | Symbol::AsQuestion
                | Symbol::BangIn
                | Symbol::BangIs,
            ) => {}
        },
    }
}

fn any_lexeme_kind() -> LexemeKind {
    let tag: u8 = kani::any();
    kani::assume(tag < 4);
    let kind = match tag {
        0 => LexemeKind::Token(any_token_kind()),
        1 => LexemeKind::Trivia(any_trivia_kind()),
        2 => LexemeKind::Invalid(any_invalid_kind()),
        _ => LexemeKind::Eof,
    };
    assert_lexeme_kind_exhaustive(kind);
    kind
}

fn any_tail_event() -> DelegationLambdaTailEvent {
    let tag: u8 = kani::any();
    kani::assume(tag < 6);
    match tag {
        0 => DelegationLambdaTailEvent::Trivia,
        1 => DelegationLambdaTailEvent::SameLineClosedCall,
        2 => DelegationLambdaTailEvent::LineBreakLeftParen,
        3 => DelegationLambdaTailEvent::UnclosedLeftParen,
        4 => DelegationLambdaTailEvent::OuterHeaderDelimiter,
        _ => DelegationLambdaTailEvent::Other,
    }
}

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
    assert_ne!(
        DeclarationStops::DELEGATION_ERROR_TARGET,
        DeclarationStops::LEADING_UNPROVED_ANGLE_COMMA
    );
    assert_ne!(
        DeclarationStops::DELEGATION_ERROR_TARGET,
        DeclarationStops::DELEGATION_UNSUPPORTED_SUFFIX
    );
    assert_ne!(
        DeclarationStops::LEADING_UNPROVED_ANGLE_COMMA,
        DeclarationStops::DELEGATION_UNSUPPORTED_SUFFIX
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
