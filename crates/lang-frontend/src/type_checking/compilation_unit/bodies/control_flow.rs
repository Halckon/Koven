//! Preserve the checker result independently of statement-context Unit typing.
use super::{CompilationUnitTypes, UnitExpressionId};

impl CompilationUnitTypes {
    /// Reports whether a checked expression has a normal continuation. In particular,
    /// a statement-context exhaustive `when` can be Unit-typed yet always return.
    /// Missing expressions remain unknown; consumers must not infer divergence from absence.
    #[must_use]
    pub fn expression_falls_through(&self, expression: UnitExpressionId) -> Option<bool> {
        self.expression_falls_through.get(&expression).copied()
    }
}
