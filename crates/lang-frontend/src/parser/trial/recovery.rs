//! Type-list recovery leaves the strict call and type witnesses intact.
use super::*;

impl StrictCallTrialIndex {
    /// 声明恢复只消费有真实闭合证据的类型列表，不授予 typed-call 准入。
    pub(in crate::parser) fn recovery_type_list_closer(
        &self,
        sources: &SourceMap,
        lexed: &LexedFile,
        raw_start: usize,
        allow_poison: bool,
    ) -> Result<Option<usize>, ParserInternalError> {
        match self.trial_at_raw(raw_start)? {
            Some(CallTrial::Match { closing_raw, .. }) => return Ok(Some(closing_raw)),
            Some(CallTrial::NoMatch {
                closed_list_raw: Some(closer),
                ..
            }) => return Ok(Some(closer)),
            Some(CallTrial::NoMatch { .. }) if allow_poison => {}
            _ => return Ok(None),
        }

        // 复用严格类型结构。替代原子的结果另存，保持严格查询和预算不变。
        let recovered = self.recovered_type_closers.get_or_init(|| {
            let (raw, significant): (Vec<_>, Vec<_>) = lexed
                .lexemes()
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, lexeme)| !matches!(lexeme.kind(), LexemeKind::Trivia(_)))
                .unzip();
            let built = TrialBuilder::new(sources, &significant, true).build()?;
            Ok(built
                .call_trials
                .into_iter()
                .map(|trial| match trial {
                    Some(SignificantCallTrial::Match {
                        closing_significant,
                        ..
                    }) => raw.get(closing_significant).copied(),
                    Some(SignificantCallTrial::NoMatch {
                        closed_list_significant: Some(closer),
                        ..
                    }) => raw.get(closer).copied(),
                    _ => None,
                })
                .collect())
        });
        let recovered = recovered.as_ref().map_err(Clone::clone)?;
        let significant =
            self.raw_to_significant[raw_start].ok_or(ParserInternalError::InvalidLexemeStream)?;
        Ok(recovered.get(significant).copied().flatten())
    }
}
