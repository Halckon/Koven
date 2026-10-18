//! Carry the active provider through ordinary checked-scalar success/Abort edges.
use super::*;
#[derive(Clone, Copy)]
pub(super) struct Roles {
    pub source: LoanId,
    pub cursor: ValueId,
    pub key: LoanId,
    pub value: LoanId,
}
impl ExpressionLowerer<'_> {
    pub(in super::super) fn carry_checked_map(
        &self,
        carried: &mut LinearBindings,
        span: Span,
    ) -> Result<(), LoweringError> {
        for context in &self.loops {
            let Some(state) = &context.map_loop else {
                continue;
            };
            if state.cleaned
                || state.frame.finished
                || state.body != self.block
                || state.nested.is_some()
                || state.conditional.is_some()
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let entities = [
                EntityId::Loan(
                    state
                        .frame
                        .source
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                ),
                EntityId::Value(state.cursor),
            ];
            for entity in entities {
                if !carried.slots.iter().any(|s| s.source == entity) {
                    let ty = self
                        .function
                        .entity(entity)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                        .ty;
                    carried.slots.push(LinearBindingSlot::new(None, entity, ty));
                }
            }
        }
        Ok(())
    }
    pub(in super::super) fn resume_checked_map(
        &mut self,
        from: BlockId,
        carried: &LinearBindings,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let parameters = &self.function.block(self.block).ok_or_else(bad)?.parameters;
        let mapped = |entity| {
            let slot = unique(
                carried
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| (s.source == entity).then_some(i)),
            )
            .ok_or_else(bad)?;
            let current = *parameters.get(slot).ok_or_else(bad)?;
            nested::transport(self.function, current, entity, span)?;
            Ok::<_, LoweringError>(current)
        };
        for context in &mut self.loops {
            let Some(state) = &mut context.map_loop else {
                continue;
            };
            if state.body != from
                || state.cleaned
                || state.frame.finished
                || state.nested.is_some()
                || state.conditional.is_some()
            {
                return Err(bad());
            }
            let EntityId::Loan(source) =
                mapped(EntityId::Loan(state.frame.source.ok_or_else(bad)?))?
            else {
                return Err(bad());
            };
            let EntityId::Value(cursor) = mapped(EntityId::Value(state.cursor))? else {
                return Err(bad());
            };
            let [(key_symbol, Some(old_key)), (value_symbol, Some(old_value))] =
                state.frame.components
            else {
                return Err(bad());
            };
            let EntityId::Loan(key) = mapped(EntityId::Loan(old_key))? else {
                return Err(bad());
            };
            let EntityId::Loan(value) = mapped(EntityId::Loan(old_value))? else {
                return Err(bad());
            };
            if self.borrow_bindings.get(&key_symbol) != Some(&key)
                || self.borrow_bindings.get(&value_symbol) != Some(&value)
            {
                return Err(bad());
            }
            if let Some((proof, identity, old)) = state.owner {
                let EntityId::Value(current) = mapped(EntityId::Value(old))? else {
                    return Err(bad());
                };
                // These are actual current bindings restored by the ordinary scalar carrier.
                let expected = match identity {
                    SourceIdentity::Parameter(root) => self.bindings.get(&root).copied(),
                    _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
                };
                if expected != Some(LoweredValue::Value(current)) {
                    return Err(bad());
                }
                state.owner = Some((proof, identity, current));
            }
            state.frame.source = Some(source);
            state.frame.components = [(key_symbol, Some(key)), (value_symbol, Some(value))];
            state.cursor = cursor;
            state.body = self.block;
            state.resumed = Some(Roles {
                source,
                cursor,
                key,
                value,
            });
        }
        Ok(())
    }
}
