use super::type_ref::{
    bounded_proven_parameter_closer, is_extension_receiver_dot_stop,
    is_function_type_right_paren_closer, is_function_type_start_kinds,
    run_segmented_string_type_poison_end,
};
use crate::lexer::{LexemeKind, Symbol, TokenKind};

#[test]
fn function_type_start_and_extension_dot_stop_match_expected_shapes() {
    let ident = LexemeKind::Token(TokenKind::Identifier);
    let lparen = LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen));
    assert!(is_function_type_start_kinds(lparen, false, None));
    assert!(is_function_type_start_kinds(ident, true, Some(lparen)));
    assert!(!is_function_type_start_kinds(ident, false, Some(lparen)));
    assert!(is_extension_receiver_dot_stop(
        true,
        Some(ident),
        Some(lparen)
    ));
    assert!(!is_extension_receiver_dot_stop(
        false,
        Some(ident),
        Some(lparen)
    ));
}

#[test]
fn proven_closer_guards_prevent_stealing_outer_parenthesis() {
    let rparen = LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen));
    assert!(is_function_type_right_paren_closer(rparen, 5, Some(10)));
    assert!(!is_function_type_right_paren_closer(rparen, 10, Some(10)));
    assert!(is_function_type_right_paren_closer(rparen, 10, None));
    assert_eq!(bounded_proven_parameter_closer(Some(5), Some(10)), Some(5));
    assert_eq!(bounded_proven_parameter_closer(Some(10), Some(10)), None);
    assert_eq!(bounded_proven_parameter_closer(Some(10), None), Some(10));
}

#[test]
fn segmented_string_type_poison_stops_at_recovery_end_or_eof() {
    let ident = LexemeKind::Token(TokenKind::Identifier);
    assert_eq!(
        run_segmented_string_type_poison_end(2, 10, &[(ident, 6), (ident, 10), (ident, 20)]),
        10
    );
    assert_eq!(
        run_segmented_string_type_poison_end(
            2,
            10,
            &[(ident, 6), (LexemeKind::Eof, 6), (ident, 12)]
        ),
        6
    );
}
