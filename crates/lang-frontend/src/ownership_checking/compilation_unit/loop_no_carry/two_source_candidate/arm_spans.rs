//! Source projection only: authorization remains in the issuing closed fact.
use super::*;
use crate::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    source::Span,
};
/// Original source locations for one ordered, fully closed arm.
#[derive(Clone, Debug)]
pub struct ArmSourceSpans {
    body: Span,
    calls: Vec<Span>,
    end: Span,
}
impl ArmSourceSpans {
    /// Original arm block.
    pub fn body(&self) -> Span {
        self.body
    }
    /// Original producer followed by every consumer call, without deduplication.
    pub fn calls(&self) -> &[Span] {
        &self.calls
    }
    /// Original statement after which the final child End was validated.
    pub fn end(&self) -> Span {
        self.end
    }
}
pub(super) fn collect(
    fact: &TwoSourceCounterFact,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
) -> Option<[ArmSourceSpans; 2]> {
    let collect = |index: usize| {
        let arm = &fact.arms[index];
        let source = names
            .names()
            .index()
            .source_units()
            .iter()
            .find(|s| s.id() == arm.callable().source_unit())?;
        let parsed = inputs
            .iter()
            .find(|i| i.source_id() == source.source_id())?
            .parsed();
        let UnitDropPoint::AfterStatement(end) = arm.end() else {
            return None;
        };
        if arm.body().source_unit() != source.id()
            || end.source_unit() != source.id()
            || arm.consumers().last()?.0 != end
        {
            return None;
        }
        let calls = fact.calls[index]
            .iter()
            .map(|(id, _)| {
                if id.source_unit() != source.id() {
                    return None;
                }
                Some(parsed.ast().expressions().get(id.expression()).ok()?.span())
            })
            .collect::<Option<Vec<_>>>()?;
        Some(ArmSourceSpans {
            body: parsed
                .ast()
                .statements()
                .get(arm.body().statement())
                .ok()?
                .span(),
            calls,
            end: parsed.ast().statements().get(end.statement()).ok()?.span(),
        })
    };
    Some([collect(0)?, collect(1)?])
}
