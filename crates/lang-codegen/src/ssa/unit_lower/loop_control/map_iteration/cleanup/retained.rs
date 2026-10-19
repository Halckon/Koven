//! A retained same-root provider may be retired only by its proven active ancestor.
use super::*;

impl UnitExpressionLowerer<'_> {
    pub(super) fn map_owner_retired_by_ancestor(
        &self,
        statement: UnitStatementId,
        state: MapState,
        point: UnitDropPoint,
        span: Span,
    ) -> Result<bool, LoweringError> {
        if state.carrier.identity.is_registered(self) {
            return Ok(false);
        }
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let Some(EntityId::Value(owner)) = self.map_cached_seed(state) else {
            return Err(bad());
        };
        let mut found = Vec::new();
        for iteration in self.loops.iter().filter_map(|c| c.iteration.as_ref()) {
            if iteration.statement == statement {
                break;
            }
            let IterationState::Map(ancestor) = &iteration.state else {
                continue;
            };
            if ancestor.carrier.identity.drop_target() != state.carrier.identity.drop_target()
                || !matches!(ancestor.phase, MapPhase::Terminal {
                    point: actual, finished: true, source_ended: true, owner_dropped: true,
                } if actual == point)
            {
                continue;
            }
            let fact = ancestor
                .carrier
                .drop_at(self.owned, point)?
                .ok_or_else(bad)?;
            if self.map_cached_seed(ancestor.path) != Some(EntityId::Value(owner)) {
                return Err(bad());
            }
            let actual = self
                .function
                .block(self.block)
                .ok_or_else(bad)?
                .instructions
                .iter()
                .filter_map(|id| self.function.instruction(*id))
                .filter(|i| {
                    i.operation == (Operation::Drop { owner })
                        && i.origin == Origin::Source(fact.value_origin())
                })
                .count();
            if actual != 1 {
                return Err(bad());
            }
            found.push(iteration.statement);
        }
        match found.as_slice() {
            [_] => Ok(true),
            _ => Err(bad()),
        }
    }
}
