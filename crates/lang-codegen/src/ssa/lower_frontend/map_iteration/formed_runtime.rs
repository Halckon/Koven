//! The current formed owner traces to its actual Call; local Move does not create a new SSA owner.
use super::*;
use lang_frontend::ownership_checking::{DropFact, DropTarget};

impl ExpressionLowerer<'_> {
    pub(in super::super) fn validate_formed_map_drop(
        &self,
        fact: DropFact,
        owner: ValueId,
    ) -> Result<bool, LoweringError> {
        let DropTarget::Named(root) = fact.target() else {
            return Ok(false);
        };
        for descriptor in self.typed.map_iterations() {
            let qualified =
                qualify::qualify(self.parsed, self.names, self.typed, self.owned, *descriptor)?;
            let SourceIdentity::Formed {
                symbol,
                producer: initializer,
            } = qualified.source
            else {
                continue;
            };
            if symbol != root
                || qualified.local.is_some()
                || qualified.owner.and_then(|proof| proof.at(fact.point())) != Some(fact)
            {
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
            if formation.producer != initializer {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.formed_map_anchor(&formation, Some(owner), span)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn formed_map_anchor(
        &self,
        formation: &crate::ssa::map_source::Formation<SymbolId>,
        dropped: Option<ValueId>,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let birth_span = self.expression_span(formation.producer)?;
        let instruction = unique(self.function.instructions.iter().filter(|i| {
            i.origin == Origin::Source(birth_span)
                && matches!(i.operation, Operation::DirectCall { .. })
        }))
        .ok_or_else(bad)?;
        let [EntityId::Value(anchor)] = instruction.results.as_slice() else {
            return Err(bad());
        };
        for root in formation.roots.iter().skip(1) {
            if self.bindings.contains_key(root) || self.borrow_bindings.contains_key(root) {
                return Err(bad());
            }
        }
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
            .any(|v| *v == current || *v == *anchor)
            || self.bindings.iter().any(|(root, value)| {
                Some(root) != formation.roots.first()
                    && matches!(value,LoweredValue::Value(v) if *v==current||*v==*anchor)
            })
        {
            return Err(bad());
        }
        nested::transport(
            self.function,
            EntityId::Value(current),
            EntityId::Value(*anchor),
            span,
        )?;
        Ok(*anchor)
    }
}
