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
