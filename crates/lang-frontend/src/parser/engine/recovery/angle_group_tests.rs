use super::*;
use crate::lexer::lex;

#[test]
fn angle_group_index_agrees_with_a_fresh_forward_owner_for_each_opener() {
    const DELIMITERS: [char; 6] = ['(', ')', '[', ']', '{', '}'];
    for mut case in 0..DELIMITERS.len().pow(5) {
        let mut text = String::new();
        for _ in 0..5 {
            text.push(DELIMITERS[case % DELIMITERS.len()]);
            text.push(' ');
            case /= DELIMITERS.len();
        }
        let mut sources = SourceMap::new();
        let id = sources.add_source("groups.ko", &text).unwrap();
        let lexed = lex(&sources, id).unwrap();
        let indexed = angle_group_closers(&lexed, &[]);
        for (opener, lexeme) in lexed.lexemes().iter().enumerate() {
            if !matches!(
                lexeme.kind(),
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::LeftParen | Symbol::LeftBracket | Symbol::LeftBrace
                ))
            ) {
                continue;
            }
            let mut owners = GroupOwners::default();
            let expected = lexed
                .lexemes()
                .iter()
                .copied()
                .enumerate()
                .skip(opener)
                .find_map(|(raw, token)| {
                    (owners.visit(raw, token, &[]) == Some(opener)).then_some(raw)
                });
            assert_eq!(
                indexed[opener].map(NonZeroUsize::get),
                expected,
                "{text:?} opener {opener}"
            );
        }
    }
}

#[test]
fn match_angle_group_delimiters_agrees_with_ordinary_groups_up_to_len_6() {
    const SYMBOLS: [Symbol; 6] = [
        Symbol::LeftParen,
        Symbol::RightParen,
        Symbol::LeftBracket,
        Symbol::RightBracket,
        Symbol::LeftBrace,
        Symbol::RightBrace,
    ];
    let mut seq = Vec::with_capacity(6);
    for len in 0..=6usize {
        let total = 6usize.pow(len as u32);
        for mut case in 0..total {
            seq.clear();
            for i in 0..len {
                let sym = SYMBOLS[case % 6];
                case /= 6;
                let (kind, opening) = group_delimiter(sym).unwrap();
                seq.push((i, kind, opening));
            }
            let indexed = angle_groups::match_angle_group_delimiters(&seq, len);
            for (i, &(raw_i, _, opening)) in seq.iter().enumerate() {
                if !opening {
                    assert_eq!(indexed[raw_i], None);
                    continue;
                }
                let mut groups = OrdinaryGroups::default();
                let expected = seq[i..].iter().find_map(|&(raw_j, k, op)| {
                    let sym = SYMBOLS[k * 2 + usize::from(!op)];
                    (groups.visit(raw_j, sym) == Some(raw_i)).then_some(raw_j)
                });
                let got = indexed[raw_i].map(NonZeroUsize::get);
                assert_eq!(got, expected, "len={len} seq={seq:?} opener={i}");
                if let Some(c) = got {
                    assert!(c > raw_i && c < len && c >= 1);
                }
            }
        }
    }
}
