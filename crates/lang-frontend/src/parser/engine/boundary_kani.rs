use super::boundary::{
    DeclarationStops, NameContext, TypeStops, class_field_start_kind, class_member_start_kind,
    classifier_declaration_start_kind, control_expression_start_kind, declaration_modifier_start,
    file_construct_start_kind, simple_declaration_start_kind,
};
use super::class::ClassifierHeaderOwner;
use super::{DeclarationModifiers, ParameterModeMarker, VisibilityModifier};
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

define_exhaustive_unit_enum_any!(
    any_name_context,
    NameContext,
    [Declaration, LocalDeclaration, TypeParameter, ValueParameter]
);

define_exhaustive_unit_enum_any!(
    any_classifier_header_owner,
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
fn verify_boundary_starter_hierarchy_and_disjointness_exhaustive() {
    let kind = any_lexeme_kind();

    let expected_control = matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::If
                | Keyword::When
                | Keyword::Return
                | Keyword::Break
                | Keyword::Continue
                | Keyword::Super
        ))
    );
    let expected_classifier = matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Class | Keyword::Interface | Keyword::Enum | Keyword::Object
        ))
    );
    let expected_field = matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Val
                | Keyword::Var
                | Keyword::Public
                | Keyword::Internal
                | Keyword::Private
                | Keyword::Override
        ))
    );
    let expected_simple = matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Val
                | Keyword::Var
                | Keyword::Const
                | Keyword::Fun
                | Keyword::Class
                | Keyword::Interface
                | Keyword::Enum
                | Keyword::Object
                | Keyword::Public
                | Keyword::Internal
                | Keyword::Private
                | Keyword::Override
        ))
    );
    let expected_member = expected_simple
        || matches!(
            kind,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Companion))
        );
    let expected_file = expected_simple
        || matches!(
            kind,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Package | Keyword::Import))
        );

    let actual_control = control_expression_start_kind(kind);
    let actual_classifier = classifier_declaration_start_kind(kind);
    let actual_field = class_field_start_kind(kind);
    let actual_simple = simple_declaration_start_kind(kind);
    let actual_member = class_member_start_kind(kind);
    let actual_file = file_construct_start_kind(kind);

    assert_eq!(actual_control, expected_control);
    assert_eq!(actual_classifier, expected_classifier);
    assert_eq!(actual_field, expected_field);
    assert_eq!(actual_simple, expected_simple);
    assert_eq!(actual_member, expected_member);
    assert_eq!(actual_file, expected_file);

    if actual_classifier {
        assert!(actual_simple);
        assert!(actual_member);
    }
    if actual_simple {
        assert!(actual_file);
    }
    if actual_field {
        assert!(actual_simple);
        assert!(actual_member);
    }
    if actual_control {
        assert!(!actual_file);
        assert!(!actual_member);
    }
}

#[kani::proof]
fn verify_name_context_stop_and_recovery_alignment_exhaustive() {
    let ctx = any_name_context();
    let kind = any_lexeme_kind();
    let symbol = match kind {
        LexemeKind::Token(TokenKind::Symbol(sym)) => Some(sym),
        _ => None,
    };

    let is_stop = ctx.is_stop_kind(kind);
    let stops = ctx.recovery_stops();
    let recovery_hit =
        stops.contains_hard_kind(kind, symbol) || stops.contains_soft_kind(kind, symbol);
    assert_eq!(is_stop, recovery_hit);
}

#[kani::proof]
fn verify_modifier_start_min_and_angle_comma_and_supertype_recovery() {
    let vis_opt: Option<usize> = if kani::any() { Some(kani::any()) } else { None };
    let ovr_opt: Option<usize> = if kani::any() { Some(kani::any()) } else { None };
    let recv_opt: Option<usize> = if kani::any() { Some(kani::any()) } else { None };

    let visibility = vis_opt.map(|pos| {
        let span = Span::synthetic_range_for_verification(pos, pos);
        let which: u8 = kani::any();
        kani::assume(which < 3);
        match which {
            0 => VisibilityModifier::Public(span),
            1 => VisibilityModifier::Internal(span),
            _ => VisibilityModifier::Private(span),
        }
    });
    let override_span = ovr_opt.map(|pos| Span::synthetic_range_for_verification(pos, pos));
    let receiver_mode = recv_opt.map(|pos| {
        let span = Span::synthetic_range_for_verification(pos, pos);
        let which: u8 = kani::any();
        kani::assume(which < 3);
        match which {
            0 => ParameterModeMarker::Own(span),
            1 => ParameterModeMarker::Borrow(span),
            _ => ParameterModeMarker::Inout(span),
        }
    });

    let actual_start = declaration_modifier_start(DeclarationModifiers {
        visibility,
        override_span,
        receiver_mode,
    });
    match actual_start {
        None => assert!(vis_opt.is_none() && ovr_opt.is_none() && recv_opt.is_none()),
        Some(min_pos) => {
            if let Some(v) = vis_opt {
                assert!(min_pos <= v);
            }
            if let Some(o) = ovr_opt {
                assert!(min_pos <= o);
            }
            if let Some(r) = recv_opt {
                assert!(min_pos <= r);
            }
            assert!(
                vis_opt == Some(min_pos) || ovr_opt == Some(min_pos) || recv_opt == Some(min_pos)
            );
        }
    }

    let raw_bits: u32 = kani::any();
    let leading: bool = kani::any();
    let decl_stops = DeclarationStops(raw_bits);
    let expected_keeps = (raw_bits & DeclarationStops::DELEGATION_ERROR_TARGET != 0)
        || (raw_bits & DeclarationStops::DELEGATION_UNSUPPORTED_SUFFIX != 0)
        || (leading && (raw_bits & DeclarationStops::LEADING_UNPROVED_ANGLE_COMMA != 0));
    assert_eq!(
        decl_stops.keeps_unproved_angle_comma(leading),
        expected_keeps
    );

    let type_bits: u16 = kani::any();
    let owner = any_classifier_header_owner();
    let type_stops = TypeStops::empty().with(type_bits);
    let recovered = type_stops.supertype_element_recovery(owner);
    assert!(recovered.has_supertype_suffix());
    assert!(recovered.recovers_type_element());
    assert!(recovered.has_semicolon());
    assert_eq!(
        recovered.0 & DeclarationStops::LEFT_BRACE != 0,
        (type_bits & (TypeStops::LEFT_BRACE | TypeStops::CLASS_BODY)) != 0
    );
    assert_eq!(
        recovered.has_class_file_boundary(),
        matches!(owner, ClassifierHeaderOwner::File)
    );
    assert_eq!(
        recovered.stops_at_class_member(),
        matches!(owner, ClassifierHeaderOwner::Member)
    );
    assert!(!(recovered.has_class_file_boundary() && recovered.stops_at_class_member()));
}
