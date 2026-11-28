use super::operator::{
    SoftInfixKeyword, can_start_expression_kind, can_start_type_ref_kind, infix_rule_from_parts,
    is_declaration_trailing_poison_decision, is_poison_lexeme_kind,
    is_unsupported_adjacent_operator_pair,
};
use super::{InfixKind, NonAssociativeGroup};

// Independent language-spec precedence table (not imported from production `PREC_*` constants)
// so swapping any production precedence tier (e.g. additive vs multiplicative) is caught.
const SPEC_PREC_ASSIGNMENT: u8 = 1;
const SPEC_PREC_OR: u8 = 2;
const SPEC_PREC_AND: u8 = 3;
const SPEC_PREC_EQUALITY: u8 = 4;
const SPEC_PREC_COMPARISON: u8 = 5;
const SPEC_PREC_MEMBERSHIP: u8 = 6;
const SPEC_PREC_ELVIS: u8 = 7;
const SPEC_PREC_TO: u8 = 8;
const SPEC_PREC_RANGE: u8 = 9;
const SPEC_PREC_BITWISE_OR: u8 = 10;
const SPEC_PREC_BITWISE_XOR: u8 = 11;
const SPEC_PREC_BITWISE_AND: u8 = 12;
const SPEC_PREC_SHIFT: u8 = 13;
const SPEC_PREC_ADDITIVE: u8 = 14;
const SPEC_PREC_MULTIPLICATIVE: u8 = 15;
const SPEC_PREC_CAST: u8 = 16;
const SPEC_PREC_PREFIX: u8 = 17;
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

define_exhaustive_unit_enum_any!(
    any_soft_infix_keyword,
    SoftInfixKeyword,
    [To, Shl, Shr, Ushr, And, Xor, Or]
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

fn any_opt_soft_infix_keyword() -> Option<SoftInfixKeyword> {
    if kani::any() {
        Some(any_soft_infix_keyword())
    } else {
        None
    }
}

#[kani::proof]
fn verify_unsupported_adjacent_operator_pair_exhaustive() {
    let first = any_lexeme_kind();
    let second = any_lexeme_kind();
    let adjacent: bool = kani::any();

    let expected = adjacent
        && matches!(
            (first, second),
            (
                LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)),
                LexemeKind::Token(TokenKind::Symbol(Symbol::Plus))
            ) | (
                LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)),
                LexemeKind::Token(TokenKind::Symbol(Symbol::Minus))
            ) | (
                LexemeKind::Token(TokenKind::Symbol(Symbol::Less)),
                LexemeKind::Token(TokenKind::Symbol(Symbol::Less))
            ) | (
                LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)),
                LexemeKind::Token(TokenKind::Symbol(Symbol::Greater))
            ) | (
                LexemeKind::Token(TokenKind::Symbol(Symbol::DotDot)),
                LexemeKind::Token(TokenKind::Symbol(Symbol::Dot))
            )
        );
    assert_eq!(
        is_unsupported_adjacent_operator_pair(first, second, adjacent),
        expected
    );
}

