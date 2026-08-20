//! 调用点类型实参的无副作用严格预索引。
//!
//! 索引只保存识别结果，不构造 AST 或诊断。所有依赖都指向更靠后的 significant lexeme，
//! 因而可以反向迭代求值，避免 Rust 递归及每个 `<` 候选重复扫描后缀。

use crate::lexer::{Keyword, LexedFile, Lexeme, LexemeKind, Symbol, TokenKind};

use super::{MAX_RECURSION_DEPTH, ParserInternalError};

/// 一个调用点类型实参候选的严格识别结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CallTrial {
    /// `<...>` 完整合法，且其后下一个非 trivia token 是 `(`。
    Match {
        /// 匹配 `>` 在原始 lexeme 流中的下标。
        closing_raw: usize,
        /// 相对查询基线所需的最大附加递归深度。
        additional_depth: usize,
    },
    /// 当前 `<` 不能提交为 typed call；cursor 与其他 parser 状态均未改变。
    NoMatch {
        /// 判定失败前实际探索到的最大附加递归深度。
        additional_depth: usize,
    },
}

impl CallTrial {
    const fn additional_depth(self) -> usize {
        match self {
            Self::Match {
                additional_depth, ..
            }
            | Self::NoMatch { additional_depth } => additional_depth,
        }
    }
}

/// 一次根解析内共享的 strict trial 只读索引。
pub(super) struct StrictCallTrialIndex {
    raw_to_significant: Vec<Option<usize>>,
    trials: Vec<Option<CallTrial>>,
    #[cfg(test)]
    token_inspections: usize,
    #[cfg(test)]
    evaluated_states: usize,
}

impl StrictCallTrialIndex {
    /// 以一次线性构建为全部 `<` 候选建立严格识别结果。
    pub(super) fn new(lexed: &LexedFile) -> Result<Self, ParserInternalError> {
        validate_lexeme_shape(lexed)?;

        let mut raw_to_significant = vec![None; lexed.lexemes().len()];
        let mut significant = Vec::with_capacity(lexed.lexemes().len());
        let mut significant_to_raw = Vec::with_capacity(lexed.lexemes().len());
        for (raw, lexeme) in lexed.lexemes().iter().copied().enumerate() {
            if matches!(lexeme.kind(), LexemeKind::Trivia(_)) {
                continue;
            }
            raw_to_significant[raw] = Some(significant.len());
            significant.push(lexeme);
            significant_to_raw.push(raw);
        }
        // Parser cursor 指向“上一个已消费 token 之后”的 raw 位置，可能落在 trivia 上。
        // 反向填充使任意该类 checkpoint 都能 O(1) 定位其下一个 significant lexeme。
        let mut next_significant = None;
        for raw in (0..raw_to_significant.len()).rev() {
            if let Some(current) = raw_to_significant[raw] {
                next_significant = Some(current);
            }
            raw_to_significant[raw] = next_significant;
        }

        let built = TrialBuilder::new(&significant).build()?;
        let trials = built
            .call_trials
            .into_iter()
            .map(|trial| match trial {
                Some(SignificantCallTrial::Match {
                    closing_significant,
                    additional_depth,
                }) => significant_to_raw
                    .get(closing_significant)
                    .copied()
                    .map(|closing_raw| CallTrial::Match {
                        closing_raw,
                        additional_depth,
                    })
                    .map(Some)
                    .ok_or(ParserInternalError::InvalidLexemeStream),
                Some(SignificantCallTrial::NoMatch { additional_depth }) => {
                    Ok(Some(CallTrial::NoMatch { additional_depth }))
                }
                None => Ok(None),
            })
            .collect::<Result<Vec<Option<_>>, _>>()?;

        Ok(Self {
            raw_to_significant,
            trials,
            #[cfg(test)]
            token_inspections: built.token_inspections,
            #[cfg(test)]
            evaluated_states: built.evaluated_states,
        })
    }

    /// 查询一个原始 `<` 下标；查询本身为 `O(1)`，并按当前 parser 深度重验统一预算。
    pub(super) fn query(
        &self,
        raw_start: usize,
        baseline_depth: usize,
    ) -> Result<CallTrial, ParserInternalError> {
        let significant = self
            .raw_to_significant
            .get(raw_start)
            .copied()
            .flatten()
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        let trial = self
            .trials
            .get(significant)
            .copied()
            .flatten()
            .unwrap_or(CallTrial::NoMatch {
                additional_depth: 1,
            });
        if baseline_depth
            .checked_add(trial.additional_depth())
            .is_none_or(|depth| depth > MAX_RECURSION_DEPTH)
        {
            return Err(ParserInternalError::NestingLimitExceeded {
                limit: MAX_RECURSION_DEPTH,
            });
        }
        Ok(trial)
    }

