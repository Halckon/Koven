//! Exhaustive Rust unit tests mirroring SPEC-0499 (F6 fourth slice) E1–E3
//! on production `scan_poison_before_else_core`.

use super::adjacent_poison::{ElsePoisonScan, scan_poison_before_else_core};
use crate::lexer::{InvalidKind, Keyword, LexemeKind, Symbol, TokenKind, TriviaKind};

const KINDS: [LexemeKind; 5] = [
    LexemeKind::Trivia(TriviaKind::Whitespace),
    LexemeKind::Invalid(InvalidKind::UnexpectedCharacter),
    LexemeKind::Token(TokenKind::Keyword(Keyword::Else)),
    LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)),
    LexemeKind::Token(TokenKind::Identifier),
];

#[test]
fn else_poison_cache_soundness_holds_exhaustively_up_to_len_6() {
    let mut seq = Vec::with_capacity(6);
    let mut checked = 0usize;
    for len in 1..=6 {
        let total = 5usize.pow(len as u32);
        for mut code in 0..total {
            seq.clear();
            for _ in 0..len {
                seq.push(KINDS[code % 5]);
                code /= 5;
            }
            for start in 0..len {
                match scan_poison_before_else_core(len, start, |idx| seq[idx]) {
                    ElsePoisonScan::FoundElse {
                        boundary,
                        last_poison,
                    } => {
                        assert!(start <= last_poison && last_poison < boundary && boundary < len);
                        assert_eq!(
                            seq[boundary],
                            LexemeKind::Token(TokenKind::Keyword(Keyword::Else))
                        );
                    }
                    ElsePoisonScan::Rejected { boundary } => {
                        assert!(start <= boundary && boundary < len);
                        for c in start..=boundary {
                            assert_eq!(
                                scan_poison_before_else_core(len, c, |idx| seq[idx]),
                                ElsePoisonScan::Rejected { boundary }
                            );
                        }
                    }
                    ElsePoisonScan::Exhausted => {}
                }
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 5 + 25 + 125 + 625 + 3125 + 15625);
}