#[kani::proof]
fn verify_expression_type_poison_and_trailing_poison_exhaustive() {
    let kind = any_lexeme_kind();
    let has_string_recovery: bool = kani::any();
    let has_lexical_poison_string_recovery: bool = kani::any();

    let expected_poison = matches!(
        kind,
        LexemeKind::Invalid(_) | LexemeKind::Token(TokenKind::ReservedWord(_))
    );
    let expected_type_ref = matches!(
        kind,
        LexemeKind::Token(TokenKind::Identifier)
            | LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen))
    );
    let expected_expr = matches!(
        kind,
        LexemeKind::Token(
            TokenKind::Identifier
                | TokenKind::IntegerLiteral(_)
                | TokenKind::FloatLiteral(_)
                | TokenKind::CharLiteral
                | TokenKind::StringStart
        ) | LexemeKind::Token(TokenKind::Keyword(
            Keyword::True
                | Keyword::False
                | Keyword::Null
                | Keyword::This
                | Keyword::If
                | Keyword::When
                | Keyword::Return
                | Keyword::Break
                | Keyword::Continue
                | Keyword::Super
        )) | LexemeKind::Token(TokenKind::Symbol(
            Symbol::LeftParen
                | Symbol::LeftBrace
                | Symbol::ColonColon
                | Symbol::Bang
                | Symbol::Plus
                | Symbol::Minus
        ))
    ) || expected_poison;

    let actual_poison = is_poison_lexeme_kind(kind);
    let actual_type_ref = can_start_type_ref_kind(kind);
    let actual_expr = can_start_expression_kind(kind);
    assert_eq!(actual_poison, expected_poison);
    assert_eq!(actual_type_ref, expected_type_ref);
    assert_eq!(actual_expr, expected_expr);

    if actual_type_ref {
        assert!(actual_expr);
    }
    if actual_poison {
        assert!(actual_expr);
        assert!(!actual_type_ref);
    }

    let expected_trailing = expected_poison
        || (matches!(kind, LexemeKind::Token(TokenKind::StringStart))
            && (has_string_recovery || has_lexical_poison_string_recovery));
    assert_eq!(
        is_declaration_trailing_poison_decision(
            kind,
            has_string_recovery,
            has_lexical_poison_string_recovery
        ),
        expected_trailing
    );
}

