use super::operator::{SoftInfixKeyword, infix_rule_from_parts};
use super::{
    InfixKind, InfixRule, NonAssociativeGroup, PREC_ASSIGNMENT, PREC_CAST, PREC_COMPARISON,
    PREC_EQUALITY, PREC_MEMBERSHIP, PREC_PREFIX, PREC_RANGE,
};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind};
use crate::parser::BinaryOperator;

const ALL_INFIX_OPERATORS: &[(LexemeKind, Option<&str>)] = &[
    (LexemeKind::Token(TokenKind::Symbol(Symbol::Star)), None),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::Slash)), None),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::Percent)), None),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)), None),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)), None),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::DotDot)), None),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::DotDotLess)),
        None,
    ),
    (LexemeKind::Token(TokenKind::Identifier), Some("to")),
    (LexemeKind::Token(TokenKind::Identifier), Some("shl")),
    (LexemeKind::Token(TokenKind::Identifier), Some("shr")),
    (LexemeKind::Token(TokenKind::Identifier), Some("ushr")),
    (LexemeKind::Token(TokenKind::Identifier), Some("and")),
    (LexemeKind::Token(TokenKind::Identifier), Some("xor")),
    (LexemeKind::Token(TokenKind::Identifier), Some("or")),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::QuestionColon)),
        None,
    ),
    (LexemeKind::Token(TokenKind::Keyword(Keyword::In)), None),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::BangIn)), None),
    (LexemeKind::Token(TokenKind::Keyword(Keyword::Is)), None),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs)), None),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::Less)), None),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)), None),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::LessEqual)),
        None,
    ),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::GreaterEqual)),
        None,
    ),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::EqualEqual)),
        None,
    ),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::BangEqual)),
        None,
    ),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::AndAnd)), None),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::OrOr)), None),
    (LexemeKind::Token(TokenKind::Keyword(Keyword::As)), None),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::AsQuestion)),
        None,
    ),
    (LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)), None),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::PlusEqual)),
        None,
    ),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::MinusEqual)),
        None,
    ),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::StarEqual)),
        None,
    ),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::SlashEqual)),
        None,
    ),
    (
        LexemeKind::Token(TokenKind::Symbol(Symbol::PercentEqual)),
        None,
    ),
];

#[test]
fn pratt_infix_rule_table_and_all_1225_pairs_hold_exhaustively() {
    assert_eq!(ALL_INFIX_OPERATORS.len(), 35);
    let rules: Vec<InfixRule> = ALL_INFIX_OPERATORS
        .iter()
        .map(|&(kind, ident)| {
            let soft = ident.and_then(SoftInfixKeyword::parse);
            infix_rule_from_parts(kind, soft).expect("operator must have InfixRule")
        })
        .collect();

    for &rule in &rules {
        assert!((PREC_ASSIGNMENT..=PREC_CAST).contains(&rule.precedence));
        assert!(rule.precedence < PREC_PREFIX);
        let right_assoc = matches!(
            rule.kind,
            InfixKind::Binary(BinaryOperator::Elvis) | InfixKind::Assignment(_)
        );
        if right_assoc {
            assert_eq!(rule.right_precedence, rule.precedence);
            assert!(rule.non_associative_group.is_none());
        } else {
            assert_eq!(rule.right_precedence, rule.precedence + 1);
        }
        match rule.non_associative_group {
            Some(NonAssociativeGroup::Range) => assert_eq!(rule.precedence, PREC_RANGE),
            Some(NonAssociativeGroup::Membership) => assert_eq!(rule.precedence, PREC_MEMBERSHIP),
            Some(NonAssociativeGroup::Comparison) => assert_eq!(rule.precedence, PREC_COMPARISON),
            Some(NonAssociativeGroup::Equality) => assert_eq!(rule.precedence, PREC_EQUALITY),
            None => assert!(
                ![PREC_RANGE, PREC_MEMBERSHIP, PREC_COMPARISON, PREC_EQUALITY]
                    .contains(&rule.precedence)
            ),
        }
    }

    // Exhaustively check all 35 x 35 = 1,225 ordered pairs.
    for &r1 in &rules {
        for &r2 in &rules {
            let mut seen = [false; 4];
            if let Some(g1) = r1.non_associative_group {
                seen[g1 as usize] = true;
            }
            let binds_right = r2.precedence >= r1.right_precedence;
            let chain_err =
                !binds_right && r2.non_associative_group.is_some_and(|g2| seen[g2 as usize]);

            if r1.precedence < r2.precedence {
                assert!(binds_right && !chain_err);
            } else if r1.precedence > r2.precedence {
                assert!(!binds_right && !chain_err);
            } else if r1.non_associative_group.is_some() {
                assert!(!binds_right && chain_err);
                assert_eq!(r1.non_associative_group, r2.non_associative_group);
            } else if r1.right_precedence == r1.precedence {
                assert!(binds_right && !chain_err);
            } else {
                assert!(!binds_right && !chain_err);
            }
        }
    }
}
