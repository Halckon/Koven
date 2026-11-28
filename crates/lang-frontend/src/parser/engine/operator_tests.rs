use super::operator::{
    SoftInfixKeyword, can_start_expression_kind, can_start_type_ref_kind, infix_rule_from_parts,
    is_declaration_trailing_poison_decision, is_poison_lexeme_kind,
    is_unsupported_adjacent_operator_pair,
};
use super::{
    BinaryOperator, Expression, InfixKind, NonAssociativeGroup, PREC_ADDITIVE, PREC_ASSIGNMENT,
    PREC_CAST, PREC_COMPARISON, PREC_ELVIS, PREC_EQUALITY, PREC_MEMBERSHIP, PREC_MULTIPLICATIVE,
    PREC_PREFIX, PREC_RANGE, parse,
};
use crate::lexer::{
    InvalidKind, Keyword, LexemeKind, ReservedWord, Symbol, TokenKind, lex_test_source_twice,
};
use crate::source::SourceMap;

#[test]
fn unsupported_adjacent_operator_pairs_require_adjacency_and_exact_symbols() {
    for (a, b) in [
        (Symbol::Plus, Symbol::Plus),
        (Symbol::Minus, Symbol::Minus),
        (Symbol::Less, Symbol::Less),
        (Symbol::Greater, Symbol::Greater),
        (Symbol::DotDot, Symbol::Dot),
    ] {
        let first = LexemeKind::Token(TokenKind::Symbol(a));
        let second = LexemeKind::Token(TokenKind::Symbol(b));
        assert!(is_unsupported_adjacent_operator_pair(first, second, true));
        assert!(!is_unsupported_adjacent_operator_pair(first, second, false));
    }

    assert!(!is_unsupported_adjacent_operator_pair(
        LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)),
        true
    ));
    assert!(!is_unsupported_adjacent_operator_pair(
        LexemeKind::Token(TokenKind::Identifier),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)),
        true
    ));
}

#[test]
fn starter_and_poison_hierarchy_and_trailing_poison_match_spec() {
    let ident = LexemeKind::Token(TokenKind::Identifier);
    let lparen = LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen));
    let string_start = LexemeKind::Token(TokenKind::StringStart);
    let invalid = LexemeKind::Invalid(InvalidKind::UnexpectedCharacter);
    let reserved = LexemeKind::Token(TokenKind::ReservedWord(ReservedWord::Async));

    assert!(can_start_type_ref_kind(ident));
    assert!(can_start_type_ref_kind(lparen));
    assert!(can_start_expression_kind(ident));
    assert!(can_start_expression_kind(lparen));
    assert!(!can_start_type_ref_kind(string_start));
    assert!(can_start_expression_kind(string_start));

    for poison in [invalid, reserved] {
        assert!(is_poison_lexeme_kind(poison));
        assert!(can_start_expression_kind(poison));
        assert!(!can_start_type_ref_kind(poison));
        assert!(is_declaration_trailing_poison_decision(
            poison, false, false
        ));
    }

    assert!(!is_declaration_trailing_poison_decision(
        string_start,
        false,
        false
    ));
    assert!(is_declaration_trailing_poison_decision(
        string_start,
        true,
        false
    ));
    assert!(is_declaration_trailing_poison_decision(
        string_start,
        false,
        true
    ));
    assert!(!is_declaration_trailing_poison_decision(ident, true, true));
}