    #[cfg(test)]
    const fn token_inspections(&self) -> usize {
        self.token_inspections
    }

    #[cfg(test)]
    const fn evaluated_states(&self) -> usize {
        self.evaluated_states
    }
}

#[derive(Clone, Copy)]
struct ParseSuccess {
    end: usize,
}

#[derive(Clone, Copy)]
struct ParseTrial {
    success: Option<ParseSuccess>,
    explored_depth: usize,
}

impl ParseTrial {
    const INVALID_TYPE: Self = Self {
        success: None,
        explored_depth: 1,
    };
}

#[derive(Clone, Copy)]
struct ListSuccess {
    end: usize,
    closing: usize,
}

#[derive(Clone, Copy, Default)]
struct ListTrial {
    success: Option<ListSuccess>,
    explored_depth: usize,
}

#[derive(Clone, Copy)]
enum SignificantCallTrial {
    Match {
        closing_significant: usize,
        additional_depth: usize,
    },
    NoMatch {
        additional_depth: usize,
    },
}

struct BuiltTrials {
    call_trials: Vec<Option<SignificantCallTrial>>,
    #[cfg(test)]
    token_inspections: usize,
    #[cfg(test)]
    evaluated_states: usize,
}

struct TrialBuilder<'a> {
    lexemes: &'a [Lexeme],
    #[cfg(test)]
    inspections: usize,
    #[cfg(test)]
    evaluated_states: usize,
}

impl<'a> TrialBuilder<'a> {
    const fn new(lexemes: &'a [Lexeme]) -> Self {
        Self {
            lexemes,
            #[cfg(test)]
            inspections: 0,
            #[cfg(test)]
            evaluated_states: 0,
        }
    }

    fn build(mut self) -> Result<BuiltTrials, ParserInternalError> {
        let len = self.lexemes.len();
        let mut path_ends = vec![None; len];
        for index in (0..len).rev() {
            if !self.is_identifier(index) {
                continue;
            }
            path_ends[index] =
                if self.is_symbol(index + 1, Symbol::Dot) && self.is_identifier(index + 2) {
                    path_ends[index + 2]
                } else {
                    Some(index + 1)
                };
        }

        let mut types = vec![ParseTrial::INVALID_TYPE; len];
        let mut angle_lists = vec![ListTrial::default(); len];
        let mut paren_lists = vec![ListTrial::default(); len];
        let mut call_trials = vec![None; len];

        for index in (0..len).rev() {
            if self.is_symbol(index, Symbol::Less) {
                self.record_state();
                let list = self.parse_list(index, Symbol::Greater, &types)?;
                angle_lists[index] = list;
                call_trials[index] = Some(match list.success {
                    Some(success) if self.is_symbol(success.end, Symbol::LeftParen) => {
                        SignificantCallTrial::Match {
                            closing_significant: success.closing,
                            additional_depth: list.explored_depth,
                        }
                    }
                    _ => SignificantCallTrial::NoMatch {
                        additional_depth: list.explored_depth,
                    },
                });
            }
            if self.is_symbol(index, Symbol::LeftParen) {
                self.record_state();
                paren_lists[index] = self.parse_list(index, Symbol::RightParen, &types)?;
            }

            self.record_state();
            types[index] = if self.is_identifier(index) {
                self.parse_qualified(index, &path_ends, &angle_lists)?
            } else if self.is_symbol(index, Symbol::LeftParen) {
                self.parse_function(index, &paren_lists, &types)?
            } else if self.is_keyword(index, Keyword::Move)
                && self.is_symbol(index + 1, Symbol::LeftParen)
            {
                types[index + 1]
            } else {
                ParseTrial::INVALID_TYPE
            };
        }

        Ok(BuiltTrials {
            call_trials,
            #[cfg(test)]
            token_inspections: self.inspections,
            #[cfg(test)]
            evaluated_states: self.evaluated_states,
        })
    }

