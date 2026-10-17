//! Read-only preflight of the exact cleanup facts replayed after NullableTake.
use super::*;
use lang_frontend::ownership_checking::UnitDropTarget;
use std::collections::BTreeSet;

impl UnitExpressionLowerer<'_> {
    pub(super) fn preflight_nullable_map_consume_cleanup(
        &self,
        source: &NullableMapConsumeSource,
        wrapper: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        // These are the only state keys removed by the extraction itself. No generic
        // alias sweep may hide a second binding or a missing owner after mutation.
        if self.bindings.get(&source.root) != Some(&LoweredValue::Value(wrapper))
            || self
                .bindings
                .iter()
                .any(|(&root, &value)| root != source.root && value == LoweredValue::Value(wrapper))
            || self.temporaries.values().any(|&value| value == wrapper)
            || self.closure_bindings.contains_key(&source.root)
            || self.borrow_bindings.contains_key(&source.root)
            || self.result_source_loans.contains_key(&source.root)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let points = source
            .operands
            .iter()
            .copied()
            .map(UnitDropPoint::AfterExpression)
            .chain(std::iter::once(UnitDropPoint::CallReturn(
                source.descriptor.expression(),
            )));
        let mut removed_symbols = BTreeSet::new();
        let mut removed_owners = BTreeSet::new();
        for point in points {
            // The strict no-carrier path has no iteration/borrow/capture/receiver
            // cleanup. Reject inconsistent facts rather than let an emitter skip them.
            if self.owned.iteration_cleanup_at(point).is_some()
                || self
                    .owned
                    .borrow_results()
                    .ends()
                    .iter()
                    .any(|fact| fact.point() == point)
                || self
                    .owned
                    .conditional_receiver_drops()
                    .iter()
                    .any(|fact| fact.point() == point)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            for fact in self
                .owned
                .drops()
                .iter()
                .filter(|fact| fact.point() == point)
            {
                let template = match fact.target() {
                    UnitDropTarget::Named(root) => self.typed.symbol_type(root),
                    UnitDropTarget::Temporary(expression) => self.typed.expression_type(expression),
                    _ => {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            fact.value_origin(),
                        ));
                    }
                }
                .ok_or_else(|| {
                    lowering_error(LoweringErrorKind::MissingFact, fact.value_origin())
                })?;
                let concrete = resolve_concrete_type(
                    self.typed,
                    template,
                    self.substitutions,
                    self.static_self,
                    fact.value_origin(),
                )?;
                // Match the original emitter's generic Copyable specialization rule.
                if concrete != template && self.typed.copyability(concrete) == Copyability::Copyable
                {
                    continue;
                }
                let owner = match fact.target() {
                    UnitDropTarget::Named(root) => {
                        if root == source.root
                            || root.source_unit() != self.source_unit
                            || !removed_symbols.insert(root)
                        {
                            return Err(lowering_error(
                                LoweringErrorKind::MissingFact,
                                fact.value_origin(),
                            ));
                        }
                        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied()
                        else {
                            return Err(lowering_error(
                                LoweringErrorKind::MissingFact,
                                fact.value_origin(),
                            ));
                        };
                        owner
                    }
                    UnitDropTarget::Temporary(expression) => {
                        *self.temporaries.get(&expression).ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, fact.value_origin())
                        })?
                    }
                    _ => {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            fact.value_origin(),
                        ));
                    }
                };
                if owner == wrapper
                    || !removed_owners.insert(owner)
                    || self
                        .function
                        .entity(EntityId::Value(owner))
                        .map(|entity| entity.ty)
                        != self.type_ids.get(&concrete).copied().map(EntityType::Value)
                {
                    return Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        fact.value_origin(),
                    ));
                }
            }
        }
        Ok(())
    }
}
