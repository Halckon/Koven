//! SPEC-0500 (F6 fifth slice, Kani track): Direct-Rust bounded symbolic verification
//! harnesses for Koven's Pratt infix precedence table (`infix_rule_from_parts`),
//! associativity grouping, and non-associative chain slot invariants.
//!
//! Evidence level: `validated (bounded Rust, all 33 infix operators + non-operator tokens)`.

use super::operator::{SoftInfixKeyword, infix_rule_from_parts};
use super::{
    InfixKind, InfixRule, NonAssociativeGroup, PREC_ASSIGNMENT, PREC_CAST, PREC_COMPARISON,
    PREC_ELVIS, PREC_EQUALITY, PREC_MEMBERSHIP, PREC_PREFIX, PREC_RANGE,
};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind};
use crate::parser::BinaryOperator;

fn symbolic_infix_input(tag: u8) -> (LexemeKind, Option<SoftInfixKeyword>) {
    match tag {
        0 => (LexemeKind::Token(TokenKind::Symbol(Symbol::Star)), None),
        1 => (LexemeKind::Token(TokenKind::Symbol(Symbol::Slash)), None),
        2 => (LexemeKind::Token(TokenKind::Symbol(Symbol::Percent)), None),
        3 => (LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)), None),
        4 => (LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)), None),
        5 => (LexemeKind::Token(TokenKind::Symbol(Symbol::DotDot)), None),
        6 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::DotDotLess)),
            None,
        ),
        7 => (
            LexemeKind::Token(TokenKind::Identifier),
            Some(SoftInfixKeyword::To),
        ),
        8 => (
            LexemeKind::Token(TokenKind::Identifier),
            Some(SoftInfixKeyword::Shl),
        ),
        9 => (
            LexemeKind::Token(TokenKind::Identifier),
            Some(SoftInfixKeyword::Shr),
        ),
        10 => (
            LexemeKind::Token(TokenKind::Identifier),
            Some(SoftInfixKeyword::Ushr),
        ),
        11 => (
            LexemeKind::Token(TokenKind::Identifier),
            Some(SoftInfixKeyword::And),
        ),
        12 => (
            LexemeKind::Token(TokenKind::Identifier),
            Some(SoftInfixKeyword::Xor),
        ),
        13 => (
            LexemeKind::Token(TokenKind::Identifier),
            Some(SoftInfixKeyword::Or),
        ),
        14 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::QuestionColon)),
            None,
        ),
        15 => (LexemeKind::Token(TokenKind::Keyword(Keyword::In)), None),
        16 => (LexemeKind::Token(TokenKind::Symbol(Symbol::BangIn)), None),
        17 => (LexemeKind::Token(TokenKind::Keyword(Keyword::Is)), None),
        18 => (LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs)), None),
        19 => (LexemeKind::Token(TokenKind::Symbol(Symbol::Less)), None),
        20 => (LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)), None),
        21 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::LessEqual)),
            None,
        ),
        22 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::GreaterEqual)),
            None,
        ),
        23 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::EqualEqual)),
            None,
        ),
        24 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangEqual)),
            None,
        ),
        25 => (LexemeKind::Token(TokenKind::Symbol(Symbol::AndAnd)), None),
        26 => (LexemeKind::Token(TokenKind::Symbol(Symbol::OrOr)), None),
        27 => (LexemeKind::Token(TokenKind::Keyword(Keyword::As)), None),
        28 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::AsQuestion)),
            None,
        ),
        29 => (LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)), None),
        30 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::PlusEqual)),
            None,
        ),
        31 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::MinusEqual)),
            None,
        ),
        32 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::StarEqual)),
            None,
        ),
        33 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::SlashEqual)),
            None,
        ),
        34 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::PercentEqual)),
            None,
        ),
        // Non-infix tokens
        35 => (LexemeKind::Token(TokenKind::Identifier), None),
        36 => (LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)), None),
        _ => (LexemeKind::Eof, None),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Grouping2 {
    Left,
    Right,
    ChainError,
}

