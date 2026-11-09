//! Ordinary delimiter pairing, isolated at every interpolation boundary.

use super::{LexedFile, LexemeKind, NonZeroUsize, Symbol, TerminalOwnerEvent, TokenKind};

#[derive(Default)]
pub(super) struct OrdinaryGroups {
    groups: Vec<(usize, usize)>,
    active: [usize; 3],
}

impl OrdinaryGroups {
    pub(super) fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub(super) fn visit(&mut self, raw: usize, symbol: Symbol) -> Option<usize> {
        let (kind, opening) = group_delimiter(symbol)?;
        if opening {
            self.groups.push((raw, kind));
            self.active[kind] += 1;
        } else if symbol == Symbol::RightBrace && self.active[kind] == 0 {
            // An unmatched right brace belongs to the enclosing caller.
            self.groups.clear();
            self.active.fill(0);
        } else if self.active[kind] != 0 {
            // 孤立的异形 closer 不会使另一个已打开的分组丢失其真实 closer。
            while let Some((opener, owner_kind)) = self.groups.pop() {
                self.active[owner_kind] -= 1;
                if owner_kind == kind {
                    return Some(opener);
                }
            }
        }
        None
    }
}

pub(super) fn group_delimiter(symbol: Symbol) -> Option<(usize, bool)> {
    Some(match symbol {
        Symbol::LeftParen => (0, true),
        Symbol::RightParen => (0, false),
        Symbol::LeftBracket => (1, true),
        Symbol::RightBracket => (1, false),
        Symbol::LeftBrace => (2, true),
        Symbol::RightBrace => (2, false),
        _ => return None,
    })
}

pub(super) fn group_closers(
    lexed: &LexedFile,
    terminals: &[TerminalOwnerEvent],
) -> Vec<Option<NonZeroUsize>> {
    let mut matched = vec![None; lexed.lexemes().len()];
    // Strings carry no ordinary delimiters. Each interpolation starts a
    // separate expression scope, which cannot close a surrounding group.
    let mut scopes = vec![Some(OrdinaryGroups::default())];
    let mut terminals = terminals.iter().peekable();
    for (raw, lexeme) in lexed.lexemes().iter().copied().enumerate() {
        while terminals
            .peek()
            .is_some_and(|event| event.offset <= lexeme.span().start())
        {
            terminals.next();
            scopes.pop();
        }
        match lexeme.kind() {
            LexemeKind::Token(TokenKind::StringStart) => scopes.push(None),
            LexemeKind::Token(TokenKind::InterpolationStart) => {
                scopes.push(Some(OrdinaryGroups::default()))
            }
            LexemeKind::Token(TokenKind::StringEnd | TokenKind::InterpolationEnd) => {
                scopes.pop();
            }
            LexemeKind::Token(TokenKind::Symbol(symbol)) => {
                if let Some(Some(scope)) = scopes.last_mut()
                    && let Some(opener) = scope.visit(raw, symbol)
                {
                    matched[opener] = NonZeroUsize::new(raw);
                }
            }
            _ => {}
        }
    }
    matched
}
