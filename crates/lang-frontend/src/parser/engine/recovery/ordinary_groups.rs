//! Ordinary delimiter pairing, isolated at every interpolation boundary.

use super::{
    Keyword, LexedFile, Lexeme, LexemeKind, LexicalRecoveryIndex, NonZeroUsize, Parser,
    ParserInternalError, Stops, Symbol, TerminalOwnerEvent, TokenKind,
    unsupported_block_element_kind,
};

struct GroupClose {
    closer: usize,
    enclosing: Option<NonZeroUsize>,
    boundaries: RecoveryBoundaries,
}

#[derive(Clone, Copy, Default)]
struct RecoveryBoundaries {
    local: Option<NonZeroUsize>,
    caller_else: Option<NonZeroUsize>,
    caller_block: Option<NonZeroUsize>,
    block_statement_separator: Option<NonZeroUsize>,
}

impl RecoveryBoundaries {
    fn include_unclosed(&mut self, child: Self) {
        self.local = self.local.max(child.local);
        self.caller_else = self.caller_else.max(child.caller_else);
        self.caller_block = self.caller_block.max(child.caller_block);
        self.block_statement_separator = self
            .block_statement_separator
            .max(child.block_statement_separator);
    }
}

pub(super) struct GroupClosers {
    by_opener: Vec<Option<NonZeroUsize>>,
    pairs: Vec<GroupClose>,
}

impl GroupClosers {
    fn pair(&self, opener: usize) -> Option<&GroupClose> {
        self.pairs
            .get(self.by_opener.get(opener)?.as_ref()?.get() - 1)
    }

    fn closer(&self, opener: usize) -> Option<usize> {
        Some(self.pair(opener)?.closer)
    }

    fn confirmed_closer(&self, opener: usize, cursor: usize, outer_stops: Stops) -> Option<usize> {
        let close = self.pair(opener)?;
        // A sole right parenthesis may be the caller's intended closer.
        if let Some(enclosing) = close.enclosing {
            self.closer(enclosing.get() - 1)?;
        }
        // Consider only caller boundaries the recovery has not reached yet.
        let at_or_after_cursor =
            |raw: Option<NonZeroUsize>| raw.is_some_and(|raw| raw.get() >= cursor);
        let boundaries = close.boundaries;
        // Lexical pairing alone cannot assign an Else or block statement to an unparsed group.
        if at_or_after_cursor(boundaries.local)
            || (outer_stops.delimiters & Stops::ELSE != 0
                && at_or_after_cursor(boundaries.caller_else))
            || (outer_stops.block_elements && at_or_after_cursor(boundaries.caller_block))
            || (outer_stops.delimiters & Stops::BLOCK_STATEMENT_SEPARATOR != 0
                && at_or_after_cursor(boundaries.block_statement_separator))
        {
            return None;
        }
        Some(close.closer)
    }
}

impl LexicalRecoveryIndex<'_> {
    fn ordinary_groups(&self, lexed: &LexedFile) -> &GroupClosers {
        self.group_closers
            .get_or_init(|| group_closers(self.source, lexed, &self.terminal_owner_events))
    }

    pub(in crate::parser::engine) fn group_closer(
        &self,
        lexed: &LexedFile,
        opener: usize,
    ) -> Option<usize> {
        self.ordinary_groups(lexed).closer(opener)
    }

    pub(in crate::parser::engine) fn block_statement_follows(
        &self,
        lexed: &LexedFile,
        separator: usize,
    ) -> bool {
        if let Some((previous, starts_statement)) = self.block_statement_after_separator.get()
            && previous == separator
        {
            return starts_statement;
        }
        // Nested expressions can all unwind at the same caller separator.
        let starts_statement =
            matches!(
                lexed.lexemes()[separator].kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
            ) && starts_block_statement(self.source, &lexed.lexemes()[separator + 1..]);
        self.block_statement_after_separator
            .set(Some((separator, starts_statement)));
        starts_statement
    }

    pub(in crate::parser::engine) fn postfix_recovery_closer(
        &self,
        lexed: &LexedFile,
        opener: usize,
        cursor: usize,
        preserve_block_semicolon: bool,
    ) -> Option<usize> {
        let pair = self.ordinary_groups(lexed).pair(opener)?;
        let crosses_separator = preserve_block_semicolon
            && pair
                .boundaries
                .block_statement_separator
                .is_some_and(|semicolon| semicolon.get() >= cursor)
            && !matches!(
                lexed.lexemes()[pair.closer].kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace))
            );
        (!crosses_separator).then_some(pair.closer)
    }
}

impl Parser<'_> {
    /// Leave a proven closer in place for its group to consume.
    pub(in crate::parser::engine) fn skip_to_group_closer(
        &mut self,
        opener: usize,
        preserve_block_separator: bool,
    ) -> Result<Option<usize>, ParserInternalError> {
        let closer = if preserve_block_separator {
            self.lexical_recoveries.postfix_recovery_closer(
                self.lexed,
                opener,
                self.current_raw()?,
                true,
            )
        } else {
            self.lexical_recoveries.group_closer(self.lexed, opener)
        };
        self.skip_before_closer(closer)
    }

    pub(in crate::parser::engine) fn skip_to_confirmed_group_closer(
        &mut self,
        opener: usize,
        outer_stops: Stops,
    ) -> Result<Option<usize>, ParserInternalError> {
        let closer = self
            .lexical_recoveries
            .ordinary_groups(self.lexed)
            .confirmed_closer(opener, self.current_raw()?, outer_stops);
        self.skip_before_closer(closer)
    }

    pub(in crate::parser::engine) fn skip_before_closer(
        &mut self,
        closer: Option<usize>,
    ) -> Result<Option<usize>, ParserInternalError> {
        if let Some(closer) = closer
            && self.current_raw()? < closer
        {
            self.index = closer;
            return Ok(Some(self.previous_significant_end()));
        }
        Ok(None)
    }
}

