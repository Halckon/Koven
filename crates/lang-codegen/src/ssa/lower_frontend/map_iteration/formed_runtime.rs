//! The current formed owner traces to its actual Call; local Move does not create a new SSA owner.
use super::*;
use crate::ssa::map_source::{Birth, Formation};
use lang_frontend::ownership_checking::{DropFact, DropTarget};
use std::collections::BTreeMap;

impl ExpressionLowerer<'_> {
    pub(in super::super) fn validate_formed_map_drop(
        &self,
        fact: DropFact,
        owner: ValueId,
    ) -> Result<bool, LoweringError> {
        let DropTarget::Named(root) = fact.target() else {
            return Ok(false);
        };
        for plan in self.owned.map_iterations() {
            let descriptor = plan.descriptor();
            let qualified =
                qualify::qualify(self.parsed, self.names, self.typed, self.owned, *descriptor)?;
            let SourceIdentity::Formed { symbol, birth } = qualified.source else {
                continue;
            };
            if symbol != root || qualified.local.is_some() {
                continue;
            }
            let span = self
                .parsed
                .ast()
                .statements()
                .get(descriptor.statement())
                .map_err(|_| error(LoweringErrorKind::MissingFact, fact.value_origin()))?
                .span();
            let formation = formed::formation(
                self.parsed,
                self.names,
                self.typed,
                self.owned,
                root,
                descriptor.source_type(),
                span,
            )?;
            if formation.birth != birth {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let bad = || error(LoweringErrorKind::MissingFact, fact.value_origin());
            let event =
                crate::ssa::map_source::drop_event(self.parsed, fact.point()).ok_or_else(bad)?;
            if !crate::ssa::map_source::in_formation(self.parsed, &formation, event)? {
                // Qualification above checked this epoch's exact public owner/condition.
                let epoch = crate::ssa::map_source::epoch_at(self.parsed, &formation, event)?;
                let resolved = formed::epoch::resolve(
                    (self.parsed, self.names, self.typed, self.owned),
                    root,
                    descriptor.source_type(),
                    epoch.rhs,
                    event.span,
                )?;
                if let Some(formation) = resolved.formation {
                    self.formed_map_anchor(&formation, Some(owner), resolved.at)?;
                } else {
                    let Birth::Call(producer) = resolved.birth else {
                        return Err(bad());
                    };
                    let anchor = self.map_call_birth(producer, event.span)?;
                    nested::transport(
                        self.function,
                        EntityId::Value(owner),
                        EntityId::Value(anchor),
                        event.span,
                    )?;
                }
                return Ok(true);
            }
            let matched = qualified
                .owner
                .map(|proof| proof.at_checked(fact.point(), self.owned))
                .transpose()?
                .flatten();
            if matched != Some(fact) {
                return Err(bad());
            }
            let terminal = crate::ssa::map_source::continuation(
                self.parsed,
                &formation,
                event,
                span,
                |id| {
                    self.typed
                        .expression_type(id)
                        .zip(self.typed.expression_category(id))
                },
                |at| self.references.get(&span_key(at)).copied().ok_or_else(bad),
                self.typed
                    .map_puts()
                    .iter()
                    .map(|d| crate::ssa::map_source::OwnerUpdate {
                        expression: d.expression(),
                        receiver: d.receiver(),
                    })
                    .chain(self.typed.map_removes().iter().map(|d| {
                        crate::ssa::map_source::OwnerUpdate {
                            expression: d.expression(),
                            receiver: d.receiver(),
                        }
                    })),
            )?;
            if !formation.same_epoch(&terminal) {
                return Err(bad());
            }
            self.formed_map_anchor(&terminal, Some(owner), event.span)?;
            return Ok(true);
        }
        for descriptor in self.typed.map_iterations() {
            let span = self
                .parsed
                .ast()
                .statements()
                .get(descriptor.statement())
                .map_err(|_| error(LoweringErrorKind::MissingFact, fact.value_origin()))?
                .span();
            let Some(plan) = self.owned.map_iteration(descriptor.statement()) else {
                continue;
            };
            if let Some(retained) = formed::retained::proof(
                (self.parsed, self.names, self.typed, self.owned),
                plan,
                fact,
                span,
            )? {
                if let Some(formation) = retained.formation {
                    self.formed_map_anchor(&formation, Some(owner), span)?;
                } else {
                    let SourceIdentity::Parameter(root) = retained.source else {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    };
                    let anchor = *self
                        .map_iteration_state
                        .entries
                        .get(&root)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let entry = self
                        .function
                        .entry_block()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    if !matches!(self.function.entity(anchor).map(|e|e.definition),Some(Definition::BlockParameter{block,..}) if block==entry)
                        || !matches!(anchor, EntityId::Value(_))
                        || self
                            .bindings
                            .values()
                            .any(|v| *v == LoweredValue::Value(owner))
                        || self.temporaries.values().any(|&v| v == owner)
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    nested::transport(self.function, EntityId::Value(owner), anchor, span)?;
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn formed_map_anchor(
        &self,
        formation: &Formation<SymbolId>,
        dropped: Option<ValueId>,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let anchor = match formation.birth {
            Birth::Call(producer) => self.map_call_birth(producer, span)?,
            Birth::Expression(_) => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            Birth::EntryParameter {
                root,
                reference,
                origin,
            } => {
                let symbol = self.names.symbols().get(root.index()).ok_or_else(bad)?;
                if symbol.span() != origin
                    || self.typed.parameter_mode(root) != Some(ParameterMode::Value)
                    || self
                        .references
                        .get(&span_key(self.expression_span(reference)?))
                        != Some(&root)
                {
                    return Err(bad());
                }
                let EntityId::Value(anchor) = *self
                    .map_iteration_state
                    .entries
                    .get(&root)
                    .ok_or_else(bad)?
                else {
                    return Err(bad());
                };
                let entry = self.function.entry_block().ok_or_else(bad)?;
                if !matches!(self.function.entity(EntityId::Value(anchor)).map(|e|e.definition),Some(Definition::BlockParameter{block,..}) if block==entry)
                {
                    return Err(bad());
                }
                anchor
            }
        };
        let current = if let Some(owner) = dropped {
            owner
        } else {
            let root = formation.roots.first().ok_or_else(bad)?;
            match self.bindings.get(root) {
                Some(LoweredValue::Value(owner)) => *owner,
                _ => return Err(bad()),
            }
        };
        if self
            .temporaries
            .values()
            .any(|v| *v == current || *v == anchor)
            || self.bindings.iter().any(|(root, value)| {
                Some(root) != formation.roots.first()
                    && matches!(value,LoweredValue::Value(v) if *v==current||*v==anchor)
            })
        {
            return Err(bad());
        }
        check_formed_transport(self, formation, current, anchor, span)?;
        Ok(anchor)
    }
}

fn check_formed_transport(
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