#[test]
fn infix_rule_from_parts_respects_precedence_associativity_and_groups() {
    let mul = infix_rule_from_parts(LexemeKind::Token(TokenKind::Symbol(Symbol::Star)), None)
        .expect("star rule");
    assert_eq!(mul.precedence, PREC_MULTIPLICATIVE);
    assert_eq!(mul.right_precedence, PREC_MULTIPLICATIVE + 1);
    assert!(mul.precedence < PREC_PREFIX);
    assert_eq!(mul.non_associative_group, None);

    let elvis = infix_rule_from_parts(
        LexemeKind::Token(TokenKind::Symbol(Symbol::QuestionColon)),
        None,
    )
    .expect("elvis rule");
    assert_eq!(elvis.precedence, PREC_ELVIS);
    assert_eq!(elvis.right_precedence, PREC_ELVIS);

    let assign = infix_rule_from_parts(LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)), None)
        .expect("assign rule");
    assert_eq!(assign.precedence, PREC_ASSIGNMENT);
    assert_eq!(assign.right_precedence, PREC_ASSIGNMENT);

    let range = infix_rule_from_parts(LexemeKind::Token(TokenKind::Symbol(Symbol::DotDot)), None)
        .expect("range rule");
    assert_eq!(range.precedence, PREC_RANGE);
    assert_eq!(
        range.non_associative_group,
        Some(NonAssociativeGroup::Range)
    );

    let membership =
        infix_rule_from_parts(LexemeKind::Token(TokenKind::Keyword(Keyword::Is)), None)
            .expect("is rule");
    assert_eq!(membership.precedence, PREC_MEMBERSHIP);
    assert_eq!(
        membership.non_associative_group,
        Some(NonAssociativeGroup::Membership)
    );
    assert!(matches!(
        membership.kind,
        InfixKind::TypeTest { negated: false }
    ));

    let cmp = infix_rule_from_parts(LexemeKind::Token(TokenKind::Symbol(Symbol::Less)), None)
        .expect("less rule");
    assert_eq!(cmp.precedence, PREC_COMPARISON);
    assert_eq!(
        cmp.non_associative_group,
        Some(NonAssociativeGroup::Comparison)
    );

    let eq = infix_rule_from_parts(
        LexemeKind::Token(TokenKind::Symbol(Symbol::EqualEqual)),
        None,
    )
    .expect("eq rule");
    assert_eq!(eq.precedence, PREC_EQUALITY);
    assert_eq!(
        eq.non_associative_group,
        Some(NonAssociativeGroup::Equality)
    );

    let cast = infix_rule_from_parts(LexemeKind::Token(TokenKind::Keyword(Keyword::As)), None)
        .expect("as rule");
    assert_eq!(cast.precedence, PREC_CAST);
    assert!(matches!(cast.kind, InfixKind::Cast(_)));

    assert!(infix_rule_from_parts(LexemeKind::Token(TokenKind::Identifier), None).is_none());
    let soft_to = infix_rule_from_parts(
        LexemeKind::Token(TokenKind::Identifier),
        Some(SoftInfixKeyword::To),
    )
    .expect("soft to rule");
    assert!(soft_to.precedence < PREC_ADDITIVE);
}

#[test]
fn operator_production_entry_preserves_associativity_and_type_operands() {
    // Left-associative '+' vs right-associative '?:'
    {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("assoc.ko", "a ?: b ?: c")
            .expect("unique source");
        let lexed = lex_test_source_twice(&sources, id, "right assoc elvis");
        let parsed = parse(&sources, &lexed).expect("elvis must parse");
        assert!(parsed.diagnostics().is_empty());
        let Expression::Binary {
            operator,
            left,
            right,
            ..
        } = parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .expect("root")
            .payload()
        else {
            panic!("expected binary elvis");
        };
        assert_eq!(*operator, BinaryOperator::Elvis);
        assert!(matches!(
            parsed
                .ast()
                .expressions()
                .get(*left)
                .expect("left")
                .payload(),
            Expression::Name
        ));
        assert!(matches!(
            parsed
                .ast()
                .expressions()
                .get(*right)
                .expect("right")
                .payload(),
            Expression::Binary {
                operator: BinaryOperator::Elvis,
                ..
            }
        ));
    }

    // Type-operand infix ('as' and 'is')
    for (src, is_cast) in [("x as Int", true), ("x is Int", false)] {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("type-op.ko", src)
            .expect("unique source");
        let lexed = lex_test_source_twice(&sources, id, "type operand infix");
        let parsed = parse(&sources, &lexed).expect("type op must parse");
        assert!(parsed.diagnostics().is_empty());
        let payload = parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .expect("root")
            .payload();
        if is_cast {
            assert!(matches!(payload, Expression::Cast { .. }));
        } else {
            assert!(matches!(
                payload,
                Expression::TypeTest { negated: false, .. }
            ));
        }
    }
}