    fn parse_list(
        &mut self,
        opener: usize,
        closer: Symbol,
        types: &[ParseTrial],
    ) -> Result<ListTrial, ParserInternalError> {
        let mut cursor = opener + 1;
        if closer == Symbol::RightParen && self.is_symbol(cursor, closer) {
            return Ok(ListTrial {
                success: Some(ListSuccess {
                    end: cursor + 1,
                    closing: cursor,
                }),
                explored_depth: 0,
            });
        }

        let mut explored_depth = 0;
        loop {
            if closer == Symbol::RightParen
                && (self.is_keyword(cursor, Keyword::Borrow)
                    || self.is_keyword(cursor, Keyword::Inout))
            {
                cursor += 1;
            }
            let child = types
                .get(cursor)
                .copied()
                .unwrap_or(ParseTrial::INVALID_TYPE);
            explored_depth = explored_depth.max(child.explored_depth);
            let Some(success) = child.success else {
                return Ok(ListTrial {
                    success: None,
                    explored_depth,
                });
            };
            cursor = success.end;
            if self.is_symbol(cursor, Symbol::Comma) {
                cursor += 1;
                continue;
            }
            if self.is_symbol(cursor, closer) {
                return Ok(ListTrial {
                    success: Some(ListSuccess {
                        end: cursor + 1,
                        closing: cursor,
                    }),
                    explored_depth,
                });
            }
            return Ok(ListTrial {
                success: None,
                explored_depth,
            });
        }
    }

    fn parse_qualified(
        &mut self,
        start: usize,
        path_ends: &[Option<usize>],
        angle_lists: &[ListTrial],
    ) -> Result<ParseTrial, ParserInternalError> {
        let mut end = path_ends
            .get(start)
            .copied()
            .flatten()
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        let mut explored_depth = 1;
        if self.is_symbol(end, Symbol::Less) {
            let list = angle_lists
                .get(end)
                .copied()
                .ok_or(ParserInternalError::InvalidLexemeStream)?;
            explored_depth = explored_depth.max(add_depth(list.explored_depth)?);
            let Some(success) = list.success else {
                return Ok(ParseTrial {
                    success: None,
                    explored_depth,
                });
            };
            end = success.end;
        }
        if self.is_symbol(end, Symbol::Question) {
            end += 1;
        }
        Ok(ParseTrial {
            success: Some(ParseSuccess { end }),
            explored_depth,
        })
    }

    fn parse_function(
        &mut self,
        start: usize,
        paren_lists: &[ListTrial],
        types: &[ParseTrial],
    ) -> Result<ParseTrial, ParserInternalError> {
        let list = paren_lists
            .get(start)
            .copied()
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        let mut explored_depth = 1;
        if list.explored_depth > 0 {
            explored_depth = explored_depth.max(add_depth(list.explored_depth)?);
        }
        let Some(parameters) = list.success else {
            return Ok(ParseTrial {
                success: None,
                explored_depth,
            });
        };
        if !self.is_symbol(parameters.end, Symbol::Arrow) {
            return Ok(ParseTrial {
                success: None,
                explored_depth,
            });
        }
        let return_type = types
            .get(parameters.end + 1)
            .copied()
            .unwrap_or(ParseTrial::INVALID_TYPE);
        explored_depth = explored_depth.max(add_depth(return_type.explored_depth)?);
        let Some(return_success) = return_type.success else {
            return Ok(ParseTrial {
                success: None,
                explored_depth,
            });
        };
        Ok(ParseTrial {
            success: Some(ParseSuccess {
                end: return_success.end,
            }),
            explored_depth,
        })
    }

    fn kind(&mut self, index: usize) -> Option<LexemeKind> {
        self.record_inspection();
        self.lexemes.get(index).map(|lexeme| lexeme.kind())
    }

    #[cfg(test)]
    fn record_inspection(&mut self) {
        self.inspections += 1;
    }

    #[cfg(not(test))]
    const fn record_inspection(&mut self) {}

    #[cfg(test)]
    fn record_state(&mut self) {
        self.evaluated_states += 1;
    }

    #[cfg(not(test))]
    const fn record_state(&mut self) {}

    fn is_identifier(&mut self, index: usize) -> bool {
        matches!(
            self.kind(index),
            Some(LexemeKind::Token(TokenKind::Identifier))
        )
    }

