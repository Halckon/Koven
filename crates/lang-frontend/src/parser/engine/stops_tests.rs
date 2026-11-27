use super::boundary::{DeclarationStops, Stops, TypeStops};
use super::class::ClassifierHeaderOwner;
use crate::lexer::{Keyword, Lexeme, LexemeKind, Symbol, TokenKind};

#[test]
fn stops_lattice_hard_delimiter_non_shielding_holds_exhaustively() {
    let tokens = [
        (LexemeKind::Eof, None),
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)),
            Some(Symbol::RightParen),
        ),
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)),
            Some(Symbol::RightBracket),
        ),
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)),
            Some(Symbol::RightBrace),
        ),
        (LexemeKind::Token(TokenKind::InterpolationEnd), None),
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)),
            Some(Symbol::Comma),
        ),
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)),
            Some(Symbol::Semicolon),
        ),
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow)),
            Some(Symbol::Arrow),
        ),
        (LexemeKind::Token(TokenKind::Keyword(Keyword::Else)), None),
    ];

    for delimiters in 0u16..(Stops::WHEN_CALL_OPERAND_RECOVERY << 1) {
        for block_elements in [false, true] {
            let stops = Stops {
                delimiters,
                block_elements,
                when_entry_body: None,
            };
            let block_stops = Stops::block_expression(stops);
            let lambda_stops = Stops::lambda_expression(stops);
            let no_lambda_soft = stops.without_lambda_body_soft_stops();
            let no_file_semi = stops.without_file_and_semicolon_stops();
            let type_stops = TypeStops::from_expression(stops);
            let decl_hard = DeclarationStops::from_expression_hard(stops);
            let decl_full = DeclarationStops::from_expression(stops);

            assert_eq!(decl_hard.0 & decl_full.0, decl_hard.0);

            for &(kind, symbol) in &tokens {
                let tok = Lexeme::synthetic_for_verification(kind);
                let is_hard = stops.contains_hard(tok);
                if is_hard {
                    assert!(stops.contains(tok));
                    assert!(block_stops.contains_hard(tok));
                    assert!(lambda_stops.contains_hard(tok));
                    assert!(type_stops.contains(tok));
                    if !matches!(kind, LexemeKind::Eof) {
                        assert!(decl_hard.contains_hard(tok, symbol));
                        assert!(decl_full.contains_hard(tok, symbol));
                    }
                }
                assert_eq!(no_lambda_soft.contains_hard(tok), is_hard);
                assert_eq!(no_file_semi.contains_hard(tok), is_hard);
            }

            for owner in [
                ClassifierHeaderOwner::File,
                ClassifierHeaderOwner::Member,
                ClassifierHeaderOwner::Declaration,
            ] {
                let rec = type_stops.supertype_element_recovery(owner);
                assert!(rec.has_supertype_suffix());
                assert!(rec.recovers_type_element());
                assert!(rec.has_semicolon());
                assert_eq!(
                    rec.has_class_file_boundary(),
                    matches!(owner, ClassifierHeaderOwner::File)
                );
                assert_eq!(
                    rec.stops_at_class_member(),
                    matches!(owner, ClassifierHeaderOwner::Member)
                );
            }
        }
    }
}
