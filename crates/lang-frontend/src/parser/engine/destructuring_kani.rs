use super::destructuring::{
    DestructuringEvent, DestructuringExitKind, DestructuringLoopState,
    destructuring_exit_emits_missing_binding, destructuring_initializer_missing_diagnostic,
    is_const_local_destructuring_start_kinds, is_local_destructuring_start_kinds,
    run_destructuring_loop, step_destructuring_loop,
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
    any_destructuring_event,
    DestructuringEvent,
    [EmptyComma, Binding, SeparatorComma, MissingSeparatorIdent]
);

define_exhaustive_unit_enum_any!(
    any_destructuring_exit,
    DestructuringExitKind,
    [RightParen, Boundary]
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

fn any_opt_lexeme_kind() -> Option<LexemeKind> {
    if kani::any() {
        Some(any_lexeme_kind())
    } else {
        None
    }
}

#[kani::proof]
fn verify_destructuring_start_predicates() {
    let keyword = any_keyword();
    let first = any_lexeme_kind();
    let second = any_opt_lexeme_kind();
    let third = any_opt_lexeme_kind();

    let expected_local = first == LexemeKind::Token(TokenKind::Keyword(keyword))
        && second == Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)));
    assert_eq!(
        is_local_destructuring_start_kinds(keyword, first, second),
        expected_local
    );

    let expected_const = first == LexemeKind::Token(TokenKind::Keyword(Keyword::Const))
        && second == Some(LexemeKind::Token(TokenKind::Keyword(Keyword::Val)))
        && third == Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)));
    assert_eq!(
        is_const_local_destructuring_start_kinds(first, second, third),
        expected_const
    );
}

#[kani::proof]
fn verify_destructuring_exit_and_initializer_guards() {
    let exit = any_destructuring_exit();
    let state = DestructuringLoopState {
        has_bindings: kani::any(),
        comma_already_reported_as_empty: kani::any(),
        needs_binding: kani::any(),
    };
    let expected_exit = match exit {
        DestructuringExitKind::RightParen => !state.has_bindings,
        DestructuringExitKind::Boundary => {
            state.needs_binding && !state.comma_already_reported_as_empty
        }
    };
    assert_eq!(
        destructuring_exit_emits_missing_binding(exit, state),
        expected_exit
    );

    let has_equals: bool = kani::any();
    let boundary: bool = kani::any();
    let starts_primary: bool = kani::any();
    let terminal_lexer_root: bool = kani::any();
    let is_poison: bool = kani::any();
    assert_eq!(
        destructuring_initializer_missing_diagnostic(
            has_equals,
            boundary,
            starts_primary,
            terminal_lexer_root,
            is_poison,
        ),
        has_equals && boundary && !starts_primary && !terminal_lexer_root && !is_poison
    );
}

#[kani::proof]
#[kani::unwind(8)]
fn verify_destructuring_loop_state_invariant_len_6() {
    let events = [
        any_destructuring_event(),
        any_destructuring_event(),
        any_destructuring_event(),
        any_destructuring_event(),
        any_destructuring_event(),
        any_destructuring_event(),
    ];
    let len: usize = kani::any();
    kani::assume(len <= 6);

    let initial = DestructuringLoopState::new();
    let final_state = run_destructuring_loop(initial, &events[..len]);

    let mut cur = initial;
    let mut i = 0usize;
    while i < len {
        let next = step_destructuring_loop(cur, events[i]);
        let expected_next = match events[i] {
            DestructuringEvent::EmptyComma => DestructuringLoopState {
                has_bindings: true,
                comma_already_reported_as_empty: true,
                needs_binding: true,
            },
            DestructuringEvent::Binding => DestructuringLoopState {
                has_bindings: true,
                comma_already_reported_as_empty: false,
                needs_binding: false,
            },
            DestructuringEvent::SeparatorComma | DestructuringEvent::MissingSeparatorIdent => {
                DestructuringLoopState {
                    needs_binding: true,
                    ..cur
                }
            }
        };
        assert_eq!(next, expected_next);
        if cur.has_bindings {
            assert!(next.has_bindings);
        }
        if events[i] == DestructuringEvent::EmptyComma {
            assert!(!destructuring_exit_emits_missing_binding(
                DestructuringExitKind::RightParen,
                next
            ));
            assert!(!destructuring_exit_emits_missing_binding(
                DestructuringExitKind::Boundary,
                next
            ));
        }
        if next.comma_already_reported_as_empty {
            assert!(next.needs_binding);
            assert!(next.has_bindings);
        }
        cur = next;
        i += 1;
    }
    assert_eq!(final_state, cur);

    // Explicitly check `val (x,, = ...` (Binding -> SeparatorComma -> EmptyComma):
    // EmptyComma must set comma_already_reported_as_empty so neither exit re-emits.
    let after_empty_comma = run_destructuring_loop(
        DestructuringLoopState::new(),
        &[
            DestructuringEvent::Binding,
            DestructuringEvent::SeparatorComma,
            DestructuringEvent::EmptyComma,
        ],
    );
    assert_eq!(
        after_empty_comma,
        DestructuringLoopState {
            has_bindings: true,
            comma_already_reported_as_empty: true,
            needs_binding: true,
        }
    );
    assert!(!destructuring_exit_emits_missing_binding(
        DestructuringExitKind::RightParen,
        after_empty_comma
    ));
    assert!(!destructuring_exit_emits_missing_binding(
        DestructuringExitKind::Boundary,
        after_empty_comma
    ));

    // Explicitly check `val (,x, = v` (EmptyComma -> Binding -> SeparatorComma -> Boundary):
    // Binding must clear comma_already_reported_as_empty so the trailing boundary still reports.
    let after_binding_sep = run_destructuring_loop(
        DestructuringLoopState::new(),
        &[
            DestructuringEvent::EmptyComma,
            DestructuringEvent::Binding,
            DestructuringEvent::SeparatorComma,
        ],
    );
    assert_eq!(
        after_binding_sep,
        DestructuringLoopState {
            has_bindings: true,
            comma_already_reported_as_empty: false,
            needs_binding: true,
        }
    );
    assert!(!destructuring_exit_emits_missing_binding(
        DestructuringExitKind::RightParen,
        after_binding_sep
    ));
    assert!(destructuring_exit_emits_missing_binding(
        DestructuringExitKind::Boundary,
        after_binding_sep
    ));
}
