//! Publish only complete facts after all ordinary bodies and trial rollback have finished.
use super::BodyChecker;
use crate::type_checking::{
    ExpressionCategory, UnitConstantDescriptor, UnitConstantFacts, UnitConstantUseDescriptor,
};
impl BodyChecker<'_> {
    pub(super) fn build_constant_facts(&self) -> Option<UnitConstantFacts> {
        if !self.diagnostics.is_empty()
            || !self.signatures.diagnostics().is_empty()
            || self.parts.constant_declaration_count != self.constant_inputs.len()
            || self.constant_inputs.len() != self.constant_values.len()
        {
            return None;
        }
        let mut declarations = Vec::new();
        for (&symbol, (span, dependencies)) in &self.constant_inputs {
            declarations.push(UnitConstantDescriptor {
                symbol,
                declaration_span: *span,
                ty: *self.parts.symbol_types.get(&symbol)?,
                value: self.constant_values.get(&symbol)?.clone(),
                dependencies: dependencies.clone(),
            });
        }
        let mut uses = Vec::new();
        for (&expression, &target) in &self.parts.constant_selections {
            let ty = *self.parts.expression_types.get(&expression)?;
            if self.parts.symbol_types.get(&target) != Some(&ty)
                || self.parts.expression_categories.get(&expression)
                    != Some(&ExpressionCategory::Temporary)
            {
                return None;
            }
            uses.push(UnitConstantUseDescriptor {
                expression,
                target,
                ty,
                value: self.constant_values.get(&target)?.clone(),
            });
        }
        Some(UnitConstantFacts { declarations, uses })
    }
}
