use super::Coverage;
// Bounded local mutable diagnostic provenance; never closure ownership.
use super::super::{Checker, OwnershipCheckingError, State};
use crate::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    parser::{Expression, VariableKind},
    type_checking::UnitTypeKind,
};

impl Checker<'_> {
    pub(in super::super) fn bind_local_escape_witness(
        &self,
        kind: VariableKind,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        if kind == VariableKind::Val {
            if state
                .escape_witnesses
                .coverage
                .get(&self.unit_expression(initializer))
                == Some(&Coverage::Known)
            {
                state.escape_witnesses.known_bindings.insert(symbol);
            }
            return self.bind_escape_witness(symbol, initializer, state);
        }
        if kind == VariableKind::Var && self.mutable_escape_witness_eligible(symbol, initializer)? {
            state.escape_witnesses.mutable_locals.insert(symbol);
            self.replace_mutable_escape_witness(symbol, initializer, state);
            if self.mutable_escape_witness_has_loop_write(symbol)? {
                // Keep pre-loop reads active; `begin_loop_escape_shadow` quarantines
                // loop-written bindings on loop entry.
                state.loop_escape_shadow.seed(
                    symbol,
                    self.unit_expression(initializer),
                    &state.escape_witnesses,
                );
            }
        }
        Ok(())
    }

    fn mutable_escape_witness_eligible(
        &self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
    ) -> Result<bool, OwnershipCheckingError> {
        let callable = match self
            .typed
            .symbol_type(symbol)
            .and_then(|ty| self.typed.types().get(ty))
        {
            Some(UnitTypeKind::Function { .. }) => true,
            Some(UnitTypeKind::Nullable(inner)) => matches!(
                self.typed.types().get(*inner),
                Some(UnitTypeKind::Function { .. })
            ),
            _ => false,
        };
        if !callable
            || self.capture_index.captures_symbol(symbol)
            || self.loop_spans.contains(
                self.parsed,
                self.parsed.ast().expressions().get(initializer)?.span(),
            )
        {
            return Ok(false);
        }
        Ok(true)
    }

    fn mutable_escape_witness_has_loop_write(
        &self,
        symbol: UnitSymbolId,
    ) -> Result<bool, OwnershipCheckingError> {
        // Exclude the entire binding, not just a write that would leave stale evidence.
        // Name resolution supplies identity; syntax only bounds unsupported loop writes.
        for (_, expression) in self.parsed.ast().expressions().iter() {
            if let Expression::Assignment { target, .. } = expression.payload()
                && self.loop_spans.contains(self.parsed, expression.span())
                && self.expression_root_symbol(*target)? == Some(symbol)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(in super::super) fn replace_mutable_escape_witness(
        &self,
        symbol: UnitSymbolId,
        value: ExpressionId,
        state: &mut State,
    ) {
        if !state.escape_witnesses.mutable_locals.contains(&symbol) {
            return;
        }
        if state
            .escape_witnesses
            .coverage
            .get(&self.unit_expression(value))
            != Some(&Coverage::Known)
        {
            // Unknown completeness does not erase positive evidence from this RHS.
            // Eligibility is a stable syntactic boundary, not a value property.
            state.escape_witnesses.known_bindings.remove(&symbol);
        } else {
            state.escape_witnesses.known_bindings.insert(symbol);
        }
        let witnesses = state
            .escape_witnesses
            .results
            .get(&self.unit_expression(value))
            .cloned()
            .unwrap_or_default();
        if witnesses.is_empty() {
            state.escape_witnesses.bindings.remove(&symbol);
        } else {
            state.escape_witnesses.bindings.insert(symbol, witnesses);
        }
    }
}
