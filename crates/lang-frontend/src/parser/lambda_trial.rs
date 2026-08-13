//! Lambda header 的无副作用严格预索引。
//!
//! 每个未决 DFA 遇到下一个非 trivia token 后都会前进、成功或永久失败；新的 `{`
//! 会先使旧 DFA 失败，再成为唯一的新候选。因此一次正向遍历即可为全部 opener 建立结果，
//! 不需要从任一 `{` 重新扫描后缀。

use crate::lexer::{LexedFile, LexemeKind, Symbol, TokenKind};

use super::{
    ParserInternalError,
    engine::{TerminalOwnerEvent, TerminalOwnerKind},
};

/// 一个 `{` 的严格 lambda header 识别结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum LambdaHeaderTrial {
    /// `{` 后的严格前缀不构成完整 header。
    NoHeader,
    /// 完整匹配 `[ Identifier { "," Identifier } ] "->"`。
    Header {
        /// 参数 Identifier 在原始 lexeme 流中的有序下标。
        parameter_raw: Vec<usize>,
        /// `->` 在原始 lexeme 流中的下标。
        arrow_raw: usize,
    },
}

/// 一次根解析内共享的 lambda header 只读索引。
pub(super) struct LambdaHeaderIndex {
    trials: Vec<Option<LambdaHeaderTrial>>,
    #[cfg(test)]
    raw_inspections: usize,
    #[cfg(test)]
    dfa_inspections: usize,
}

