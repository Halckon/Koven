//! Diagnostic-only immutable alias provenance. Never a closure holder or loan proof.
use super::{Checker, State};
use crate::{
    name_resolution::UnitSymbolId, ownership_checking::ClosureCaptureMode,
    type_checking::UnitExpressionId,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct EscapeWitnesses {
    pub(super) bindings: BTreeMap<UnitSymbolId, BTreeSet<UnitExpressionId>>,
    pub(super) results: BTreeMap<UnitExpressionId, BTreeSet<UnitExpressionId>>,
}
impl EscapeWitnesses {
    pub(super) fn merge(&mut self, other: Self) {
        for (expression, witnesses) in other.results {
            self.results
                .entry(expression)
                .or_default()
                .extend(witnesses);
        }
        for (symbol, witnesses) in other.bindings {
            self.bindings.entry(symbol).or_default().extend(witnesses);
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
                Expression::Group { expression } => result(*expression),
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
