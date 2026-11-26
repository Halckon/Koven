use super::when_boundary::{
    SelectorEvent, SelectorScanState, SelectorStep, WhenEntryBody,
    is_new_selector_after_line_break, run_selector_events,
};
use crate::lexer::{Keyword, Symbol, TokenKind};

fn any_body() -> WhenEntryBody {
    if kani::any() {
        WhenEntryBody::WithSubject
    } else {
        WhenEntryBody::WithoutSubject
    }
}

fn any_control_token() -> TokenKind {
    let tag: u8 = kani::any();
    kani::assume(tag < 13);
    match tag {
        0 => TokenKind::Symbol(Symbol::Arrow),
        1 => TokenKind::Keyword(Keyword::Else),
        2 => TokenKind::Symbol(Symbol::LeftParen),
        3 => TokenKind::Symbol(Symbol::LeftBrace),
        4 => TokenKind::Symbol(Symbol::Plus),
        5 => TokenKind::Symbol(Symbol::Minus),
        6 => TokenKind::Keyword(Keyword::Is),
        7 => TokenKind::Keyword(Keyword::In),
        8 => TokenKind::Symbol(Symbol::BangIs),
        9 => TokenKind::Symbol(Symbol::BangIn),
        10 => TokenKind::Symbol(Symbol::LeftBracket),
        11 => TokenKind::Keyword(Keyword::If),
        _ => TokenKind::Identifier,
    }
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
    let kind = any_control_token();
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
