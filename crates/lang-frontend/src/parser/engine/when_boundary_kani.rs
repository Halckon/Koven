use super::when_boundary::{
    SelectorEvent, SelectorScanState, SelectorStep, WhenEntryBody,
    is_new_selector_after_line_break, run_selector_events,
};
use crate::lexer::{
    FloatLiteralSuffix, IntegerLiteralSuffix, Keyword, ReservedWord, Symbol, TokenKind,
};

fn any_body() -> WhenEntryBody {
    if kani::any() {
        WhenEntryBody::WithSubject
    } else {
        WhenEntryBody::WithoutSubject
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

/// Compile-time exhaustiveness witness ensuring `any_token_kind()` stays in sync
/// with every production `TokenKind`, `Keyword`, `ReservedWord`, `Symbol`, and literal suffix.
const fn assert_token_kind_exhaustive(kind: TokenKind) {
    match kind {
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
    }
}

fn any_token_kind() -> TokenKind {
    let tag: u8 = kani::any();
    kani::assume(tag < 12);
    let kind = match tag {
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
    };
    assert_token_kind_exhaustive(kind);
    kind
}

fn any_event() -> SelectorEvent {
    let tag: u8 = kani::any();
    kani::assume(tag < 10);
    match tag {
        0 => SelectorEvent::Arrow,
        1 => SelectorEvent::Else,
        2 => SelectorEvent::NewSelectorAfterLineBreak,
        3 => SelectorEvent::NonOperandContinuation,
        4 => SelectorEvent::NonOperandExpectsOperand,
        5 => SelectorEvent::NonOperandGroupFallthrough,
        6 => SelectorEvent::OperandIf,
        7 => SelectorEvent::OperandPrefixContinue,
        8 => SelectorEvent::OperandComplete,
        _ => SelectorEvent::Reject,
    }
}

#[kani::proof]
fn verify_when_boundary_new_selector_and_control() {
    let body = any_body();
    let kind = any_token_kind();
    let has_line_break: bool = kani::any();
    let state = SelectorScanState {
        needs_operand: kani::any(),
        pending_else: kani::any(),
    };

    let always_new = matches!(
        kind,
        TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBrace | Symbol::Plus | Symbol::Minus)
    );
    let subject_only_new = matches!(
        kind,
        TokenKind::Keyword(Keyword::Is | Keyword::In)
            | TokenKind::Symbol(Symbol::BangIs | Symbol::BangIn)
    );
    let expected_new = always_new || (body == WhenEntryBody::WithSubject && subject_only_new);
    assert_eq!(is_new_selector_after_line_break(body, kind), expected_new);

    let result = state.step_non_operand_control(body, kind, has_line_break);
    if state.needs_operand {
        assert!(result.is_none());
    } else if kind == TokenKind::Symbol(Symbol::Arrow) {
        assert_eq!(result, Some(SelectorStep::Finish(state.pending_else == 0)));
    } else if state.pending_else > 0 && kind == TokenKind::Keyword(Keyword::Else) {
        assert_eq!(
            result,
            Some(SelectorStep::Continue(SelectorScanState {
                needs_operand: true,
                pending_else: state.pending_else - 1,
            }))
        );
    } else if expected_new && has_line_break {
        assert_eq!(result, Some(SelectorStep::Finish(false)));
    } else {
        assert!(result.is_none());
    }
}

#[kani::proof]
fn verify_when_boundary_step_invariants() {
    let state = SelectorScanState {
        needs_operand: kani::any(),
        pending_else: kani::any(),
    };
    let event = any_event();
    let step = state.step(event);

    match event {
        SelectorEvent::Arrow => {
            assert_eq!(
                step,
                SelectorStep::Finish(!state.needs_operand && state.pending_else == 0)
            );
        }
        SelectorEvent::Else => {
            if !state.needs_operand && state.pending_else > 0 {
                assert_eq!(
                    step,
                    SelectorStep::Continue(SelectorScanState {
                        needs_operand: true,
                        pending_else: state.pending_else - 1,
                    })
                );
            } else {
                assert_eq!(step, SelectorStep::Finish(false));
            }
        }
        SelectorEvent::OperandIf => {
            if state.needs_operand && state.pending_else < usize::MAX {
                assert_eq!(
                    step,
                    SelectorStep::Continue(SelectorScanState {
                        needs_operand: true,
                        pending_else: state.pending_else + 1,
                    })
                );
            } else {
                assert_eq!(step, SelectorStep::Finish(false));
            }
        }
        SelectorEvent::OperandComplete => {
            if state.needs_operand {
                assert_eq!(
                    step,
                    SelectorStep::Continue(SelectorScanState {
                        needs_operand: false,
                        pending_else: state.pending_else,
                    })
                );
            } else {
                assert_eq!(step, SelectorStep::Finish(false));
            }
        }
        SelectorEvent::NewSelectorAfterLineBreak | SelectorEvent::Reject => {
            assert_eq!(step, SelectorStep::Finish(false));
        }
        SelectorEvent::NonOperandContinuation | SelectorEvent::NonOperandGroupFallthrough => {
            if !state.needs_operand {
                assert_eq!(step, SelectorStep::Continue(state));
            } else {
                assert_eq!(step, SelectorStep::Finish(false));
            }
        }
        SelectorEvent::NonOperandExpectsOperand => {
            if !state.needs_operand {
                assert_eq!(
                    step,
                    SelectorStep::Continue(SelectorScanState {
                        needs_operand: true,
                        pending_else: state.pending_else,
                    })
                );
            } else {
                assert_eq!(step, SelectorStep::Finish(false));
            }
        }
        SelectorEvent::OperandPrefixContinue => {
            if state.needs_operand {
                assert_eq!(step, SelectorStep::Continue(state));
            } else {
                assert_eq!(step, SelectorStep::Finish(false));
            }
        }
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn verify_when_boundary_run_events_balance_len_6() {
    let events = [
        any_event(),
        any_event(),
        any_event(),
        any_event(),
        any_event(),
        any_event(),
    ];
    let len: usize = kani::any();
    kani::assume(len <= 6);

    let accepted = run_selector_events(&events[..len]);

    let mut state = SelectorScanState::INITIAL;
    let mut if_count = 0usize;
    let mut else_count = 0usize;
    let mut accepted_at_arrow = false;
    let mut terminated = false;

    let mut i = 0usize;
    while i < len {
        let ev = events[i];
        match state.step(ev) {
            SelectorStep::Continue(next) => {
                if ev == SelectorEvent::OperandIf {
                    if_count += 1;
                } else if ev == SelectorEvent::Else {
                    else_count += 1;
                }
                assert!(else_count <= if_count);
                assert_eq!(next.pending_else + else_count, if_count);
                state = next;
            }
            SelectorStep::Finish(ok) => {
                terminated = true;
                if ok {
                    assert_eq!(ev, SelectorEvent::Arrow);
                    assert!(!state.needs_operand);
                    assert_eq!(state.pending_else, 0);
                    assert_eq!(if_count, else_count);
                    accepted_at_arrow = true;
                }
                break;
            }
        }
        i += 1;
    }

    assert_eq!(accepted, accepted_at_arrow);
    if !terminated {
        assert!(!accepted);
    }
}