fn pratt_group_2_ops(r1: InfixRule, r2: InfixRule) -> Grouping2 {
    // Mirrors `parse_expression_bp_inner` starting at `min_bp = 0` on `a op1 b op2 c`.
    let mut seen = [false; 4];
    if let Some(g1) = r1.non_associative_group {
        seen[g1 as usize] = true;
    }
    // Inner recursive call `parse_expression_bp(r1.right_precedence)` inspects `r2`:
    if r2.precedence >= r1.right_precedence {
        Grouping2::Right
    } else if let Some(g2) = r2.non_associative_group
        && seen[g2 as usize]
    {
        Grouping2::ChainError
    } else {
        Grouping2::Left
    }
}

#[kani::proof]
fn verify_infix_rule_table_invariants() {
    let tag: u8 = kani::any();
    kani::assume(tag < 38);
    let (kind, ident) = symbolic_infix_input(tag);
    let rule = infix_rule_from_parts(kind, ident);

    if tag < 35 {
        let rule = rule.expect("every Koven infix operator must map to an InfixRule");
        // Precedence bounds: strictly within 1..=16 (below PREC_PREFIX = 17).
        assert!(rule.precedence >= PREC_ASSIGNMENT);
        assert!(rule.precedence <= PREC_CAST);
        assert!(rule.precedence < PREC_PREFIX);

        // Right-associativity iff Elvis or Assignment.
        let is_right_assoc = matches!(
            rule.kind,
            InfixKind::Binary(BinaryOperator::Elvis) | InfixKind::Assignment(_)
        );
        if is_right_assoc {
            assert_eq!(rule.right_precedence, rule.precedence);
            assert!(rule.non_associative_group.is_none());
        } else {
            assert_eq!(rule.right_precedence, rule.precedence + 1);
        }

        // Exact bijection between NonAssociativeGroup and its precedence level, and slot < 4.
        match rule.non_associative_group {
            Some(NonAssociativeGroup::Range) => {
                assert_eq!(rule.precedence, PREC_RANGE);
                assert!((NonAssociativeGroup::Range as usize) < 4);
            }
            Some(NonAssociativeGroup::Membership) => {
                assert_eq!(rule.precedence, PREC_MEMBERSHIP);
                assert!((NonAssociativeGroup::Membership as usize) < 4);
            }
            Some(NonAssociativeGroup::Comparison) => {
                assert_eq!(rule.precedence, PREC_COMPARISON);
                assert!((NonAssociativeGroup::Comparison as usize) < 4);
            }
            Some(NonAssociativeGroup::Equality) => {
                assert_eq!(rule.precedence, PREC_EQUALITY);
                assert!((NonAssociativeGroup::Equality as usize) < 4);
            }
            None => {
                assert!(
                    rule.precedence != PREC_RANGE
                        && rule.precedence != PREC_MEMBERSHIP
                        && rule.precedence != PREC_COMPARISON
                        && rule.precedence != PREC_EQUALITY
                );
            }
        }
        if matches!(rule.kind, InfixKind::Binary(BinaryOperator::Elvis)) {
            assert_eq!(rule.precedence, PREC_ELVIS);
        }
    } else {
        assert!(rule.is_none());
    }
}

#[kani::proof]
fn verify_pratt_precedence_and_non_assoc_3_ops() {
    let tag1: u8 = kani::any();
    let tag2: u8 = kani::any();
    kani::assume(tag1 < 35 && tag2 < 35);
    let (k1, id1) = symbolic_infix_input(tag1);
    let (k2, id2) = symbolic_infix_input(tag2);
    let r1 = infix_rule_from_parts(k1, id1).unwrap();
    let r2 = infix_rule_from_parts(k2, id2).unwrap();

    let grouping = pratt_group_2_ops(r1, r2);
    if r1.precedence < r2.precedence {
        assert_eq!(grouping, Grouping2::Right);
    } else if r1.precedence > r2.precedence {
        assert_eq!(grouping, Grouping2::Left);
    } else if r1.non_associative_group.is_some() {
        assert_eq!(r1.non_associative_group, r2.non_associative_group);
        assert_eq!(grouping, Grouping2::ChainError);
    } else if r1.right_precedence == r1.precedence {
        assert_eq!(grouping, Grouping2::Right);
    } else {
        assert_eq!(grouping, Grouping2::Left);
    }
}
