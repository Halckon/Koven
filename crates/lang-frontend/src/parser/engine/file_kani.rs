use super::boundary::ReceiverModifierPolicy;
use super::file::{
    FileHeaderSeparatorDecision, FileRootSeparatorDiagnostic, ModifierScanState,
    ModifierStepOutcome, ModifierToken, file_header_separator_decision,
    file_root_separator_diagnostic, run_declaration_modifiers, step_declaration_modifier,
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
    any_receiver_policy,
    ReceiverModifierPolicy,
    [Allowed, Rejected, NotRecognized]
);

define_exhaustive_unit_enum_any!(
    any_modifier_token,
    ModifierToken,
    [Visibility, Override, Receiver]
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
fn verify_file_header_and_root_separator_decisions() {
    let kind = any_lexeme_kind();
    let has_line_break: bool = kani::any();

    let header = file_header_separator_decision(kind, has_line_break);
    if matches!(kind, LexemeKind::Eof) || has_line_break {
        assert_eq!(header, FileHeaderSeparatorDecision::Satisfied);
    } else if matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
    ) {
        assert_eq!(header, FileHeaderSeparatorDecision::ConsumeSemicolon);
    } else {
        assert_eq!(header, FileHeaderSeparatorDecision::Missing);
    }

    let started_as_declaration: bool = kani::any();
    let at_file_boundary: bool = kani::any();
    let root = file_root_separator_diagnostic(
        started_as_declaration,
        at_file_boundary,
        has_line_break,
        kind,
    );
    if matches!(
        kind,
        LexemeKind::Eof | LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
    ) || !started_as_declaration
        || !at_file_boundary
        || has_line_break
    {
        assert_eq!(root, FileRootSeparatorDiagnostic::None);
    } else if matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(Keyword::Package | Keyword::Import))
    ) {
        assert_eq!(
            root,
            FileRootSeparatorDiagnostic::ExpectedFileHeaderSeparator
        );
    } else {
        assert_eq!(
            root,
            FileRootSeparatorDiagnostic::ExpectedDeclarationSeparator
        );
    }
}

#[kani::proof]
fn verify_declaration_modifier_step_invariants() {
    let allow_override: bool = kani::any();
    let policy = any_receiver_policy();
    let state = ModifierScanState {
        has_visibility: kani::any(),
        saw_override: kani::any(),
        saw_receiver: kani::any(),
    };
    let token = any_modifier_token();

    let (next, outcome) = step_declaration_modifier(allow_override, policy, state, token);

    if state.has_visibility {
        assert!(next.has_visibility);
    }
    if state.saw_override {
        assert!(next.saw_override);
    }
    if state.saw_receiver {
        assert!(next.saw_receiver);
    }

    let expected_next = match token {
        ModifierToken::Visibility => {
            if state.has_visibility || state.saw_override || state.saw_receiver {
                state
            } else {
                ModifierScanState {
                    has_visibility: true,
                    ..state
                }
            }
        }
        ModifierToken::Override => ModifierScanState {
            saw_override: true,
            ..state
        },
        ModifierToken::Receiver => match policy {
            ReceiverModifierPolicy::NotRecognized => state,
            ReceiverModifierPolicy::Rejected | ReceiverModifierPolicy::Allowed => {
                ModifierScanState {
                    saw_receiver: true,
                    ..state
                }
            }
        },
    };
    assert_eq!(next, expected_next);

    assert_eq!(
        outcome == ModifierStepOutcome::Stop,
        token == ModifierToken::Receiver && policy == ReceiverModifierPolicy::NotRecognized
    );
    let expected_accept = match token {
        ModifierToken::Visibility => {
            !state.has_visibility && !state.saw_override && !state.saw_receiver
        }
        ModifierToken::Override => allow_override && !state.saw_override && !state.saw_receiver,
        ModifierToken::Receiver => policy == ReceiverModifierPolicy::Allowed && !state.saw_receiver,
    };
    assert_eq!(outcome == ModifierStepOutcome::Accept, expected_accept);

    // Explicitly check rejected Override followed by Visibility when override is disallowed.
    let (after_override, override_outcome) = step_declaration_modifier(
        false,
        policy,
        ModifierScanState::default(),
        ModifierToken::Override,
    );
    assert_eq!(override_outcome, ModifierStepOutcome::Invalid);
    assert!(after_override.saw_override);
    assert!(!after_override.has_visibility);
    let (after_visibility, visibility_outcome) =
        step_declaration_modifier(false, policy, after_override, ModifierToken::Visibility);
    assert_eq!(visibility_outcome, ModifierStepOutcome::Invalid);
    assert!(after_visibility.saw_override);
    assert!(!after_visibility.has_visibility);
}

#[kani::proof]
#[kani::unwind(8)]
fn verify_declaration_modifier_run_len_6() {
    let allow_override: bool = kani::any();
    let policy = any_receiver_policy();
    let tokens = [
        any_modifier_token(),
        any_modifier_token(),
        any_modifier_token(),
        any_modifier_token(),
        any_modifier_token(),
        any_modifier_token(),
    ];
    let len: usize = kani::any();
    kani::assume(len <= 6);

    let (final_state, errors) = run_declaration_modifiers(
        allow_override,
        policy,
        ModifierScanState::default(),
        &tokens[..len],
    );

    let mut had_stop = false;
    let mut i = 0usize;
    while i < len {
        if tokens[i] == ModifierToken::Receiver && policy == ReceiverModifierPolicy::NotRecognized {
            had_stop = true;
            break;
        }
        i += 1;
    }

    if !had_stop && errors == 0 {
        assert!(len <= 3);
    }
    let rank = usize::from(final_state.has_visibility)
        + usize::from(final_state.saw_override)
        + usize::from(final_state.saw_receiver);
    assert!(rank <= 3);

    let (rej_state, rej_errors) = run_declaration_modifiers(
        false,
        policy,
        ModifierScanState::default(),
        &[ModifierToken::Override, ModifierToken::Visibility],
    );
    assert!(rej_state.saw_override);
    assert!(!rej_state.has_visibility);
    assert_eq!(rej_errors, 2);
}
