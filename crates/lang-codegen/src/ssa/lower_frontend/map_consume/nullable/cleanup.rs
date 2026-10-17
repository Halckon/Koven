//! Read-only cleanup preflight before the irreversible wrapper/view transfer.
use std::collections::BTreeSet;

use super::*;
use lang_frontend::ownership_checking::{DropTarget, IterationCleanupAction, LoanEndPoint};

impl ExpressionLowerer<'_> {
    pub(super) fn preflight_nullable_consume_cleanup(
        &self,
        source: &NullableConsumeSource,
        owner: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        // The carrier gate established no borrow state. A fact requiring that state
        // must fail here, rather than being swallowed by an absent-binding fallback.
        if self
            .owned
            .loans()
            .iter()
            .any(|loan| loan.call() == source.call || source.operands.contains(&loan.argument()))
            || self
                .owned
                .loan_ends()
                .iter()
                .any(|fact| fact.point() == LoanEndPoint::CallReturn(source.call))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let mut removed_symbols = BTreeSet::new();
        let mut removed_owners = BTreeSet::new();
        removed_symbols.insert(source.root);
        removed_owners.insert(owner);
        let points = source
            .operands
            .iter()
            .copied()
            .map(DropPoint::AfterExpression)
            .chain(std::iter::once(DropPoint::CallReturn(source.call)));
        for point in points {
            if self
                .owned
                .borrow_results()
                .ends()
                .iter()
                .any(|fact| fact.point() == point)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let facts = self
                .owned
                .drops()
                .iter()
                .copied()
                .filter(|fact| fact.point() == point)
                .collect::<Vec<_>>();
            let mut step_drops = Vec::new();
            for (_, action) in self
                .owned
                .cleanup_steps()
                .iter()
                .filter(|(at, _)| *at == point)
            {
                match action {
                    IterationCleanupAction::Drop(fact) => step_drops.push(*fact),
                    _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
                }
            }
            if step_drops != facts {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            for fact in facts {
                if fact.condition().is_some()
                    || fact.capture_slot().is_some()
                    || fact.instance_address().is_some()
                {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
                let DropTarget::Named(symbol) = fact.target() else {
                    // No receiver temporary is formed by Name/Group, and the call's
                    // result is not registered until the enclosing lower() returns.
                    return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                };
                let Some(LoweredValue::Value(value)) = self.bindings.get(&symbol).copied() else {
                    return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                };
                if !removed_symbols.insert(symbol) || !removed_owners.insert(value) {
                    return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                }
                let template = self
                    .typed
                    .symbol_type(symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact.value_origin()))?;
                let concrete = self.resolve_type(template, fact.value_origin())?;
                let ty =
                    self.type_ids.get(&concrete).copied().ok_or_else(|| {
                        error(LoweringErrorKind::MissingFact, fact.value_origin())
                    })?;
                if self.typed.copyability(concrete) != Some(Copyability::MoveOnly)
                    || self
                        .function
                        .entity(EntityId::Value(value))
                        .map(|data| data.ty)
                        != Some(EntityType::Value(ty))
                {
                    return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                }
            }
        }
        Ok(())
    }
}