#[derive(Default)]
pub(super) struct OrdinaryGroups {
    groups: Vec<OpenGroup>,
    active: [usize; 3],
}

struct OpenGroup {
    raw: usize,
    kind: usize,
    boundaries: RecoveryBoundaries,
}

impl OrdinaryGroups {
    pub(super) fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub(super) fn visit(&mut self, raw: usize, symbol: Symbol) -> Option<usize> {
        self.visit_group(raw, symbol).map(|group| group.raw)
    }

    fn visit_group(&mut self, raw: usize, symbol: Symbol) -> Option<OpenGroup> {
        let (kind, opening) = group_delimiter(symbol)?;
        if opening {
            self.groups.push(OpenGroup {
                raw,
                kind,
                boundaries: RecoveryBoundaries::default(),
            });
            self.active[kind] += 1;
        } else if symbol == Symbol::RightBrace && self.active[kind] == 0 {
            // An unmatched right brace belongs to the enclosing caller.
            self.groups.clear();
            self.active.fill(0);
        } else if self.active[kind] != 0 {
            // 孤立的异形 closer 不会使另一个已打开的分组丢失其真实 closer。
            let mut boundaries = RecoveryBoundaries::default();
            while let Some(mut group) = self.groups.pop() {
                self.active[group.kind] -= 1;
                boundaries.include_unclosed(group.boundaries);
                if group.kind == kind {
                    // Discarded, unclosed child groups cannot shield a caller boundary.
                    group.boundaries = boundaries;
                    return Some(group);
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

fn starts_block_statement(source: &str, tail: &[Lexeme]) -> bool {
    let mut tokens = tail
        .iter()
        .copied()
        .filter(|lexeme| !matches!(lexeme.kind(), LexemeKind::Trivia(_)));
    let Some(first) = tokens.next() else {
        return false;
    };
    let kind = first.kind();
    if matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
    ) || unsupported_block_element_kind(kind)
    {
        return true;
    }
    if kind != LexemeKind::Token(TokenKind::Identifier) {
        return false;
    }
    match source.get(first.span().start()..first.span().end()) {
        Some("borrow") => tokens.next().is_some_and(|second| {
            second.kind() == LexemeKind::Token(TokenKind::Keyword(Keyword::Val))
        }),
        Some("loop") => tokens.next().is_some_and(|second| {
            second.kind() == LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
        }),
        _ => false,
    }
}

fn group_closers(
    source: &str,
    lexed: &LexedFile,
    terminals: &[TerminalOwnerEvent],
) -> GroupClosers {
    let mut closers = GroupClosers {
        by_opener: vec![None; lexed.lexemes().len()],
        pairs: Vec::new(),
    };
    // Strings carry no ordinary delimiters. Each interpolation starts a
    // separate expression scope, which cannot close a surrounding group.
    let mut scopes = vec![Some(OrdinaryGroups::default())];
    let mut terminals = terminals.iter().peekable();
    // Commas can be the malformed group tail. Other structural caller stops block a jump.
    let block_stops = Stops::block_expression(Stops::ROOT).with(Stops::SEMICOLON);
    let boundaries = block_stops
        .with(Stops::FILE_DECLARATION | Stops::RIGHT_BRACKET | Stops::ARROW | Stops::ELSE);
    for (raw, lexeme) in lexed.lexemes().iter().copied().enumerate() {
        while terminals
            .peek()
            .is_some_and(|event| event.offset <= lexeme.span().start())
        {
            terminals.next();
            scopes.pop();
        }
        if boundaries.contains(lexeme)
            && let Some(Some(scope)) = scopes.last_mut()
            && let Some(group) = scope.groups.last_mut()
        {
            group.boundaries.local = NonZeroUsize::new(raw);
            // A semicolon in a malformed comma tail may still belong to that tail.
            // Check prefixes selected by block dispatch. The lookaheads never cross a semicolon.
            if matches!(
                lexeme.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
            ) && starts_block_statement(source, &lexed.lexemes()[raw + 1..])
            {
                group.boundaries.block_statement_separator = group.boundaries.local;
            }
            if matches!(
                lexeme.kind(),
                LexemeKind::Token(TokenKind::Keyword(Keyword::Else))
            ) {
                group.boundaries.caller_else = group.boundaries.local;
            }
            if block_stops.contains(lexeme)
                && !matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace))
                )
            {
                group.boundaries.caller_block = group.boundaries.local;
            }
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
                    && let Some(group) = scope.visit_group(raw, symbol)
                {
                    if let Some(parent) = scope.groups.last_mut() {
                        parent.boundaries.caller_else = parent
                            .boundaries
                            .caller_else
                            .max(group.boundaries.caller_else);
                        // A paired brace owns its own statements.
                        if symbol != Symbol::RightBrace {
                            parent.boundaries.caller_block = parent
                                .boundaries
                                .caller_block
                                .max(group.boundaries.caller_block);
                            parent.boundaries.block_statement_separator = parent
                                .boundaries
                                .block_statement_separator
                                .max(group.boundaries.block_statement_separator);
                        }
                    }
                    closers.pairs.push(GroupClose {
                        closer: raw,
                        enclosing: scope
                            .groups
                            .last()
                            .and_then(|outer| NonZeroUsize::new(outer.raw + 1)),
                        boundaries: group.boundaries,
                    });
                    closers.by_opener[group.raw] = NonZeroUsize::new(closers.pairs.len());
                }
            }
            _ => {}
        }
    }
    closers
}
