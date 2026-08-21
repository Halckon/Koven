//! Parser poison-insertion matrices shared lexical-mode gap enumeration.

use lang_frontend::lexer::TokenKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LexicalMode {
    Code,
    String,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Gap {
    pub(crate) offset: usize,
    pub(crate) code_mode: bool,
}

pub(crate) fn token_gaps(slots: impl IntoIterator<Item = (TokenKind, usize)>) -> Vec<Gap> {
    let mut modes = vec![LexicalMode::Code];
    let mut gaps = vec![Gap {
        offset: 0,
        code_mode: true,
    }];

    for (kind, end) in slots {
        match kind {
            TokenKind::StringStart => {
                assert_eq!(modes.last(), Some(&LexicalMode::Code));
                modes.push(LexicalMode::String);
            }
            TokenKind::InterpolationStart => {
                assert_eq!(modes.last(), Some(&LexicalMode::String));
                modes.push(LexicalMode::Code);
            }
            TokenKind::InterpolationEnd => {
                assert_eq!(modes.pop(), Some(LexicalMode::Code));
                assert_eq!(modes.last(), Some(&LexicalMode::String));
            }
            TokenKind::StringEnd => {
                assert_eq!(modes.pop(), Some(LexicalMode::String));
            }
            _ => {}
        }
        gaps.push(Gap {
            offset: end,
            code_mode: modes.last() == Some(&LexicalMode::Code),
        });
    }

    assert_eq!(modes, [LexicalMode::Code]);
    gaps
}