#[kani::proof]
fn verify_infix_rule_from_parts_exhaustive() {
    let kind = any_lexeme_kind();
    let soft = any_opt_soft_infix_keyword();
    let actual = infix_rule_from_parts(kind, soft);

    const {
        assert!(SPEC_PREC_ASSIGNMENT < SPEC_PREC_OR);
        assert!(SPEC_PREC_OR < SPEC_PREC_AND);
        assert!(SPEC_PREC_AND < SPEC_PREC_EQUALITY);
        assert!(SPEC_PREC_EQUALITY < SPEC_PREC_COMPARISON);
        assert!(SPEC_PREC_COMPARISON < SPEC_PREC_MEMBERSHIP);
        assert!(SPEC_PREC_MEMBERSHIP < SPEC_PREC_ELVIS);
        assert!(SPEC_PREC_ELVIS < SPEC_PREC_TO);
        assert!(SPEC_PREC_TO < SPEC_PREC_RANGE);
        assert!(SPEC_PREC_RANGE < SPEC_PREC_BITWISE_OR);
        assert!(SPEC_PREC_BITWISE_OR < SPEC_PREC_BITWISE_XOR);
        assert!(SPEC_PREC_BITWISE_XOR < SPEC_PREC_BITWISE_AND);
        assert!(SPEC_PREC_BITWISE_AND < SPEC_PREC_SHIFT);
        assert!(SPEC_PREC_SHIFT < SPEC_PREC_ADDITIVE);
        assert!(SPEC_PREC_ADDITIVE < SPEC_PREC_MULTIPLICATIVE);
        assert!(SPEC_PREC_MULTIPLICATIVE < SPEC_PREC_CAST);
        assert!(SPEC_PREC_CAST < SPEC_PREC_PREFIX);
    }

    let expected: Option<(u8, bool, Option<NonAssociativeGroup>, bool)> = match kind {
        LexemeKind::Token(TokenKind::Symbol(Symbol::Star | Symbol::Slash | Symbol::Percent)) => {
            Some((SPEC_PREC_MULTIPLICATIVE, false, None, false))
        }
        LexemeKind::Token(TokenKind::Symbol(Symbol::Plus | Symbol::Minus)) => {
            Some((SPEC_PREC_ADDITIVE, false, None, false))
        }
        LexemeKind::Token(TokenKind::Symbol(Symbol::DotDot | Symbol::DotDotLess)) => Some((
            SPEC_PREC_RANGE,
            false,
            Some(NonAssociativeGroup::Range),
            false,
        )),
        LexemeKind::Token(TokenKind::Identifier) => match soft {
            Some(SoftInfixKeyword::To) => Some((SPEC_PREC_TO, false, None, false)),
            Some(SoftInfixKeyword::Shl | SoftInfixKeyword::Shr | SoftInfixKeyword::Ushr) => {
                Some((SPEC_PREC_SHIFT, false, None, false))
            }
            Some(SoftInfixKeyword::And) => Some((SPEC_PREC_BITWISE_AND, false, None, false)),
            Some(SoftInfixKeyword::Xor) => Some((SPEC_PREC_BITWISE_XOR, false, None, false)),
            Some(SoftInfixKeyword::Or) => Some((SPEC_PREC_BITWISE_OR, false, None, false)),
            None => None,
        },
        LexemeKind::Token(TokenKind::Symbol(Symbol::QuestionColon)) => {
            Some((SPEC_PREC_ELVIS, true, None, false))
        }
        LexemeKind::Token(TokenKind::Keyword(Keyword::In))
        | LexemeKind::Token(TokenKind::Symbol(Symbol::BangIn)) => Some((
            SPEC_PREC_MEMBERSHIP,
            false,
            Some(NonAssociativeGroup::Membership),
            false,
        )),
        LexemeKind::Token(TokenKind::Keyword(Keyword::Is))
        | LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs)) => Some((
            SPEC_PREC_MEMBERSHIP,
            false,
            Some(NonAssociativeGroup::Membership),
            true,
        )),
        LexemeKind::Token(TokenKind::Symbol(
            Symbol::Less | Symbol::Greater | Symbol::LessEqual | Symbol::GreaterEqual,
        )) => Some((
            SPEC_PREC_COMPARISON,
            false,
            Some(NonAssociativeGroup::Comparison),
            false,
        )),
        LexemeKind::Token(TokenKind::Symbol(Symbol::EqualEqual | Symbol::BangEqual)) => Some((
            SPEC_PREC_EQUALITY,
            false,
            Some(NonAssociativeGroup::Equality),
            false,
        )),
        LexemeKind::Token(TokenKind::Symbol(Symbol::AndAnd)) => {
            Some((SPEC_PREC_AND, false, None, false))
        }
        LexemeKind::Token(TokenKind::Symbol(Symbol::OrOr)) => {
            Some((SPEC_PREC_OR, false, None, false))
        }
        LexemeKind::Token(TokenKind::Keyword(Keyword::As))
        | LexemeKind::Token(TokenKind::Symbol(Symbol::AsQuestion)) => {
            Some((SPEC_PREC_CAST, false, None, true))
        }
        LexemeKind::Token(TokenKind::Symbol(
            Symbol::Equal
            | Symbol::PlusEqual
            | Symbol::MinusEqual
            | Symbol::StarEqual
            | Symbol::SlashEqual
            | Symbol::PercentEqual,
        )) => Some((SPEC_PREC_ASSIGNMENT, true, None, false)),
        _ => None,
    };

    match (actual, expected) {
        (Some(rule), Some((prec, right_assoc, group, is_type_operand))) => {
            assert_eq!(rule.precedence, prec);
            assert!(rule.precedence >= SPEC_PREC_ASSIGNMENT && rule.precedence < SPEC_PREC_PREFIX);
            let expected_right_prec = if right_assoc { prec } else { prec + 1 };
            assert_eq!(rule.right_precedence, expected_right_prec);
            assert_eq!(rule.non_associative_group, group);
            if rule.non_associative_group.is_some() {
                assert_eq!(rule.right_precedence, rule.precedence + 1);
            }
            let actual_type_operand =
                matches!(rule.kind, InfixKind::Cast(_) | InfixKind::TypeTest { .. });
            assert_eq!(actual_type_operand, is_type_operand);
        }
        (None, None) => {}
        _ => panic!("infix_rule_from_parts diverged from independent specification"),
    }

    if is_poison_lexeme_kind(kind) {
        assert!(actual.is_none());
    }
    if !matches!(kind, LexemeKind::Token(TokenKind::Identifier)) {
        let without_soft = infix_rule_from_parts(kind, None);
        assert_eq!(actual.is_some(), without_soft.is_some());
        if let (Some(a), Some(b)) = (actual, without_soft) {
            assert_eq!(a.precedence, b.precedence);
            assert_eq!(a.right_precedence, b.right_precedence);
            assert_eq!(a.non_associative_group, b.non_associative_group);
        }
    }
}
