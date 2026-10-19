//! Current owners traverse actual mutation results and every ordinary CFG incoming.
use super::*;
use crate::ssa::map_source::Formation;
use std::collections::BTreeMap;

pub(super) fn check(
    lowerer: &ExpressionLowerer<'_>,
    formation: &Formation<SymbolId>,
    current: ValueId,
    anchor: ValueId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let function = &lowerer.function;
    let anchor = EntityId::Value(anchor);
    let mut updates = BTreeMap::new();
    for update in &formation.updates {
        let at = lowerer.expression_span(update.expression)?;
        let put = lowerer.typed.map_put(update.expression);
        let remove = lowerer.typed.map_remove(update.expression);
        if put.is_some() == remove.is_some()
            || put
                .map(|d| d.receiver())
                .or_else(|| remove.map(|d| d.receiver()))
                != Some(update.receiver)
        {
            return Err(bad());
        }
        let instruction = unique(function.instructions.iter().filter(|i| {
            i.origin == Origin::Source(at)
                && matches!(
                    i.operation,
                    Operation::MapPut { .. } | Operation::MapRemove { .. }
                )
        }))
        .ok_or_else(bad)?;
        let owner = match instruction.operation {
            Operation::MapPut { owner, .. } if put.is_some() => owner,
            Operation::MapRemove { owner, .. } if remove.is_some() => owner,
            _ => return Err(bad()),
        };
        let Some(&result @ EntityId::Value(_)) = instruction.results.first() else {
            return Err(bad());
        };
        if updates.insert(result, EntityId::Value(owner)).is_some() {
            return Err(bad());
        }
    }
    crate::ssa::map_source::transport::check(
        function,
        EntityId::Value(current),
        anchor,
        &updates,
        span,
    )
}