    fn is_keyword(&mut self, index: usize, expected: Keyword) -> bool {
        matches!(
            self.kind(index),
            Some(LexemeKind::Token(TokenKind::Keyword(keyword))) if keyword == expected
        )
    }

    fn is_symbol(&mut self, index: usize, expected: Symbol) -> bool {
        matches!(
            self.kind(index),
            Some(LexemeKind::Token(TokenKind::Symbol(symbol))) if symbol == expected
        )
    }
}

fn add_depth(depth: usize) -> Result<usize, ParserInternalError> {
    depth
        .checked_add(1)
        .ok_or(ParserInternalError::InvalidLexemeStream)
}

fn validate_lexeme_shape(lexed: &LexedFile) -> Result<(), ParserInternalError> {
    if lexed.lexemes().is_empty() {
        return Err(ParserInternalError::InvalidLexemeStream);
    }
    let mut next_start = 0;
    let mut saw_eof = false;
    for (index, lexeme) in lexed.lexemes().iter().copied().enumerate() {
        let span = lexeme.span();
        if span.source_id() != lexed.source_id() || span.start() != next_start {
            return Err(ParserInternalError::InvalidLexemeStream);
        }
        if matches!(lexeme.kind(), LexemeKind::Eof) {
            if saw_eof
                || index + 1 != lexed.lexemes().len()
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
    }
    saw_eof
        .then_some(())
        .ok_or(ParserInternalError::InvalidLexemeStream)
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer::{LexemeKind, Symbol, TokenKind, lex},
        source::SourceMap,
    };

    use super::{CallTrial, MAX_RECURSION_DEPTH, ParserInternalError, StrictCallTrialIndex};

    fn indexed(text: &str) -> (crate::lexer::LexedFile, StrictCallTrialIndex) {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("trial.ko", text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let index = StrictCallTrialIndex::new(&lexed).expect("lexer output must index");
        (lexed, index)
    }

    fn less_raw(lexed: &crate::lexer::LexedFile, nth: usize) -> usize {
        lexed
            .lexemes()
            .iter()
            .enumerate()
            .filter(|(_, lexeme)| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Less))
                )
            })
            .nth(nth)
            .map(|(raw, _)| raw)
            .expect("test input must contain the requested '<'")
    }

    #[test]
    fn recognizes_complete_strict_types_and_rejects_incomplete_candidates() {
        let (lexed, index) =
            indexed("f<A.B<C?>, Box<move (borrow D, inout E<F>) -> G?>> /* trivia */ ()");
        let trial = index
            .query(less_raw(&lexed, 0), 0)
            .expect("query must work");
        let CallTrial::Match {
            closing_raw,
            additional_depth,
        } = trial
        else {
            panic!("complete strict type arguments must match");
        };
        assert!(additional_depth >= 3);
        assert!(matches!(
            lexed.lexemes()[closing_raw].kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater))
        ));

        for malformed in [
            "f<T>",
            "f<>()",
            "f<T,>()",
            "f<,T>()",
            "f<T",
            "f<(borrow inout T) -> R>()",
            "f<(borrow borrow T) -> R>()",
            "f<Box<(borrow inout T) -> R>>()",
        ] {
            let (lexed, index) = indexed(malformed);
            assert!(matches!(
                index.query(less_raw(&lexed, 0), 0),
                Ok(CallTrial::NoMatch {
                    additional_depth: 1..
                })
            ));
        }
    }

    #[test]
    fn reapplies_budget_to_cached_match_and_no_match() {
        let nested = "A<".repeat(8) + "T" + &">".repeat(8);
        for suffix in ["()", ""] {
            let text = format!("f<{nested}>{suffix}");
            let (lexed, index) = indexed(&text);
            let raw = less_raw(&lexed, 0);
            let shallow = index.query(raw, 0).expect("shallow query must fit");
            let additional_depth = shallow.additional_depth();
            assert!(additional_depth >= 9);
            let exact_limit = MAX_RECURSION_DEPTH - additional_depth;
            let inspections = index.token_inspections();
            let states = index.evaluated_states();
            assert_eq!(
                index
                    .query(raw, exact_limit)
                    .expect("baseline + additional depth == 1024 must fit"),
                shallow
            );
            assert!(matches!(
                index.query(raw, exact_limit + 1),
                Err(ParserInternalError::NestingLimitExceeded {
                    limit: MAX_RECURSION_DEPTH
                })
            ));
            assert_eq!(
                index.query(raw, 0).expect("cache must be reusable"),
                shallow
            );
            assert_eq!(index.token_inspections(), inspections);
            assert_eq!(index.evaluated_states(), states);
        }
    }

    #[test]
    fn successful_failed_and_mixed_trial_families_stay_linear_when_doubled() {
        fn successful(cases: usize) -> String {
            std::iter::repeat_n(
                "f /* trivia */ <A.B.C<D, E>, move (borrow F, inout G<H>) -> I?> /* trivia */ ()",
                cases,
            )
            .collect::<Vec<_>>()
            .join(" + ")
        }

        fn terminal_no_match(levels: usize) -> String {
            let mut text = "f /* trivia */ <".to_owned();
            for _ in 0..levels {
                text.push_str("A /* trivia */ <");
            }
            text.push('T');
            for _ in 0..=levels {
                text.push_str("> /* trivia */ ");
            }
            text
        }

        fn alternating(cases: usize) -> String {
            (0..cases)
                .map(|index| {
                    if index % 2 == 0 {
                        "f /* trivia */ <A<B>>()"
                    } else {
                        "f /* trivia */ <A<B>>"
                    }
                })
                .collect::<Vec<_>>()
                .join(" + ")
        }

        fn metrics(text: &str) -> (usize, usize, usize, usize, usize) {
            let (lexed, index) = indexed(text);
            let raw = lexed.lexemes().len();
            let significant = lexed
                .lexemes()
                .iter()
                .filter(|lexeme| !matches!(lexeme.kind(), LexemeKind::Trivia(_)))
                .count();
            let mut matches = 0;
            for nth in 0..lexed
                .lexemes()
                .iter()
                .filter(|lexeme| {
                    matches!(
                        lexeme.kind(),
                        LexemeKind::Token(TokenKind::Symbol(Symbol::Less))
                    )
                })
                .count()
            {
                if matches!(
                    index
                        .query(less_raw(&lexed, nth), 0)
                        .expect("every '<' has a cached trial"),
                    CallTrial::Match { .. }
                ) {
                    matches += 1;
                }
            }

            // `validate_lexeme_shape` 与 significant-stream 构建各线性查看 raw 流一次；
            // 其后的动态 counter 覆盖 DP 的全部 token-kind inspection。
            let accounted_inspections = raw * 2 + index.token_inspections();
            (
                raw,
                significant,
                accounted_inspections,
                index.evaluated_states(),
                matches,
            )
        }

        fn assert_linear_doubling(
            label: &str,
            smaller: (usize, usize, usize, usize, usize),
            larger: (usize, usize, usize, usize, usize),
        ) {
            let (small_raw, small_significant, small_inspections, small_states, _) = smaller;
            let (large_raw, large_significant, large_inspections, large_states, _) = larger;

            assert!(
                small_states <= small_significant * 3,
                "{label}: small states"
            );
            assert!(
                large_states <= large_significant * 3,
                "{label}: large states"
            );
            assert!(
                small_inspections <= small_raw * 34,
                "{label}: {small_inspections} inspections for {small_raw} raw lexemes"
            );
            assert!(
                large_inspections <= large_raw * 34,
                "{label}: {large_inspections} inspections for {large_raw} raw lexemes"
            );
            assert!(
                large_inspections <= small_inspections * 2 + 64,
                "{label}: inspections grew from {small_inspections} to {large_inspections}"
            );
            assert!(
                large_states <= small_states * 2 + 16,
                "{label}: states grew from {small_states} to {large_states}"
            );
        }

        let success_small = metrics(&successful(64));
        let success_large = metrics(&successful(128));
        assert_eq!(success_small.4, 64);
        assert_eq!(success_large.4, 128);
        assert_linear_doubling("successful", success_small, success_large);

        let failed_small = metrics(&terminal_no_match(128));
        let failed_large = metrics(&terminal_no_match(256));
        assert_eq!(failed_small.4, 0);
        assert_eq!(failed_large.4, 0);
        assert_linear_doubling("terminal no-match", failed_small, failed_large);

        let mixed_small = metrics(&alternating(64));
        let mixed_large = metrics(&alternating(128));
        assert_eq!(mixed_small.4, 32);
        assert_eq!(mixed_large.4, 64);
        assert_linear_doubling("alternating", mixed_small, mixed_large);
    }
}
