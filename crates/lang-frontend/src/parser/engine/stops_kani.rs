//! SPEC-0502 (F6 sixth slice, Kani track): Direct-Rust symbolic verification
//! harnesses for `Stops`, `TypeStops`, and `DeclarationStops` boundary propagation
//! and hard-delimiter non-shielding in `boundary.rs`.
//!
//! Evidence level: `validated (complete bit-vector symbolic Rust over all 2^17 Stops states)`.

use super::boundary::{DeclarationStops, Stops, TypeStops};
use super::class::ClassifierHeaderOwner;
use crate::lexer::{Keyword, Lexeme, LexemeKind, Symbol, TokenKind};

fn symbolic_token_lexeme() -> (Lexeme, Option<Symbol>) {
    let tag: u8 = kani::any();
    kani::assume(tag < 11);
    let (kind, sym) = match tag {
        0 => (LexemeKind::Eof, None),
        1 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)),
            Some(Symbol::RightParen),
        ),
        2 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)),
            Some(Symbol::RightBracket),
        ),
        3 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)),
            Some(Symbol::RightBrace),
        ),
        4 => (LexemeKind::Token(TokenKind::InterpolationEnd), None),
        5 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)),
            Some(Symbol::Comma),
        ),
        6 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)),
            Some(Symbol::Semicolon),
        ),
        7 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow)),
            Some(Symbol::Arrow),
        ),
        8 => (LexemeKind::Token(TokenKind::Keyword(Keyword::Else)), None),
        9 => (
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)),
            Some(Symbol::LeftBrace),
        ),
        _ => (LexemeKind::Token(TokenKind::Keyword(Keyword::Val)), None),
    };
    (Lexeme::synthetic_for_verification(kind), sym)
}

#[kani::proof]
fn verify_stops_hard_delimiter_non_shielding() {
    let delimiters: u16 = kani::any();
    let block_elements: bool = kani::any();
    let stops = Stops {
        delimiters,
        block_elements,
        when_entry_body: None,
    };
    let (lexeme, symbol) = symbolic_token_lexeme();

    // S1: Every hard stop is also a stop.
    if stops.contains_hard(lexeme) {
        assert!(stops.contains(lexeme));
    }

    // S2 & S3: Entering block or lambda expression never shields any outer hard stop.
    let block_stops = Stops::block_expression(stops);
    let lambda_stops = Stops::lambda_expression(stops);
    if stops.contains_hard(lexeme) {
        assert!(block_stops.contains_hard(lexeme));
        assert!(lambda_stops.contains_hard(lexeme));
    }
    let right_brace = Lexeme::synthetic_for_verification(LexemeKind::Token(TokenKind::Symbol(
        Symbol::RightBrace,
    )));
    let arrow =
        Lexeme::synthetic_for_verification(LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow)));
    assert!(block_stops.contains(right_brace));
    assert!(lambda_stops.contains(right_brace));
    assert!(lambda_stops.contains(arrow));

    // S4: Soft-stop stripping preserves all hard stops identically.
    assert_eq!(
        stops.without_lambda_body_soft_stops().contains_hard(lexeme),
        stops.contains_hard(lexeme)
    );
    assert_eq!(
        stops
            .without_file_and_semicolon_stops()
            .contains_hard(lexeme),
        stops.contains_hard(lexeme)
    );

    // S5: TypeStops::from_expression preserves all hard stops.
    let type_stops = TypeStops::from_expression(stops);
    if stops.contains_hard(lexeme) {
        assert!(type_stops.contains(lexeme));
    }

    // S6: DeclarationStops::from_expression_hard <= from_expression and both preserve non-EOF hard stops.
    let decl_hard = DeclarationStops::from_expression_hard(stops);
    let decl_full = DeclarationStops::from_expression(stops);
    assert_eq!(decl_hard.0 & decl_full.0, decl_hard.0);
    if !matches!(lexeme.kind(), LexemeKind::Eof) && stops.contains_hard(lexeme) {
        assert!(decl_hard.contains_hard(lexeme, symbol));
        assert!(decl_full.contains_hard(lexeme, symbol));
    }
}

#[kani::proof]
fn verify_type_stops_recovery_projections() {
    let flags: u16 = kani::any();
    let type_stops = TypeStops::empty().with(flags);

    // S8: for_proven_function_parameters clears outer delimiters while keeping inner hard delimiters.
    let proven = type_stops.for_proven_function_parameters(42);
    assert_eq!(proven.proven_type_closer(), Some(42));
    for sym in [
        Symbol::Greater,
        Symbol::RightBracket,
        Symbol::Arrow,
        Symbol::RightBrace,
    ] {
        let tok = Lexeme::synthetic_for_verification(LexemeKind::Token(TokenKind::Symbol(sym)));
        assert!(!proven.contains(tok));
    }
    let right_paren = Lexeme::synthetic_for_verification(LexemeKind::Token(TokenKind::Symbol(
        Symbol::RightParen,
    )));
    let interp_end =
        Lexeme::synthetic_for_verification(LexemeKind::Token(TokenKind::InterpolationEnd));
    assert_eq!(
        proven.contains(right_paren),
        type_stops.contains(right_paren)
    );
    assert_eq!(proven.contains(interp_end), type_stops.contains(interp_end));

    // S7: supertype_element_recovery preserves hard delimiters and injects owner/supertype flags.
    let owner_tag: u8 = kani::any();
    kani::assume(owner_tag < 3);
    let owner = match owner_tag {
        0 => ClassifierHeaderOwner::File,
        1 => ClassifierHeaderOwner::Member,
        _ => ClassifierHeaderOwner::Declaration,
    };
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
    for (kind, sym) in [
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
    ] {
        let lex = Lexeme::synthetic_for_verification(kind);
        assert_eq!(rec.contains_hard(lex, sym), type_stops.contains(lex));
    }
}
