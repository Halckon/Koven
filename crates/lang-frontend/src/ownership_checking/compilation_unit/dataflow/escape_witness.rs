//! Diagnostic-only local alias provenance. Never a closure holder or loan proof.
use super::{Checker, State};

mod coverage;
mod loop_shadow;
pub(super) use loop_shadow::LoopEscapeShadow;
mod mutable;
use crate::{
    name_resolution::UnitSymbolId, ownership_checking::ClosureCaptureMode,
    type_checking::UnitExpressionId,
};
pub(super) use coverage::Coverage;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct EscapeWitnesses {
    pub(super) bindings: BTreeMap<UnitSymbolId, BTreeSet<UnitExpressionId>>,
    pub(super) known_bindings: BTreeSet<UnitSymbolId>,
    pub(super) coverage: BTreeMap<UnitExpressionId, Coverage>,
    pub(super) mutable_locals: BTreeSet<UnitSymbolId>,
    pub(super) results: BTreeMap<UnitExpressionId, BTreeSet<UnitExpressionId>>,
}

static EMPTY_ESCAPE_WITNESSES: EscapeWitnesses = EscapeWitnesses {
    bindings: BTreeMap::new(),
    known_bindings: BTreeSet::new(),
    coverage: BTreeMap::new(),
    mutable_locals: BTreeSet::new(),
    results: BTreeMap::new(),
};

#[derive(Clone, Debug, Default)]
pub(super) struct EscapeWitnessSlot(Option<Box<EscapeWitnesses>>);

impl PartialEq for EscapeWitnessSlot {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl Eq for EscapeWitnessSlot {}

impl std::ops::Deref for EscapeWitnessSlot {
    type Target = EscapeWitnesses;

    fn deref(&self) -> &Self::Target {
        self.0.as_deref().unwrap_or(&EMPTY_ESCAPE_WITNESSES)
    }
}

impl std::ops::DerefMut for EscapeWitnessSlot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.get_or_insert_with(Default::default)
    }
}

impl EscapeWitnessSlot {
    pub(super) fn merge_preserving(
        &mut self,
        other: Self,
        shared_candidates: Option<&BTreeSet<UnitSymbolId>>,
    ) {
        match (&mut self.0, other.0) {
            (Some(left), Some(right)) => left.merge_preserving(*right, shared_candidates),
            (Some(left), None) => {
                left.merge_preserving(EscapeWitnesses::default(), shared_candidates)
            }
            (left @ None, Some(right)) => {
                let mut merged = EscapeWitnesses::default();
                merged.merge_preserving(*right, shared_candidates);
                if merged != EMPTY_ESCAPE_WITNESSES {
                    *left = Some(Box::new(merged));
                }
            }
            (None, None) => {}
        }
    }
}
impl EscapeWitnesses {
    #[cfg(test)]
    pub(super) fn merge(&mut self, other: Self) {
        self.merge_preserving(other, None);
    }

    pub(super) fn merge_preserving(
        &mut self,
        other: Self,
        shared_candidates: Option<&BTreeSet<UnitSymbolId>>,
    ) {
        self.known_bindings
            .retain(|symbol| other.known_bindings.contains(symbol));
        for (expression, coverage) in other.coverage {
            self.coverage
                .entry(expression)
                .and_modify(|current| {
                    if coverage == Coverage::Unknown {
                        *current = Coverage::Unknown;
                    }
                })
                .or_insert(coverage);
        }
        let mutable_symbols: BTreeSet<_> = self
            .mutable_locals
            .union(&other.mutable_locals)
            .copied()
            .collect();
        self.mutable_locals
            .retain(|symbol| other.mutable_locals.contains(symbol));
        for (expression, witnesses) in other.results {
            self.results
                .entry(expression)
                .or_default()
                .extend(witnesses);
        }
        for (symbol, witnesses) in other.bindings {
            self.bindings.entry(symbol).or_default().extend(witnesses);
        }
        for symbol in mutable_symbols.difference(&self.mutable_locals) {
            if !shared_candidates.is_some_and(|candidates| candidates.contains(symbol)) {
                self.bindings.remove(symbol);
            }
        }
    }
}
impl Checker<'_> {
    /// Freeze actual borrowed leaves while the formed environment graph still exists.
    /// Later source rebinding or cleanup must not change this diagnostic evidence.
    pub(super) fn freeze_escape_witnesses(
        &self,
        roots: impl IntoIterator<Item = UnitExpressionId>,
        state: &State,
    ) -> BTreeSet<UnitExpressionId> {
        state
            .closure_tree(roots)
            .into_iter()
            .filter(|lambda| {
                self.capture_index
                    .for_lambda(*lambda)
                    .any(|(_, capture)| capture.mode() == ClosureCaptureMode::Shared)
            })
            .collect()
    }
}