impl LambdaHeaderIndex {
    /// 在完整 raw lexeme 流上一次建立全部 `{` 的严格识别结果。
    pub(super) fn new(
        lexed: &LexedFile,
        terminal_owner_events: &[TerminalOwnerEvent],
    ) -> Result<Self, ParserInternalError> {
        if lexed.lexemes().is_empty() {
            return Err(ParserInternalError::InvalidLexemeStream);
        }

        let mut trials = vec![None; lexed.lexemes().len()];
        let mut owners = OwnerStack::default();
        let mut active: Option<ActiveTrial> = None;
        let mut next_start = 0;
        let mut saw_eof = false;
        let mut next_terminal_event = 0;
        #[cfg(test)]
        let mut raw_inspections = 0;
        #[cfg(test)]
        let mut dfa_inspections = 0;

        for (raw, lexeme) in lexed.lexemes().iter().copied().enumerate() {
            #[cfg(test)]
            {
                raw_inspections += 1;
            }

            let span = lexeme.span();
            while terminal_owner_events
                .get(next_terminal_event)
                .is_some_and(|event| event.offset <= span.start())
            {
                owners.apply_terminal(terminal_owner_events[next_terminal_event])?;
                next_terminal_event += 1;
            }
            if span.source_id() != lexed.source_id() || span.start() != next_start {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            if matches!(lexeme.kind(), LexemeKind::Eof) {
                if saw_eof
                    || raw + 1 != lexed.lexemes().len()
                    || !span.is_empty()
                    || span.end() != next_start
                {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                saw_eof = true;
            } else {
                if span.is_empty() {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                next_start = span.end();
            }

            if !matches!(lexeme.kind(), LexemeKind::Trivia(_)) {
                if let Some(candidate) = active.as_mut() {
                    #[cfg(test)]
                    {
                        dfa_inspections += 1;
                    }
                    match candidate.advance(raw, lexeme.kind()) {
                        TrialProgress::Continue => {}
                        TrialProgress::Failed => active = None,
                        TrialProgress::Matched => {
                            let candidate = active
                                .take()
                                .ok_or(ParserInternalError::InvalidLexemeStream)?;
                            trials[candidate.opener_raw] = Some(LambdaHeaderTrial::Header {
                                parameter_raw: candidate.parameters,
                                arrow_raw: raw,
                            });
                        }
                    }
                }

                if is_symbol(lexeme.kind(), Symbol::LeftBrace) && owners.syntax_tokens_enabled() {
                    // NoHeader 是永久失败的默认值；只有完整命中箭头才会原子替换它。
                    trials[raw] = Some(LambdaHeaderTrial::NoHeader);
                    active = Some(ActiveTrial::new(raw));
                }
            }

            owners.update(lexeme.kind(), span.start())?;
        }

        if !saw_eof || next_terminal_event != terminal_owner_events.len() {
            return Err(ParserInternalError::InvalidLexemeStream);
        }

        Ok(Self {
            trials,
            #[cfg(test)]
            raw_inspections,
            #[cfg(test)]
            dfa_inspections,
        })
    }

    /// 按原始 `{` 下标执行 `O(1)` 查询。
    pub(super) fn query(
        &self,
        opener_raw: usize,
    ) -> Result<&LambdaHeaderTrial, ParserInternalError> {
        self.trials
            .get(opener_raw)
            .and_then(Option::as_ref)
            .ok_or(ParserInternalError::InvalidLexemeStream)
    }

    #[cfg(test)]
    const fn raw_inspections(&self) -> usize {
        self.raw_inspections
    }

    #[cfg(test)]
    const fn dfa_inspections(&self) -> usize {
        self.dfa_inspections
    }
}

struct ActiveTrial {
    opener_raw: usize,
    parameters: Vec<usize>,
    state: TrialState,
}

impl ActiveTrial {
    const fn new(opener_raw: usize) -> Self {
        Self {
            opener_raw,
            parameters: Vec::new(),
            state: TrialState::Start,
        }
    }

    fn advance(&mut self, raw: usize, kind: LexemeKind) -> TrialProgress {
        match (self.state, kind) {
            (
                TrialState::Start | TrialState::AfterComma,
                LexemeKind::Token(TokenKind::Identifier),
            ) => {
                self.parameters.push(raw);
                self.state = TrialState::AfterIdentifier;
                TrialProgress::Continue
            }
            (
                TrialState::Start | TrialState::AfterIdentifier,
                LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow)),
            ) => TrialProgress::Matched,
            (TrialState::AfterIdentifier, LexemeKind::Token(TokenKind::Symbol(Symbol::Comma))) => {
                self.state = TrialState::AfterComma;
                TrialProgress::Continue
            }
            _ => TrialProgress::Failed,
        }
    }
}

#[derive(Clone, Copy)]
enum TrialState {
    Start,
    AfterIdentifier,
    AfterComma,
}

enum TrialProgress {
    Continue,
    Failed,
    Matched,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DelimiterOwner {
    Paren,
    Bracket,
    Brace,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LexicalOwner {
    String,
    Interpolation,
}

#[derive(Default)]
struct OwnerStack {
    delimiters: Vec<DelimiterOwner>,
    lexicals: Vec<(LexicalOwner, usize)>,
}

impl OwnerStack {
    fn syntax_tokens_enabled(&self) -> bool {
        !matches!(self.lexicals.last(), Some((LexicalOwner::String, _)))
    }

    fn apply_terminal(&mut self, event: TerminalOwnerEvent) -> Result<(), ParserInternalError> {
        let kind = match event.kind {
            TerminalOwnerKind::String => LexicalOwner::String,
            TerminalOwnerKind::Interpolation => LexicalOwner::Interpolation,
        };
        if self.lexicals.last().copied() != Some((kind, event.opener)) {
            return Err(ParserInternalError::InvalidLexemeStream);
        }
        self.lexicals.pop();
        Ok(())
    }

    fn update(&mut self, kind: LexemeKind, start: usize) -> Result<(), ParserInternalError> {
        match kind {
            LexemeKind::Token(TokenKind::StringStart) => {
                self.lexicals.push((LexicalOwner::String, start));
            }
            LexemeKind::Token(TokenKind::InterpolationStart) => {
                self.lexicals.push((LexicalOwner::Interpolation, start));
            }
            LexemeKind::Token(TokenKind::StringEnd) => {
                pop_lexical(&mut self.lexicals, LexicalOwner::String)?;
            }
            LexemeKind::Token(TokenKind::InterpolationEnd) => {
                pop_lexical(&mut self.lexicals, LexicalOwner::Interpolation)?;
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => {
                self.delimiters.push(DelimiterOwner::Paren);
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBracket)) => {
                self.delimiters.push(DelimiterOwner::Bracket);
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) => {
                self.delimiters.push(DelimiterOwner::Brace);
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                pop_if_top(&mut self.delimiters, DelimiterOwner::Paren);
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => {
                pop_if_top(&mut self.delimiters, DelimiterOwner::Bracket);
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)) => {
                pop_if_top(&mut self.delimiters, DelimiterOwner::Brace);
            }
            _ => {}
        }
        Ok(())
    }
}

fn pop_if_top<T: Copy + PartialEq>(owners: &mut Vec<T>, expected: T) {
    if owners.last() == Some(&expected) {
        owners.pop();
    }
}

fn pop_lexical(
    owners: &mut Vec<(LexicalOwner, usize)>,
    expected: LexicalOwner,
) -> Result<(), ParserInternalError> {
    if !matches!(owners.last(), Some((kind, _)) if *kind == expected) {
        return Err(ParserInternalError::InvalidLexemeStream);
    }
    owners.pop();
    Ok(())
}

fn is_symbol(kind: LexemeKind, expected: Symbol) -> bool {
    matches!(kind, LexemeKind::Token(TokenKind::Symbol(symbol)) if symbol == expected)
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer::{LexedFile, LexemeKind, Symbol, TokenKind, lex},
        source::SourceMap,
    };

    use super::{LambdaHeaderIndex, LambdaHeaderTrial, TerminalOwnerEvent, TerminalOwnerKind};

    fn indexed(text: &str) -> (LexedFile, LambdaHeaderIndex) {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("lambda-trial.ko", text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let index = LambdaHeaderIndex::new(&lexed, &[]).expect("lexer output must index");
        (lexed, index)
    }

    fn brace_raw(lexed: &LexedFile, nth: usize) -> usize {
        lexed
            .lexemes()
            .iter()
            .enumerate()
            .filter(|(_, lexeme)| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
                )
            })
            .nth(nth)
            .map(|(raw, _)| raw)
            .expect("test input must contain requested '{'")
    }

