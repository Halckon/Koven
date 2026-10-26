//! Subset rules for `Inout` (SPEC-0357): scalar `Inout` parameters read and replaced in
//! the callee, and `&local` arguments at the call site. They mirror `build/inout.rs`.
use lang_frontend::{
    ast::ExpressionId, name_resolution::UnitSymbolId, ownership_checking::LoanKind,
    parser::Expression, type_checking::ParameterMode,
};

use super::Checker;

impl Checker<'_, '_> {
    /// The mode of a parameter of this function; `None` for locals.
    pub(super) fn parameter_mode(&self, symbol: UnitSymbolId) -> Option<ParameterMode> {
        let index = self
            .inputs
            .parameters
            .iter()
            .position(|parameter| *parameter == symbol)?;
        self.inputs.modes.get(index).copied()
    }

    /// `&local` for a scalar `Inout` parameter: a known local (not a parameter) whose
    /// exclusive loan the ownership facts publish for this argument.
    pub(super) fn inout_argument(&mut self, call: ExpressionId, value: ExpressionId) -> bool {
        self.migrated = true;
        let Ok(node) = self.inputs.parsed.ast().expressions().get(value) else {
            return false;
        };
        let Expression::Name = node.payload() else {
            return false;
        };
        let Some(symbol) = self.reference(value) else {
            return false;
        };
        let (call, value_id) = (self.id(call), self.id(value));
        let typed = self.inputs.typed;
        self.known.contains(&symbol)
            && self.parameter_mode(symbol).is_none()
            && typed.non_null_use(value_id).is_none()
            && typed
                .symbol_type(symbol)
                .is_some_and(|ty| self.scalar(ty) && typed.expression_type(value_id) == Some(ty))
            && self.inputs.owned.loans().iter().any(|loan| {
                loan.call() == call
                    && loan.argument() == value_id
                    && loan.kind() == LoanKind::Exclusive
            })
    }
}