impl Checker<'_> {
    pub(super) fn escape_result_witnesses(
        &self,
        expression: crate::ast::ExpressionId,
        state: &State,
    ) -> Result<BTreeSet<UnitExpressionId>, super::OwnershipCheckingError> {
        use crate::parser::Expression;
        let result = |id| {
            state
                .escape_witnesses
                .results
                .get(&self.unit_expression(id))
                .cloned()
                .unwrap_or_default()
        };
        Ok(
            match self.parsed.ast().expressions().get(expression)?.payload() {
                Expression::Lambda { .. } => result(expression),
                Expression::Group { expression } => result(*expression),
                Expression::NonNullAssert { operand, .. } => result(*operand),
                Expression::Binary {
                    left,
                    operator: crate::parser::BinaryOperator::Elvis,
                    right,
                    ..
                } => {
                    let mut witnesses = result(*left);
                    witnesses.extend(result(*right));
                    witnesses
                }
                Expression::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    let mut witnesses = self.escape_tail_witnesses(*then_branch, state)?;
                    if let Some(branch) = else_branch {
                        witnesses.extend(self.escape_tail_witnesses(*branch, state)?);
                    }
                    witnesses
                }
                Expression::When { entries, .. } => {
                    let mut witnesses = BTreeSet::new();
                    for entry in entries {
                        witnesses.extend(self.escape_tail_witnesses(entry.body, state)?);
                    }
                    witnesses
                }
                _ => BTreeSet::new(),
            },
        )
    }
    fn escape_tail_witnesses(
        &self,
        statement: crate::ast::StatementId,
        state: &State,
    ) -> Result<BTreeSet<UnitExpressionId>, super::OwnershipCheckingError> {
        use crate::parser::Statement;
        Ok(
            match self.parsed.ast().statements().get(statement)?.payload() {
                Statement::Expression { expression } => state
                    .escape_witnesses
                    .results
                    .get(&self.unit_expression(*expression))
                    .cloned()
                    .unwrap_or_default(),
                Statement::ControlBody { elements } | Statement::LambdaBody { elements } => {
                    match elements.last() {
                        Some(tail) => self.escape_tail_witnesses(*tail, state)?,
                        None => BTreeSet::new(),
                    }
                }
                _ => BTreeSet::new(),
            },
        )
    }
    pub(super) fn seed_capture_body_witness(
        &self,
        lambda: crate::ast::ExpressionId,
        capture: &crate::ownership_checking::UnitClosureCaptureDescriptor,
        entry_state: &State,
        body_state: &mut State,
    ) -> Result<(), super::OwnershipCheckingError> {
        use crate::{
            name_resolution::SymbolKind, ownership_checking::UnitClosureCaptureSource,
            parser::VariableKind,
        };
        if let UnitClosureCaptureSource::Symbol(symbol) = capture.source()
            && capture.mode() == ClosureCaptureMode::Owned
            && self.variable_kinds.get(&symbol) == Some(&VariableKind::Val)
            && self.symbol_kind(symbol) == Some(SymbolKind::Variable)
            && !self.loop_spans.contains(
                self.parsed,
                self.parsed.ast().expressions().get(lambda)?.span(),
            )
            && let Some(witnesses) = entry_state.escape_witnesses.bindings.get(&symbol)
            && !witnesses.is_empty()
        {
            body_state
                .escape_witnesses
                .bindings
                .insert(symbol, witnesses.clone());
        }
        Ok(())
    }

    pub(super) fn bind_escape_witness(
        &self,
        symbol: UnitSymbolId,
        initializer: crate::ast::ExpressionId,
        state: &mut State,
    ) -> Result<(), super::OwnershipCheckingError> {
        let expression = self.unit_expression(initializer);
        let span = self.parsed.ast().expressions().get(initializer)?.span();
        if !self.loop_spans.contains(self.parsed, span) {
            let witnesses = state
                .escape_witnesses
                .results
                .get(&expression)
                .cloned()
                .unwrap_or_default();
            if !witnesses.is_empty() {
                state.escape_witnesses.bindings.insert(symbol, witnesses);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
