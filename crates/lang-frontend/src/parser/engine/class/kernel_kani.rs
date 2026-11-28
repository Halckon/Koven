use super::super::ClassifierKind;
use super::super::boundary::{ClassMemberContext, ReceiverModifierPolicy};
use super::kernel::{
    ClassifierKindTag, classifier_forbids_type_parameters, classifier_kind_tag,
    classifier_requires_body, classifier_requires_constructor, classifier_supports_constructor,
    is_class_field_invalid_prefix_kind, is_classifier_name_boundary_kind, is_enum_body_stop_kind,
    is_enum_variant_param_invalid_prefix_kind, member_companion_is_unsupported,
    member_receiver_policy, member_visibility_invalid_for_fun,
};
use crate::lexer::{
    FloatLiteralSuffix, IntegerLiteralSuffix, InvalidKind, Keyword, LexemeKind, ReservedWord,
    Symbol, TokenKind, TriviaKind,
};
use crate::source::Span;

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

// ClassifierKind carries Span fields; `any_classifier_kind` is defined below via
// `define_exhaustive_enum_any!` so Kani starts from production `ClassifierKind` values
// and passes through `classifier_kind_tag`.

define_exhaustive_unit_enum_any!(
    any_class_member_context,
    ClassMemberContext,
    [ValueClass, Class, Interface, Enum, Object, Companion]
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

define_exhaustive_enum_any!(
    any_classifier_kind,
    ClassifierKind,
    [
        ClassifierKind::ValueClass { .. } => ClassifierKind::ValueClass {
            value_span: Span::synthetic_for_verification(),
            class_span: Span::synthetic_for_verification(),
        },
        ClassifierKind::Class { .. } => ClassifierKind::Class {
            class_span: Span::synthetic_for_verification(),
        },
        ClassifierKind::Interface { .. } => ClassifierKind::Interface {
            interface_span: Span::synthetic_for_verification(),
        },
        ClassifierKind::EnumClass { .. } => ClassifierKind::EnumClass {
            enum_span: Span::synthetic_for_verification(),
            class_span: Span::synthetic_for_verification(),
        },
        ClassifierKind::Object { .. } => ClassifierKind::Object {
            object_span: Span::synthetic_for_verification(),
        },
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
fn verify_classifier_kind_and_member_context_policies_exhaustive() {
    let kind = any_classifier_kind();
    let tag = classifier_kind_tag(kind);
    let expected_tag = match kind {
        ClassifierKind::ValueClass { .. } => ClassifierKindTag::ValueClass,
        ClassifierKind::Class { .. } => ClassifierKindTag::Class,
        ClassifierKind::Interface { .. } => ClassifierKindTag::Interface,
        ClassifierKind::EnumClass { .. } => ClassifierKindTag::EnumClass,
        ClassifierKind::Object { .. } => ClassifierKindTag::Object,
    };
    assert_eq!(tag, expected_tag);

    let supports_ctor = classifier_supports_constructor(tag);
    let requires_ctor = classifier_requires_constructor(tag);
    let requires_body = classifier_requires_body(tag);
    let forbids_type_params = classifier_forbids_type_parameters(tag);

    assert_eq!(
        supports_ctor,
        matches!(
            kind,
            ClassifierKind::ValueClass { .. } | ClassifierKind::Class { .. }
        )
    );
    assert_eq!(
        requires_ctor,
        matches!(kind, ClassifierKind::ValueClass { .. })
    );
    assert_eq!(
        requires_body,
        matches!(kind, ClassifierKind::EnumClass { .. })
    );
    assert_eq!(
        forbids_type_params,
        matches!(kind, ClassifierKind::Object { .. })
    );
    if requires_ctor {
        assert!(supports_ctor);
    }

    let ctx = any_class_member_context();
    let allows_override = ctx.allows_override();
    let allows_companion = ctx.allows_companion();
    let allows_constant = ctx.allows_constant();
    let allows_deinit = ctx.allows_deinit();
    assert_eq!(allows_companion, !allows_constant);
    assert_eq!(allows_deinit, matches!(ctx, ClassMemberContext::Class));
    if allows_deinit {
        assert!(allows_override && allows_companion);
    }
    assert_eq!(
        member_receiver_policy(ctx),
        if matches!(ctx, ClassMemberContext::Companion) {
            ReceiverModifierPolicy::Rejected
        } else {
            ReceiverModifierPolicy::Allowed
        }
    );

    let is_fun: bool = kani::any();
    let is_internal_or_private: bool = kani::any();
    assert_eq!(
        member_visibility_invalid_for_fun(ctx, is_fun, is_internal_or_private),
        matches!(ctx, ClassMemberContext::Interface) && is_fun && is_internal_or_private
    );

    let companion_seen: bool = kani::any();
    assert_eq!(
        member_companion_is_unsupported(ctx, companion_seen),
        !allows_companion || companion_seen
    );
}

#[kani::proof]
fn verify_classifier_name_boundary_and_enum_body_stop_exhaustive() {
    let kind = any_lexeme_kind();
    let expected_name_boundary = matches!(
        kind,
        LexemeKind::Eof
            | LexemeKind::Token(TokenKind::Symbol(
                Symbol::Less
                    | Symbol::LeftParen
                    | Symbol::Colon
                    | Symbol::LeftBrace
                    | Symbol::RightBrace
            ))
    );
    let actual_name_boundary = is_classifier_name_boundary_kind(kind);
    assert_eq!(actual_name_boundary, expected_name_boundary);

    let expected_enum_stop = matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace | Symbol::Semicolon))
            | LexemeKind::Eof
    );
    let actual_enum_stop = is_enum_body_stop_kind(kind);
    assert_eq!(actual_enum_stop, expected_enum_stop);

    if actual_enum_stop
        && !matches!(
            kind,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
        )
    {
        assert!(actual_name_boundary);
    }
}

#[kani::proof]
fn verify_class_field_and_enum_param_invalid_prefix_exhaustive() {
    let kind = any_lexeme_kind();
    let is_mode_ident: bool = kani::any();
    let next1 = any_opt_lexeme_kind();

    let expected_field_invalid = (is_mode_ident
        && matches!(
            next1,
            Some(LexemeKind::Token(TokenKind::Keyword(
                Keyword::Val | Keyword::Var
            )))
        ))
        || matches!(kind, LexemeKind::Token(TokenKind::Keyword(Keyword::Vararg)));
    assert_eq!(
        is_class_field_invalid_prefix_kind(kind, is_mode_ident, next1),
        expected_field_invalid
    );

    let expected_enum_param_invalid = (is_mode_ident
        && matches!(next1, Some(LexemeKind::Token(TokenKind::Identifier))))
        || matches!(
            kind,
            LexemeKind::Token(TokenKind::Keyword(
                Keyword::Val | Keyword::Var | Keyword::Vararg
            ))
        );
    assert_eq!(
        is_enum_variant_param_invalid_prefix_kind(kind, is_mode_ident, next1),
        expected_enum_param_invalid
    );
}
