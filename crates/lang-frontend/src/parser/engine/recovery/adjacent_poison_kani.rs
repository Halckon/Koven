//! SPEC-0499 (F6 fourth slice, Kani track): Direct-Rust bounded symbolic verification
//! harness for `ElsePoisonTail` memoization soundness on `scan_poison_before_else_core`.
//!
//! Evidence level: `validated (bounded Rust, len = 6 lexemes, arbitrary start/cursor)`.

use super::adjacent_poison::{ElsePoisonScan, scan_poison_before_else_core};
use crate::lexer::{InvalidKind, Keyword, LexemeKind, Symbol, TokenKind, TriviaKind};

const N: usize = 6;

fn symbolic_kind() -> LexemeKind {
    let tag: u8 = kani::any();
    kani::assume(tag < 6);
    match tag {
        0 => LexemeKind::Trivia(TriviaKind::Whitespace),
        1 => LexemeKind::Invalid(InvalidKind::UnexpectedCharacter),
        2 => LexemeKind::Token(TokenKind::Keyword(Keyword::Else)),
        3 => LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)),
        4 => LexemeKind::Token(TokenKind::Identifier),
        _ => LexemeKind::Eof,
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn verify_else_poison_cache_soundness_len_6() {
    let mut kinds = [LexemeKind::Eof; N];
    let mut i = 0usize;
    while i < N {
        kinds[i] = symbolic_kind();
        i += 1;
    }

    let start: usize = kani::any();
    kani::assume(start < N);

    match scan_poison_before_else_core(N, start, |idx| kinds[idx]) {
        ElsePoisonScan::FoundElse {
            boundary,
            last_poison,
        } => {
            assert!(start <= last_poison && last_poison < boundary && boundary < N);
            assert_eq!(
                kinds[boundary],
                LexemeKind::Token(TokenKind::Keyword(Keyword::Else))
            );
        }
        ElsePoisonScan::Rejected { boundary } => {
            assert!(start <= boundary && boundary < N);
            // Universally quantified over every cursor `c in start..=boundary`:
            // a fresh scan from `c` must also reject at the exact same `boundary`.
            let c: usize = kani::any();
            kani::assume(start <= c && c <= boundary);
            assert_eq!(
                scan_poison_before_else_core(N, c, |idx| kinds[idx]),
                ElsePoisonScan::Rejected { boundary }
            );
        }
        ElsePoisonScan::Exhausted => {}
    }
}