    #[test]
    fn distinguishes_absent_explicit_zero_and_parameterized_headers() {
        let (lexed, index) = indexed("{} { -> body } { first, /* t */ second -> first }");
        assert_eq!(
            index.query(brace_raw(&lexed, 0)).expect("brace must index"),
            &LambdaHeaderTrial::NoHeader
        );
        assert!(matches!(
            index.query(brace_raw(&lexed, 1)),
            Ok(LambdaHeaderTrial::Header {
                parameter_raw,
                ..
            }) if parameter_raw.is_empty()
        ));
        assert!(matches!(
            index.query(brace_raw(&lexed, 2)),
            Ok(LambdaHeaderTrial::Header {
                parameter_raw,
                ..
            }) if parameter_raw.len() == 2
        ));
    }

    #[test]
    fn first_failure_is_permanent_and_does_not_scan_the_body() {
        let long_tail = "name, ".repeat(4_096);
        let text = format!("{{ ({long_tail}) -> tail }}");
        let (lexed, index) = indexed(&text);
        assert_eq!(
            index.query(brace_raw(&lexed, 0)).expect("brace must index"),
            &LambdaHeaderTrial::NoHeader
        );
        assert_eq!(index.dfa_inspections(), 1);
        assert_eq!(index.raw_inspections(), lexed.lexemes().len());
    }

    #[test]
    fn doubling_successful_headers_keeps_exactly_one_raw_pass() {
        fn family(count: usize) -> String {
            std::iter::repeat_n("{ first, second -> first }", count)
                .collect::<Vec<_>>()
                .join(" + ")
        }

        let (small_lexed, small) = indexed(&family(128));
        let (large_lexed, large) = indexed(&family(256));
        assert_eq!(small.raw_inspections(), small_lexed.lexemes().len());
        assert_eq!(large.raw_inspections(), large_lexed.lexemes().len());
        assert!(large.dfa_inspections() <= small.dfa_inspections() * 2 + 1);
        // 两组之间多出的 join trivia 是固定边界成本，不随 case 内 header 长度增长。
        assert!(large.raw_inspections() <= small.raw_inspections() * 2 + 8);
    }

    #[test]
    fn deeply_nested_candidates_each_inspect_only_a_fixed_prefix() {
        let depth = 2_048;
        let text = "{".repeat(depth) + "x" + &"}".repeat(depth);
        let (lexed, index) = indexed(&text);
        assert_eq!(index.raw_inspections(), lexed.lexemes().len());
        assert_eq!(index.dfa_inspections(), depth + 1);
        for nth in 0..depth {
            assert_eq!(
                index
                    .query(brace_raw(&lexed, nth))
                    .expect("brace must index"),
                &LambdaHeaderTrial::NoHeader
            );
        }
    }

    #[test]
    fn terminal_string_events_restore_header_indexing_after_newline_recovery() {
        for text in ["\"bad\n{ x -> x }", "\"bad\\\n{ x -> x }"] {
            let mut sources = SourceMap::new();
            let source_id = sources
                .add_source("recovered-string.ko", text)
                .expect("test source name must be unique");
            let lexed = lex(&sources, source_id).expect("test source must lex");
            let diagnostic = lexed
                .diagnostics()
                .first()
                .expect("test input must produce a terminal string diagnostic");
            let events = [TerminalOwnerEvent {
                offset: diagnostic.primary_span().end(),
                kind: TerminalOwnerKind::String,
                opener: 0,
            }];
            let index = LambdaHeaderIndex::new(&lexed, &events)
                .expect("terminal owner event must restore syntax mode");
            assert!(matches!(
                index.query(brace_raw(&lexed, 0)),
                Ok(LambdaHeaderTrial::Header { parameter_raw, .. })
                    if parameter_raw.len() == 1
            ));
        }
    }

    #[test]
    fn nested_terminal_events_close_inner_to_outer_without_panicking() {
        let text = "\"${\"inner";
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("nested-terminal.ko", text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let events = [
            TerminalOwnerEvent {
                offset: text.len(),
                kind: TerminalOwnerKind::String,
                opener: 3,
            },
            TerminalOwnerEvent {
                offset: text.len(),
                kind: TerminalOwnerKind::Interpolation,
                opener: 1,
            },
            TerminalOwnerEvent {
                offset: text.len(),
                kind: TerminalOwnerKind::String,
                opener: 0,
            },
        ];
        let index = LambdaHeaderIndex::new(&lexed, &events)
            .expect("nested terminal owners must close in verified order");
        assert_eq!(index.raw_inspections(), lexed.lexemes().len());
    }
}
